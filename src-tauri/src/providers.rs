//! Adapter HTTP dos providers + orquestrador de resiliencia.
//! A ramificacao (classify/plan) vive em `ember_core`; aqui so ha I/O.

use ember_core::error::{CoreError, OutcomeClass};
use ember_core::model::{LlmRequest, LlmResponse, Provider};
use ember_core::providers::{self as wire, ClaudeStreamEvent};
use ember_core::retry::{classify, plan, Decision, LoopState, RetryConfig};
use futures_util::StreamExt;
use reqwest::Client;
use serde_json::{json, Value};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Quanto tempo esperar por bytes novos do stream antes de desistir. NAO e um teto na
/// duracao TOTAL da resposta (que pode legitimamente demorar minutos com thinking pesado:
/// ver `AppState::new`), so deteta uma ligacao presa a meio, sem trafego.
const STREAM_STALL_TIMEOUT: Duration = Duration::from_secs(25);

/// Fonte barata de jitter em [0,1) sem dependencia de `rand`: os nanos do relogio bastam
/// para desalinhar retries concorrentes (evitar thundering-herd). Nao e criptografico.
fn jitter01() -> f64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    f64::from(nanos) / 1_000_000_000.0
}

fn retry_after_ms(resp: &reqwest::Response) -> Option<u64> {
    resp.headers()
        .get(reqwest::header::RETRY_AFTER)?
        .to_str()
        .ok()?
        .parse::<u64>()
        .ok()
        .map(|s| s.saturating_mul(1000))
}

/// Uma tentativa contra um provider, sempre em streaming. `Ok(texto)` = sucesso (texto
/// completo acumulado); `Err(outcome)` = a classificar. `on_delta` recebe cada tranche de
/// texto assim que chega, para o overlay mostrar progresso real em vez de um orb mudo.
async fn call_once(
    client: &Client,
    provider: Provider,
    key: &str,
    req: &LlmRequest,
    on_delta: &(dyn Fn(&str) + Send + Sync),
) -> Result<String, OutcomeClass> {
    let builder = match provider {
        Provider::Gemini => client
            .post(wire::gemini_url(&req.model, true))
            .header("x-goog-api-key", key)
            .json(&wire::gemini_request_body(req)),
        Provider::Claude => client
            .post(wire::claude_url())
            .header("x-api-key", key)
            .header("anthropic-version", wire::ANTHROPIC_VERSION)
            .json(&wire::claude_request_body(req, true)),
    };

    let resp = match builder.send().await {
        Ok(r) => r,
        Err(_) => {
            return Err(OutcomeClass::Transient {
                retry_after_ms: None,
            })
        }
    };

    let status = resp.status().as_u16();
    let ra = retry_after_ms(&resp);

    match classify(provider, status, None, ra) {
        OutcomeClass::Success => consume_stream(provider, resp, on_delta).await,
        // Nao-200: mesmo com stream:true, um erro (auth/payload/rate-limit) chega como um
        // JSON normal, nao SSE, por isso lemos o corpo inteiro aqui (so neste ramo).
        outcome => {
            let body: Option<Value> = resp.json().await.ok();
            match outcome {
                // Chave Gemini invalida vem como 400 (classificado Payload). Reclassifica
                // como Auth para disparar o fallback: a outra familia tem chave diferente.
                OutcomeClass::Payload
                    if provider == Provider::Gemini
                        && body.as_ref().map(wire::gemini_is_invalid_key).unwrap_or(false) =>
                {
                    Err(OutcomeClass::Auth)
                }
                // O header Retry-After nao veio, mas a Gemini pode sugerir o atraso no
                // corpo (RetryInfo). Sem isto, o backoff exponencial cego ignorava o valor
                // que o proprio servidor recomenda.
                OutcomeClass::Transient { retry_after_ms: None } if provider == Provider::Gemini => {
                    let body_ra = body.as_ref().and_then(wire::gemini_retry_delay_ms);
                    Err(OutcomeClass::Transient {
                        retry_after_ms: body_ra,
                    })
                }
                other => Err(other),
            }
        }
    }
}

