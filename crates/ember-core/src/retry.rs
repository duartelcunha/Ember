//! Control flow puro de retry + fallback. O coracao da resiliencia.
//!
//! Tres funcoes puras (`classify`, `backoff_ms`, `plan`) que carregam 100% da
//! ramificacao. O orquestrador (em `src-tauri`) e ~15 linhas e so faz I/O. Assim o
//! standard de resiliencia fica totalmente testavel sem rede (provider-fallback-on-
//! transient-errors / STACK-07).

use crate::error::{CoreError, OutcomeClass};
use serde::{Deserialize, Serialize};

/// Configuracao da maquina de resiliencia.
#[derive(Debug, Clone, PartialEq)]
pub struct RetryConfig {
    /// Quantos providers existem na cadeia (Gemini primario + Claude fallback = 2).
    pub provider_count: usize,
    /// Retries por provider antes de passar ao seguinte (em erros transitorios).
    pub max_retries_per_provider: u32,
    pub base_delay_ms: u64,
    pub max_delay_ms: u64,
    /// Fracao de jitter aplicada ao backoff (ex: 0.25 = ate +25%).
    pub jitter_frac: f64,
    /// Se `true`, uma recusa por politica tenta a outra familia de provider.
    pub fallback_on_content_policy: bool,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            provider_count: 2,
            max_retries_per_provider: 2,
            base_delay_ms: 400,
            max_delay_ms: 8_000,
            jitter_frac: 0.25,
            fallback_on_content_policy: false,
        }
    }
}

/// Estado da maquina: em que provider e em que tentativa estamos.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoopState {
    pub provider_index: usize,
    pub attempt: u32,
}

impl LoopState {
    pub fn start() -> Self {
        Self {
            provider_index: 0,
            attempt: 0,
        }
    }
}

/// O que fazer a seguir, dada a ultima tentativa.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// Sucesso: devolve o resultado.
    Succeed,
    /// Repetir o mesmo provider apos `delay_ms`.
    Retry { delay_ms: u64, next: LoopState },
    /// Passar ao provider seguinte (reset da tentativa).
    Fallback { next: LoopState },
    /// Desistir, propagando a razao sem mascarar.
    Fail { reason: CoreError },
}

/// Classifica uma resposta HTTP numa `OutcomeClass` (com base no status code).
///
/// Content-policy (200 + recusa) e detetado pelo body, nao aqui: o orquestrador usa
/// `providers::*_is_content_policy` e constroi `OutcomeClass::ContentPolicy` diretamente.
/// `api_error_code` fica disponivel para refinamento futuro (mapeamos sobretudo o status).
pub fn classify(
    _provider: crate::model::Provider,
    http_status: u16,
    _api_error_code: Option<&str>,
    retry_after_ms: Option<u64>,
) -> OutcomeClass {
    match http_status {
        200 => OutcomeClass::Success,
        // Transitorios explicitos: timeout/conflito/rate-limit/erros de servidor/overload.
        408 | 409 | 429 | 500 | 502 | 503 | 504 | 529 => {
            OutcomeClass::Transient { retry_after_ms }
        }
        // Credencial: nao faz retry cego; dispara fallback (chave diferente no outro).
        401 | 403 => OutcomeClass::Auth,
        // Bug nosso: propaga sem mascarar.
        400 | 404 | 413 | 422 => OutcomeClass::Payload,
        // Resto: 5xx desconhecido -> transitorio; 4xx desconhecido -> payload.
        s if (500..=599).contains(&s) => OutcomeClass::Transient { retry_after_ms },
        _ => OutcomeClass::Payload,
    }
}

/// Backoff exponencial com jitter. `rng01` e injetado em [0,1) para determinismo nos testes
/// (sem `rand`, sem `Instant` aqui dentro). Honra o `Retry-After`/`RetryInfo` do servidor.
pub fn backoff_ms(
    attempt: u32,
    cfg: &RetryConfig,
    rng01: f64,
    server_retry_after_ms: Option<u64>,
) -> u64 {
    if let Some(server) = server_retry_after_ms {
        return server.min(cfg.max_delay_ms);
    }
    let factor = 1u64.checked_shl(attempt).unwrap_or(u64::MAX);
    let capped = cfg.base_delay_ms.saturating_mul(factor).min(cfg.max_delay_ms);
    let jitter = (capped as f64) * cfg.jitter_frac * rng01.clamp(0.0, 1.0);
    ((capped as f64) + jitter).min(cfg.max_delay_ms as f64) as u64
}

