//! Adapter HTTP dos providers + orquestrador de resiliencia.
//! A ramificacao (classify/plan) vive em `ember_core`; aqui so ha I/O.

use ember_core::codex;
use ember_core::error::{CoreError, OutcomeClass};
use ember_core::health::KeyCheck;
use ember_core::model::{LlmRequest, LlmResponse, Provider};
use ember_core::models::ModelInfo;
use ember_core::providers::{self as wire, OpenAiStreamEvent};
use ember_core::retry::{classify, plan, Decision, LoopState, RetryConfig};
use futures_util::StreamExt;
use reqwest::Client;
use serde_json::{json, Value};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Base URL do slot OpenAI-compatible, passada ao `refine`/`call_once`/`validate`. Interno do
/// shell (a decisao de resiliencia vive no core).
///
/// Os modelos ja NAO vivem aqui: passaram para cada passo da cadeia (`ChainStep`), porque o mesmo
/// provider pode ser tentado com modelos diferentes na mesma cadeia.
pub struct ProviderCtx<'a> {
    pub on_response: Option<&'a (dyn Fn() + Send + Sync)>,
    pub openai_base_url: &'a str,
}

/// Como um passo se autentica.
///
/// O token da subscricao ja vem resolvido (renovado se preciso) de quem construiu a cadeia, e nao
/// e ido buscar aqui: assim o caminho do refine nao tem de saber nada sobre OAuth, e a renovacao
/// acontece uma vez por refine em vez de uma vez por tentativa.
#[derive(Clone)]
pub enum Credential {
    Key(String),
    ChatGpt {
        access_token: String,
        account_id: Option<String>,
    },
}

/// Um passo da cadeia de tentativa: contra que provider, com que credencial, com que modelo.
///
/// A cadeia deixou de ser "um passo por provider" e passou a ser "um passo por par
/// (provider, modelo)". E o que permite trocar de modelo dentro da mesma familia quando o
/// escolhido esta sobrecarregado, em vez de dar a familia inteira por perdida (ver
/// `ember_core::models::alternates`).
pub struct ChainStep {
    pub provider: Provider,
    pub credential: Credential,
    pub model: String,
}

/// Que protocolo se fala neste pedido. Nao e o mesmo que o provider: o slot OpenAI fala
/// chat-completions com uma chave e a Responses API do backend do ChatGPT com uma sessao, e os
/// dois streams tem formas diferentes (um traz `choices[].delta`, o outro eventos com `type`).
/// Confundi-los colava raciocinio por cima do texto do utilizador, por isso sao arms distintos.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Wire {
    Gemini,
    OpenAiChat,
    Codex,
}

fn wire_of(provider: Provider, credential: &Credential) -> Wire {
    match (provider, credential) {
        (Provider::Gemini, _) => Wire::Gemini,
        (Provider::OpenAi, Credential::ChatGpt { .. }) => Wire::Codex,
        (Provider::OpenAi, Credential::Key(_)) => Wire::OpenAiChat,
    }
}

/// Identificador de sessao para o backend do ChatGPT, que o espera em cada pedido. Aleatorio por
/// pedido: nao ha conversa a manter (cada refine e independente) e um id fixo so serviria para
/// nos correlacionar entre si.
fn session_id() -> String {
    let mut b = [0u8; 16];
    if getrandom::getrandom(&mut b).is_err() {
        return "00000000-0000-4000-8000-000000000000".into();
    }
    b[6] = (b[6] & 0x0f) | 0x40; // versao 4
    b[8] = (b[8] & 0x3f) | 0x80; // variante RFC 4122
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &h[0..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..32]
    )
}

