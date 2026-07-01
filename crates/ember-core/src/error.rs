//! Taxonomia de erros e classes de outcome.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Classe de outcome de uma tentativa contra um provider. E o que alimenta `plan`.
///
/// - `Success`: 200 com conteudo util.
/// - `Transient`: 429/5xx/timeout/overload. Retry com backoff; fallback no esgotamento.
/// - `Auth`: 401/403. Chave invalida -> fallback (o outro provider tem chave diferente).
/// - `Payload`: 400/404/413. Bug nosso -> propaga sem mascarar (nao faz fallback).
/// - `ContentPolicy`: recusa por politica (Claude stop_reason=refusal; Gemini SAFETY).
///   Nao-transitorio: propaga por defeito (config pode tentar a outra familia).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OutcomeClass {
    Success,
    Transient { retry_after_ms: Option<u64> },
    Auth,
    Payload,
    ContentPolicy,
}

/// Erros de dominio. Sao o `reason` em `Decision::Fail` e o retorno dos parsers de wire.
#[derive(Debug, Clone, PartialEq, Eq, Error, Serialize, Deserialize)]
pub enum CoreError {
    #[error("todos os providers falharam (erros transitorios esgotados)")]
    AllProvidersFailed,
    #[error("sem providers configurados")]
    NoProvidersConfigured,
    #[error("chave de API invalida ou sem permissao")]
    Auth,
    #[error("pedido invalido (payload)")]
    Payload,
    #[error("recusado por politica de conteudo")]
    ContentPolicy,
    #[error("resposta do provider sem texto utilizavel")]
    EmptyResponse,
    #[error("falha a interpretar a resposta do provider: {0}")]
    Parse(String),
}