/// A funcao de transicao. Pura: dado o estado e o outcome, devolve a proxima `Decision`.
pub fn plan(state: &LoopState, outcome: &OutcomeClass, cfg: &RetryConfig, rng01: f64) -> Decision {
    let has_next_provider = state.provider_index + 1 < cfg.provider_count;
    let fallback = || Decision::Fallback {
        next: LoopState {
            provider_index: state.provider_index + 1,
            attempt: 0,
        },
    };

    match outcome {
        OutcomeClass::Success => Decision::Succeed,
        OutcomeClass::Payload => Decision::Fail {
            reason: CoreError::Payload,
        },
        OutcomeClass::ContentPolicy => {
            if cfg.fallback_on_content_policy && has_next_provider {
                fallback()
            } else {
                Decision::Fail {
                    reason: CoreError::ContentPolicy,
                }
            }
        }
        OutcomeClass::Auth => {
            if has_next_provider {
                fallback()
            } else {
                Decision::Fail {
                    reason: CoreError::Auth,
                }
            }
        }
        OutcomeClass::Transient { retry_after_ms } => {
            if state.attempt < cfg.max_retries_per_provider {
                Decision::Retry {
                    delay_ms: backoff_ms(state.attempt, cfg, rng01, *retry_after_ms),
                    next: LoopState {
                        provider_index: state.provider_index,
                        attempt: state.attempt + 1,
                    },
                }
            } else if has_next_provider {
                fallback()
            } else {
                Decision::Fail {
                    reason: CoreError::AllProvidersFailed,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Provider;

    fn cfg() -> RetryConfig {
        RetryConfig::default()
    }

    #[test]
    fn classify_maps_status_codes() {
        let g = Provider::Gemini;
        assert_eq!(classify(g, 200, None, None), OutcomeClass::Success);
        assert_eq!(
            classify(g, 429, None, Some(1500)),
            OutcomeClass::Transient {
                retry_after_ms: Some(1500)
            }
        );
        assert_eq!(classify(g, 503, None, None), OutcomeClass::Transient { retry_after_ms: None });
        assert_eq!(classify(g, 401, None, None), OutcomeClass::Auth);
        assert_eq!(classify(g, 403, None, None), OutcomeClass::Auth);
        assert_eq!(classify(g, 400, None, None), OutcomeClass::Payload);
        assert_eq!(classify(g, 404, None, None), OutcomeClass::Payload);
        assert_eq!(classify(g, 418, None, None), OutcomeClass::Payload);
    }

    #[test]
    fn backoff_is_deterministic_and_capped() {
        let c = cfg();
        // attempt 0, sem jitter (rng01=0): == base.
        assert_eq!(backoff_ms(0, &c, 0.0, None), 400);
        // attempt 1: base*2 = 800.
        assert_eq!(backoff_ms(1, &c, 0.0, None), 800);
        // jitter maximo (rng01=1): 800 * (1 + 0.25) = 1000.
        assert_eq!(backoff_ms(1, &c, 1.0, None), 1000);
        // cresce mas nunca passa max_delay.
        assert!(backoff_ms(20, &c, 1.0, None) <= c.max_delay_ms);
        // honra o Retry-After do servidor (capado ao max).
        assert_eq!(backoff_ms(0, &c, 1.0, Some(2000)), 2000);
        assert_eq!(backoff_ms(0, &c, 1.0, Some(999_999)), c.max_delay_ms);
    }

    #[test]
    fn transient_retries_then_falls_back_then_fails() {
        let c = cfg(); // max_retries_per_provider = 2, provider_count = 2
        let out = OutcomeClass::Transient { retry_after_ms: None };

        // attempt 0 e 1 -> retry no mesmo provider.
        let s0 = LoopState::start();
        match plan(&s0, &out, &c, 0.0) {
            Decision::Retry { next, .. } => {
                assert_eq!(next.provider_index, 0);
                assert_eq!(next.attempt, 1);
            }
            d => panic!("esperava Retry, veio {d:?}"),
        }
        let s_exhausted = LoopState { provider_index: 0, attempt: 2 };
        match plan(&s_exhausted, &out, &c, 0.0) {
            Decision::Fallback { next } => {
                assert_eq!(next.provider_index, 1);
                assert_eq!(next.attempt, 0);
            }
            d => panic!("esperava Fallback, veio {d:?}"),
        }
        // ultimo provider esgotado -> Fail.
        let s_last = LoopState { provider_index: 1, attempt: 2 };
        assert_eq!(
            plan(&s_last, &out, &c, 0.0),
            Decision::Fail { reason: CoreError::AllProvidersFailed }
        );
    }

    #[test]
    fn auth_triggers_fallback_then_fails() {
        let c = cfg();
        assert!(matches!(
            plan(&LoopState::start(), &OutcomeClass::Auth, &c, 0.0),
            Decision::Fallback { .. }
        ));
        let last = LoopState { provider_index: 1, attempt: 0 };
        assert_eq!(
            plan(&last, &OutcomeClass::Auth, &c, 0.0),
            Decision::Fail { reason: CoreError::Auth }
        );
    }

    #[test]
    fn payload_never_falls_back() {
        let c = cfg();
        assert_eq!(
            plan(&LoopState::start(), &OutcomeClass::Payload, &c, 0.0),
            Decision::Fail { reason: CoreError::Payload }
        );
    }

    #[test]
    fn content_policy_propagates_by_default_but_can_fall_back() {
        let mut c = cfg();
        assert_eq!(
            plan(&LoopState::start(), &OutcomeClass::ContentPolicy, &c, 0.0),
            Decision::Fail { reason: CoreError::ContentPolicy }
        );
        c.fallback_on_content_policy = true;
        assert!(matches!(
            plan(&LoopState::start(), &OutcomeClass::ContentPolicy, &c, 0.0),
            Decision::Fallback { .. }
        ));
    }

    #[test]
    fn success_succeeds() {
        assert_eq!(
            plan(&LoopState::start(), &OutcomeClass::Success, &cfg(), 0.0),
            Decision::Succeed
        );
    }
}