/// Quanto tempo esperar por bytes novos do stream antes de desistir. NAO e um teto na
/// duracao TOTAL da resposta (que pode legitimamente demorar minutos com thinking pesado:
/// ver `AppState::new`), so deteta uma ligacao presa a meio, sem trafego.
const STREAM_STALL_TIMEOUT: Duration = Duration::from_secs(60);

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
    credential: &Credential,
    req: &LlmRequest,
    pctx: &ProviderCtx<'_>,
    on_delta: &(dyn Fn(&str) + Send + Sync),
) -> Result<String, OutcomeClass> {
    let protocol = wire_of(provider, credential);
    if matches!(protocol, Wire::OpenAiChat)
        && crate::connection::ProviderConnection::parse(pctx.openai_base_url).is_err()
    {
        return Err(OutcomeClass::Payload);
    }
    let build = |endpoint: &str| match (protocol, credential) {
        (Wire::Gemini, Credential::Key(key)) => Some(
            client
                .post(wire::gemini_url(&req.model, true))
                .header("x-goog-api-key", key)
                .json(&wire::gemini_request_body(req)),
        ),
        (Wire::OpenAiChat, Credential::Key(key)) => Some(
            client
                .post(wire::openai_chat_url(pctx.openai_base_url))
                .header("Authorization", format!("Bearer {key}"))
                .json(&wire::openai_request_body(req, true, pctx.openai_base_url)),
        ),
        (
            Wire::Codex,
            Credential::ChatGpt {
                access_token,
                account_id,
            },
        ) => {
            let mut b = client
                .post(endpoint)
                .header("Authorization", format!("Bearer {access_token}"))
                .header("OpenAI-Beta", "responses=experimental")
                // O backend identifica o cliente por este header e recusa o que nao conhece. Tem
                // de ser o MESMO valor que foi no `originator` do login (mesma constante, para
                // nao poderem divergir sem alguem dar por isso).
                .header("originator", codex::ORIGINATOR)
                .header("session_id", session_id())
                .header("Accept", "text/event-stream")
                .json(&codex::codex_request_body(req));
            if let Some(acc) = account_id {
                // Sem isto o pedido nao sabe que subscricao cobrar e volta 401.
                b = b.header("ChatGPT-Account-Id", acc.as_str());
            }
            Some(b)
        }
        // Combinacoes que a construcao da cadeia nunca produz (uma sessao ChatGPT no Gemini, uma
        // chave no backend do ChatGPT). Se aparecer uma, e bug nosso: propaga sem mascarar.
        _ => None,
    };

    // O caminho do backend do ChatGPT nao esta documentado e os clientes que o usam nao concordam
    // sobre o nome (`wham` ou `codex`). Um 404 no primeiro nao e uma avaria: e o outro nome. Nos
    // outros protocolos ha um so endpoint e este ciclo corre exatamente uma vez.
    let endpoints: &[&str] = match protocol {
        Wire::Codex => &codex::CODEX_RESPONSES_URLS,
        _ => &[""],
    };
    let mut resp = None;
    for (i, endpoint) in endpoints.iter().enumerate() {
        let Some(builder) = build(endpoint) else {
            log::error!("bug: passo com credencial que nao serve para {provider:?}");
            return Err(OutcomeClass::Payload);
        };
        let r = match tokio::time::timeout(std::time::Duration::from_secs(30), builder.send()).await
        {
            Ok(Ok(r)) => r,
            _ => return Err(OutcomeClass::Uncertain),
        };
        if let Some(received) = pctx.on_response {
            received();
        }
        let last = i + 1 == endpoints.len();
        if r.status().as_u16() == 404 && !last {
            log::info!("codex: {endpoint} respondeu 404; a tentar o caminho alternativo");
            continue;
        }
        resp = Some(r);
        break;
    }
    let resp = match resp {
        Some(r) => r,
        None => return Err(OutcomeClass::ModelNotFound),
    };

    let status = resp.status().as_u16();
    let ra = retry_after_ms(&resp);

    match classify(provider, status, None, ra) {
        OutcomeClass::Success => consume_stream(protocol, resp, on_delta).await,
        // Nao-200: mesmo com stream:true, um erro (auth/payload/rate-limit) chega como um
        // JSON normal, nao SSE, por isso lemos o corpo inteiro aqui (so neste ramo).
        outcome => {
            let body: Option<Value> = bounded_json(resp).await;
            // O corpo do erro era lido e deitado fora: ficavamos a saber a CLASSE (rate-limit)
            // mas nunca o motivo que o provider explica ("free-models-per-day", "requires
            // credits", "model not found"...). Sem isto e impossivel dizer ao utilizador o que
            // fazer. Nao ha segredos aqui: e a mensagem de erro do provider, nunca a chave nem o
            // texto do utilizador. Truncado, para um corpo grande nao inundar o log.
            log::warn!("{provider:?} request failed with HTTP {status}");
            match outcome {
                // Chave Gemini invalida vem como 400 (classificado Payload). Reclassifica
                // como Auth para disparar o fallback: a outra familia tem chave diferente.
                OutcomeClass::Payload
                    if provider == Provider::Gemini
                        && body
                            .as_ref()
                            .map(wire::gemini_is_invalid_key)
                            .unwrap_or(false) =>
                {
                    Err(OutcomeClass::Auth)
                }
                // O header Retry-After nao veio, mas a Gemini pode sugerir o atraso no
                // corpo (RetryInfo). Sem isto, o backoff exponencial cego ignorava o valor
                // que o proprio servidor recomenda.
                // 503 "high demand": e o MODELO que esta cheio, nao o servico. Repetir no mesmo
                // modelo devolve o mesmo, por isso vale mais ir ja para o passo seguinte.
                OutcomeClass::Transient { .. }
                    if provider == Provider::Gemini
                        && body
                            .as_ref()
                            .map(wire::gemini_is_overloaded)
                            .unwrap_or(false) =>
                {
                    Err(OutcomeClass::Overloaded)
                }
                OutcomeClass::Transient {
                    retry_after_ms: None,
                } if provider == Provider::Gemini => {
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
    protocol: Wire,
    resp: reqwest::Response,
    on_delta: &(dyn Fn(&str) + Send + Sync),
) -> Result<String, OutcomeClass> {
    let mut stream = resp.bytes_stream();
    let mut byte_buf: Vec<u8> = Vec::new();
    let mut text_acc = String::new();
    let mut complete = false;
    let mut received = 0usize;
    let mut last_progress = std::time::Instant::now();

    loop {
        let chunk = match tokio::time::timeout(
            STREAM_STALL_TIMEOUT.saturating_sub(last_progress.elapsed()),
            stream.next(),
        )
        .await
        {
            Ok(Some(Ok(bytes))) => bytes,
            Ok(Some(Err(_))) => return Err(OutcomeClass::Uncertain),
            Ok(None) => break, // EOF: o provider fechou a ligacao normalmente.
            Err(_) => {
                // Stall: nenhum byte novo dentro do timeout. Trata como transitorio; o
                // retry ou o fallback tentam de novo (o `select!` em flow.rs continua a
                // poder cancelar isto a qualquer momento, independentemente deste timeout).
                return Err(OutcomeClass::Uncertain);
            }
        };

        received = received.saturating_add(chunk.len());
        if received > 8 * 1024 * 1024 {
            return Err(OutcomeClass::Payload);
        }
        byte_buf.extend_from_slice(&chunk);
        let (events, rest) = wire::split_sse_events(&byte_buf);
        byte_buf = rest;

        for event_block in &events {
            for data in wire::parse_sse_data_lines(event_block) {
                let Ok(v) = serde_json::from_str::<Value>(data) else {
                    continue;
                };
                match protocol {
                    Wire::Gemini => {
                        if wire::gemini_is_content_policy(&v) {
                            return Err(OutcomeClass::ContentPolicy);
                        }
                        if wire::gemini_is_truncated(&v) {
                            return Err(OutcomeClass::Truncated);
                        }
                        if v.pointer("/candidates/0/finishReason")
                            .and_then(Value::as_str)
                            == Some("STOP")
                        {
                            complete = true;
                        }
                        if let Some(delta) = wire::gemini_stream_text_delta(&v) {
                            last_progress = std::time::Instant::now();
                            on_delta(&delta);
                            text_acc.push_str(&delta);
                        }
                    }
                    // Responses API: eventos tipados, e nao `choices[].delta`. O resumo do
                    // raciocinio chega no MESMO stream e com a mesma forma que a resposta, por
                    // isso e aqui que se garante que so o texto final e acumulado.
                    Wire::Codex => match codex::codex_stream_event(&v) {
                        codex::CodexStreamEvent::TextDelta(delta) => {
                            last_progress = std::time::Instant::now();
                            on_delta(&delta);
                            text_acc.push_str(&delta);
                        }
                        codex::CodexStreamEvent::ReasoningDelta(r) => {
                            log::debug!("codex reasoning delta: {} chars", r.len());
                        }
                        codex::CodexStreamEvent::Completed {
                            status,
                            incomplete_reason,
                        } => {
                            if codex::codex_is_truncated(&status, incomplete_reason.as_deref()) {
                                return Err(OutcomeClass::Truncated);
                            }
                            // TERMINAL, sem esperar pelo EOF. Se este backend mantiver a ligacao
                            // aberta depois de completar (nao ha documentacao que o desminta), uma
                            // resposta boa ficava presa ate o watchdog de stall disparar e virava
                            // um retry de 60 segundos. Com texto acumulado, acabou aqui.
                            if !text_acc.trim().is_empty() {
                                if status != "completed" {
                                    return Err(OutcomeClass::Truncated);
                                }
                                return Ok(text_acc);
                            }
                        }
                        // Recusa do modelo: nao e transitorio (repetir da a mesma recusa) e nao e
                        // um erro nosso. Vai por ContentPolicy, que ja tem mensagem propria e nao
                        // manda o utilizador procurar um problema de chave ou de rede.
                        codex::CodexStreamEvent::Refusal(reason) => {
                            let _ = reason;
                            log::warn!("codex: content policy refusal");
                            return Err(OutcomeClass::ContentPolicy);
                        }
                        codex::CodexStreamEvent::Failed { message } => {
                            log::warn!("codex: stream failed");
                            return Err(if codex::codex_is_content_policy(&message) {
                                OutcomeClass::ContentPolicy
                            } else {
                                OutcomeClass::Uncertain
                            });
                        }
                        codex::CodexStreamEvent::Other => {}
                    },
                    Wire::OpenAiChat => match wire::openai_stream_event(&v) {
                        OpenAiStreamEvent::ContentDelta(delta) => {
                            last_progress = std::time::Instant::now();
                            on_delta(&delta);
                            text_acc.push_str(&delta);
                        }
                        // Raciocinio (DeepSeek R1 / Qwen3): NUNCA para o text_acc. So cola a
                        // resposta final por cima da seleccao, igual ao `thought:true` da Gemini.
                        OpenAiStreamEvent::ReasoningDelta(r) => {
                            log::debug!("openai reasoning delta: {} chars", r.len());
                        }
                        OpenAiStreamEvent::Stopped { finish_reason } => {
                            if let Some(delta) = v
                                .pointer("/choices/0/delta/content")
                                .and_then(Value::as_str)
                            {
                                text_acc.push_str(delta);
                                on_delta(delta);
                            }
                            complete = finish_reason == "stop";
                            let fake = json!({ "choices": [{ "finish_reason": finish_reason }] });
                            if wire::openai_is_content_policy(&fake) {
                                return Err(OutcomeClass::ContentPolicy);
                            }
                            if wire::openai_is_truncated(&fake) {
                                return Err(OutcomeClass::Truncated);
                            }
                        }
                        OpenAiStreamEvent::Other => {}
                    },
                }
            }
        }
        if complete && !text_acc.trim().is_empty() {
            return Ok(text_acc);
        }
    }

    // Sem texto acumulado (stream terminou sem nenhuma tranche util): mesmo tratamento que
    // uma resposta vazia na versao nao-streaming, transitorio, retry/fallback tratam.
    if !complete || text_acc.trim().is_empty() {
        Err(OutcomeClass::Uncertain)
    } else {
        Ok(text_acc)
    }
}

/// Refina com resiliencia: retry transitorio + fallback no esgotamento. A decisao e pura.
/// `on_attempt(provider, step_index, attempt)` e chamado antes de cada tentativa, para o
/// shell dar feedback visivel ("Trying Claude...", "Retrying...") durante esperas longas.
/// `on_delta(texto)` e chamado a cada tranche de texto que chega do stream.
pub async fn refine(
    client: &Client,
    cfg: &RetryConfig,
    chain: &[ChainStep],
    base_req: &LlmRequest,
    pctx: &ProviderCtx<'_>,
    on_attempt: &(dyn Fn(Provider, usize, u32) + Send + Sync),
    on_delta: &(dyn Fn(&str) + Send + Sync),
) -> Result<LlmResponse, CoreError> {
    if chain.is_empty() {
        return Err(CoreError::NoProvidersConfigured);
    }
    let mut state = LoopState::start();
    loop {
        // `get` e nao indexacao: o `step_count` do `RetryConfig` e a forma da cadeia sao dois
        // valores que TEM de bater certo, e quem os faz bater e o chamador. Um default do
        // `RetryConfig` que sobrevivesse a uma cadeia mais curta entrava aqui a indexar fora e
        // matava a app. Falhar honestamente e sempre melhor do que um panico no caminho do refine.
        let Some(step) = chain.get(state.step_index) else {
            log::error!(
                "bug: passo {} fora de uma cadeia de {}",
                state.step_index,
                chain.len()
            );
            return Err(CoreError::AllProvidersFailed);
        };
        let (provider, model) = (step.provider, &step.model);
        let mut req = base_req.clone();
        req.model = model.clone();

        on_attempt(provider, state.step_index, state.attempt);
        match call_once(client, provider, &step.credential, &req, pctx, on_delta).await {
            Ok(text) => {
                log::info!(
                    "provider {:?} ok (model={model} attempt={})",
                    provider,
                    state.attempt
                );
                return Ok(LlmResponse {
                    text,
                    provider,
                    model: model.clone(),
                });
            }
            // Cada tentativa falhada era engolida em silencio: a overlay dizia "provider error"
            // e o log nao tinha rasto nenhum de qual provider falhou nem porque. Logamos o
            // outcome (ja e um enum sem segredos, nao o corpo cru) e a decisao da maquina.
            Err(outcome) => {
                log::warn!(
                    "provider {:?} failed (model={model} attempt={}): {outcome:?}",
                    provider,
                    state.attempt
                );
                match plan(&state, &outcome, cfg, jitter01()) {
                    Decision::Retry { delay_ms, next } => {
                        log::info!("retrying {:?} in {delay_ms}ms", provider);
                        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                        state = next;
                    }
                    Decision::Fallback { next } => {
                        // Diz QUAL e o passo seguinte: outro modelo da mesma familia e a outra
                        // familia sao decisoes diferentes, e sem isto o log dizia sempre o mesmo.
                        let to = &chain[next.step_index];
                        log::info!("falling back to {:?} with model {}", to.provider, to.model);
                        state = next;
                    }
                    Decision::Fail { reason } => {
                        log::error!("chain exhausted: {reason:?}");
                        return Err(reason);
                    }
                    Decision::Succeed => return Err(CoreError::EmptyResponse),
                }
            }
        }
    }
}

/// O que um probe descobriu: se a chave serve, e que modelos o provider diz servir hoje.
///
/// Os dois vem do MESMO pedido de proposito. O probe ja batia em `GET /models` para validar a
/// chave e deitava o corpo fora; agora aproveita-o. Zero pedidos extra, e a listagem chega com a
/// mesma frescura (e o mesmo TTL) do resultado da validacao.
#[derive(Debug, Clone)]
pub struct Probe {
    pub check: KeyCheck,
    /// Vazia quando a chave nao serve, quando a rede falhou, ou quando o endpoint nao publica
    /// `/models` (um Ollama local, por exemplo). Vazia significa "nao sei", nunca "nao ha
    /// nenhum": `ember_core::models::reconcile` trata as duas coisas de forma diferente.
    pub models: Vec<ModelInfo>,
}

/// Probe barato de validacao de chave (pre-validacao). `KeyCheck` vive em `ember_core::health`.
/// O probe bate num endpoint diferente do `refine` (GET /models vs POST chat) e NUNCA tira o
/// provider da cadeia, so informa a saude (uma chave pode passar num e falhar no outro).
pub async fn validate(
    client: &Client,
    provider: Provider,
    key: &str,
    pctx: &ProviderCtx<'_>,
) -> Probe {
    if provider == Provider::OpenAi
        && crate::connection::ProviderConnection::parse(pctx.openai_base_url).is_err()
    {
        return Probe {
            check: KeyCheck::NetworkError,
            models: Vec::new(),
        };
    }
    let result = match provider {
        Provider::Gemini => {
            client
                .get("https://generativelanguage.googleapis.com/v1beta/models")
                .header("x-goog-api-key", key)
                .timeout(std::time::Duration::from_secs(20))
                .send()
                .await
        }
        Provider::OpenAi => {
            client
                .get(wire::openai_models_url(pctx.openai_base_url))
                .header("Authorization", format!("Bearer {key}"))
                .timeout(std::time::Duration::from_secs(20))
                .send()
                .await
        }
    };
    match result {
        Ok(resp) if resp.status().is_success() => {
            // O corpo e a listagem de modelos. Se nao vier ou vier num formato que nao
            // reconhecemos, a chave continua valida e a lista fica vazia ("nao sei"): a
            // descoberta e um extra, nunca uma razao para declarar uma chave boa como ma.
            let models = match bounded_json(resp).await {
                Some(body) => match provider {
                    Provider::Gemini => ember_core::models::parse_gemini_models(&body),
                    Provider::OpenAi => ember_core::models::parse_openai_models(&body),
                },
                None => Vec::new(),
            };
            Probe {
                check: KeyCheck::Valid,
                models,
            }
        }
        // Qualquer resposta HTTP (401/403/etc.) e o provider a recusar a chave.
        Ok(response) => Probe {
            check: if matches!(response.status().as_u16(), 401 | 403) {
                KeyCheck::Invalid
            } else {
                KeyCheck::NetworkError
            },
            models: Vec::new(),
        },
        // Falha de transporte (sem rede, DNS, timeout): nao diz nada sobre a chave.
        Err(_) => Probe {
            check: KeyCheck::NetworkError,
            models: Vec::new(),
        },
    }
}

async fn bounded_json(response: reqwest::Response) -> Option<Value> {
    let mut stream = response.bytes_stream();
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.ok()?;
        if bytes.len().saturating_add(chunk.len()) > 2 * 1024 * 1024 {
            return None;
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).ok()
}

#[cfg(test)]
mod stream_regressions {
    use super::*;
    use std::io::{Read, Write};

    async fn response(body: &str) -> reqwest::Response {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let body = body.to_owned();
        std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut request = [0u8; 1024];
            let _ = socket.read(&mut request).unwrap();
            write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).unwrap();
        });
        Client::new()
            .get(format!("http://{address}/synthetic"))
            .send()
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn partial_openai_stream_is_never_a_final_result() {
        let body = "data: {\"choices\":[{\"delta\":{\"content\":\"partial\"}}]}\n\n";
        assert_eq!(
            consume_stream(Wire::OpenAiChat, response(body).await, &|_| {}).await,
            Err(OutcomeClass::Uncertain)
        );
    }

    #[tokio::test]
    async fn final_delta_is_kept_when_completion_shares_the_event() {
        let body = "data: {\"choices\":[{\"delta\":{\"content\":\"final\"},\"finish_reason\":\"stop\"}]}\n\n";
        assert_eq!(
            consume_stream(Wire::OpenAiChat, response(body).await, &|_| {}).await,
            Ok("final".into())
        );
    }

    #[tokio::test]
    async fn gemini_requires_explicit_completion() {
        for (finish, expected) in [
            ("", Err(OutcomeClass::Uncertain)),
            (",\"finishReason\":\"STOP\"", Ok("done".into())),
        ] {
            let body = format!("data: {{\"candidates\":[{{\"content\":{{\"parts\":[{{\"text\":\"done\"}}]}}{finish}}}]}}\n\n");
            assert_eq!(
                consume_stream(Wire::Gemini, response(&body).await, &|_| {}).await,
                expected
            );
        }
    }

    #[tokio::test]
    async fn oversized_json_is_rejected_before_deserialization() {
        let body = format!("{{\"value\":\"{}\"}}", "x".repeat(2 * 1024 * 1024));
        assert!(bounded_json(response(&body).await).await.is_none());
    }
}
