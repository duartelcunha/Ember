//! Adapter HTTP dos providers + orquestrador de resiliencia.
//! A ramificacao (classify/plan) vive em `ember_core`; aqui so ha I/O.

use ember_core::error::{CoreError, OutcomeClass};
use ember_core::model::{LlmRequest, LlmResponse, Provider};
use ember_core::providers as wire;
use ember_core::retry::{classify, plan, Decision, LoopState, RetryConfig};
use reqwest::Client;
use serde_json::Value;
use std::time::Duration;

fn retry_after_ms(resp: &reqwest::Response) -> Option<u64> {
    resp.headers()
        .get(reqwest::header::RETRY_AFTER)?
        .to_str()
        .ok()?
        .parse::<u64>()
        .ok()
        .map(|s| s.saturating_mul(1000))
}

/// Uma tentativa contra um provider. `Ok(texto)` = sucesso; `Err(outcome)` = a classificar.
async fn call_once(
    client: &Client,
    provider: Provider,
    key: &str,
    req: &LlmRequest,
) -> Result<String, OutcomeClass> {
    let builder = match provider {
        Provider::Gemini => client
            .post(wire::gemini_url(&req.model, false))
            .header("x-goog-api-key", key)
            .json(&wire::gemini_request_body(req)),
        Provider::Claude => client
            .post(wire::claude_url())
            .header("x-api-key", key)
            .header("anthropic-version", wire::ANTHROPIC_VERSION)
            .json(&wire::claude_request_body(req, false)),
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
        OutcomeClass::Success => {
            let v: Value = resp.json().await.map_err(|_| OutcomeClass::Transient {
                retry_after_ms: None,
            })?;
            let extracted = match provider {
                Provider::Gemini => wire::gemini_extract_text(&v),
                Provider::Claude => wire::claude_extract_text(&v),
            };
            match extracted {
                Ok(t) => Ok(t),
                Err(CoreError::ContentPolicy) => Err(OutcomeClass::ContentPolicy),
                Err(_) => Err(OutcomeClass::Transient {
                    retry_after_ms: None,
                }),
            }
        }
        other => Err(other),
    }
}

/// Refina com resiliencia: retry transitorio + fallback no esgotamento. A decisao e pura.
pub async fn refine(
    client: &Client,
    cfg: &RetryConfig,
    chain: &[(Provider, String)],
    base_req: &LlmRequest,
    gemini_model: &str,
    claude_model: &str,
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

        match call_once(client, *provider, key, &req).await {
            Ok(text) => {
                return Ok(LlmResponse {
                    text,
                    provider: *provider,
                })
            }
            Err(outcome) => match plan(&state, &outcome, cfg, 0.5) {
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

/// Probe barato de validacao de chave (pre-validacao). `true` = chave aceite.
pub async fn validate(client: &Client, provider: Provider, key: &str) -> bool {
    match provider {
        Provider::Gemini => client
            .get("https://generativelanguage.googleapis.com/v1beta/models")
            .header("x-goog-api-key", key)
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false),
        Provider::Claude => client
            .get("https://api.anthropic.com/v1/models")
            .header("x-api-key", key)
            .header("anthropic-version", wire::ANTHROPIC_VERSION)
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false),
    }
}