/// Consome o corpo SSE de uma resposta 200 ate ao fim, acumulando o texto e chamando
/// `on_delta` a cada tranche nova. Deteta truncamento/politica a partir dos proprios eventos
/// do stream (mesmas regras que a resposta completa, aplicadas por chunk). Um watchdog de
/// stall (`STREAM_STALL_TIMEOUT`) trata uma ligacao presa sem trafego como transitorio.
async fn consume_stream(
    provider: Provider,
    resp: reqwest::Response,
    on_delta: &(dyn Fn(&str) + Send + Sync),
) -> Result<String, OutcomeClass> {
    let mut stream = resp.bytes_stream();
    let mut byte_buf: Vec<u8> = Vec::new();
    let mut text_acc = String::new();

    loop {
        let chunk = match tokio::time::timeout(STREAM_STALL_TIMEOUT, stream.next()).await {
            Ok(Some(Ok(bytes))) => bytes,
            Ok(Some(Err(_))) => {
                return Err(OutcomeClass::Transient {
                    retry_after_ms: None,
                })
            }
            Ok(None) => break, // EOF: o provider fechou a ligacao normalmente.
            Err(_) => {
                // Stall: nenhum byte novo dentro do timeout. Trata como transitorio; o
                // retry ou o fallback tentam de novo (o `select!` em flow.rs continua a
                // poder cancelar isto a qualquer momento, independentemente deste timeout).
                return Err(OutcomeClass::Transient {
                    retry_after_ms: None,
                });
            }
        };

        byte_buf.extend_from_slice(&chunk);
        let (events, rest) = wire::split_sse_events(&byte_buf);
        byte_buf = rest;

        for event_block in &events {
            for data in wire::parse_sse_data_lines(event_block) {
                let Ok(v) = serde_json::from_str::<Value>(data) else {
                    continue;
                };
                match provider {
                    Provider::Gemini => {
                        if wire::gemini_is_content_policy(&v) {
                            return Err(OutcomeClass::ContentPolicy);
                        }
                        if wire::gemini_is_truncated(&v) {
                            return Err(OutcomeClass::Truncated);
                        }
                        if let Some(delta) = wire::gemini_stream_text_delta(&v) {
                            on_delta(&delta);
                            text_acc.push_str(&delta);
                        }
                    }
                    Provider::Claude => match wire::claude_stream_event(&v) {
                        ClaudeStreamEvent::TextDelta(delta) => {
                            on_delta(&delta);
                            text_acc.push_str(&delta);
                        }
                        ClaudeStreamEvent::Stopped { stop_reason } => {
                            let fake = json!({ "stop_reason": stop_reason });
                            if wire::claude_is_content_policy(&fake) {
                                return Err(OutcomeClass::ContentPolicy);
                            }
                            if wire::claude_is_truncated(&fake) {
                                return Err(OutcomeClass::Truncated);
                            }
                        }
                        ClaudeStreamEvent::Other => {}
                    },
                }
            }
        }
    }

    // Sem texto acumulado (stream terminou sem nenhuma tranche util): mesmo tratamento que
    // uma resposta vazia na versao nao-streaming, transitorio, retry/fallback tratam.
    if text_acc.trim().is_empty() {
        Err(OutcomeClass::Transient {
            retry_after_ms: None,
        })
    } else {
        Ok(text_acc)
    }
}

/// Refina com resiliencia: retry transitorio + fallback no esgotamento. A decisao e pura.
/// `on_attempt(provider, provider_index, attempt)` e chamado antes de cada tentativa, para o
/// shell dar feedback visivel ("Trying Claude...", "Retrying...") durante esperas longas.
/// `on_delta(texto)` e chamado a cada tranche de texto que chega do stream.
pub async fn refine(
    client: &Client,
    cfg: &RetryConfig,
    chain: &[(Provider, String)],
    base_req: &LlmRequest,
    gemini_model: &str,
    claude_model: &str,
    on_attempt: &(dyn Fn(Provider, usize, u32) + Send + Sync),
    on_delta: &(dyn Fn(&str) + Send + Sync),
) -> Result<LlmResponse, CoreError> {
    if chain.is_empty() {
        return Err(CoreError::NoProvidersConfigured);
    }
    let mut state = LoopState::start();
    loop {
        let (provider, key) = &chain[state.provider_index];
        let model = match provider {
            Provider::Gemini => gemini_model,
            Provider::Claude => claude_model,
        };
        let mut req = base_req.clone();
        req.model = model.to_string();

        on_attempt(*provider, state.provider_index, state.attempt);
        match call_once(client, *provider, key, &req, on_delta).await {
            Ok(text) => {
                return Ok(LlmResponse {
                    text,
                    provider: *provider,
                })
            }
            Err(outcome) => match plan(&state, &outcome, cfg, jitter01()) {
                Decision::Retry { delay_ms, next } => {
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                    state = next;
                }
                Decision::Fallback { next } => state = next,
                Decision::Fail { reason } => return Err(reason),
                Decision::Succeed => return Err(CoreError::EmptyResponse),
            },
        }
    }
}

/// Resultado do probe de validacao de chave. Distinto de um simples `bool`: uma falha de
/// rede (sem ligacao no momento) nao diz NADA sobre se a chave e valida, e colapsar os dois
/// em `false` fazia uma chave boa parecer invalida so por estar offline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyCheck {
    Valid,
    Invalid,
    NetworkError,
}

/// Probe barato de validacao de chave (pre-validacao).
pub async fn validate(client: &Client, provider: Provider, key: &str) -> KeyCheck {
    let result = match provider {
        Provider::Gemini => {
            client
                .get("https://generativelanguage.googleapis.com/v1beta/models")
                .header("x-goog-api-key", key)
                .send()
                .await
        }
        Provider::Claude => {
            client
                .get("https://api.anthropic.com/v1/models")
                .header("x-api-key", key)
                .header("anthropic-version", wire::ANTHROPIC_VERSION)
                .send()
                .await
        }
    };
    match result {
        Ok(resp) if resp.status().is_success() => KeyCheck::Valid,
        // Qualquer resposta HTTP (401/403/etc.) e o provider a recusar a chave.
        Ok(_) => KeyCheck::Invalid,
        // Falha de transporte (sem rede, DNS, timeout): nao diz nada sobre a chave.
        Err(_) => KeyCheck::NetworkError,
    }
}
