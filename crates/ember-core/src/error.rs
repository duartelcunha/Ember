//! Taxonomia de erros e classes de outcome.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Classe de outcome de uma tentativa contra um provider. E o que alimenta `plan`.
///
/// - `Success`: 200 com conteudo util.
/// - `Transient`: 429/5xx/timeout/overload. Retry com backoff; fallback no esgotamento.
/// - `Auth`: 401/403. Chave invalida -> fallback (o outro provider tem chave diferente).
/// - `ModelNotFound`: 404. O MODELO nao existe (foi descontinuado, ou o id esta mal escrito).
///   Faz fallback: a outra familia tem um modelo completamente diferente e nao sabe nada deste.
///   Sem retry (repetir devolve o mesmo 404).
/// - `Payload`: 400/413/422. Bug nosso no pedido -> propaga sem mascarar (nao faz fallback).
/// - `ContentPolicy`: recusa por politica (Claude stop_reason=refusal; Gemini SAFETY).
///   Nao-transitorio: propaga por defeito (config pode tentar a outra familia).
/// - `Truncated`: resposta cortada por max_tokens. Deterministico no mesmo provider
///   (retry cego devolve o mesmo corte), mas o outro provider pode ter folga: fallback.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OutcomeClass {
    Success,
    Transient { retry_after_ms: Option<u64> },
    /// O MODELO esta sem capacidade agora (o Gemini responde 503 "high demand"). Parece um
    /// transitorio mas nao e: insistir no mesmo modelo da o mesmo 503, porque a fila e daquele
    /// modelo e nao da conta nem da rede. Visto em producao a queimar tres tentativas e tres
    /// segundos antes de fazer o que devia ter feito logo, que e ir para o modelo do lado.
    Overloaded,
    Auth,
    ModelNotFound,
    Payload,
    ContentPolicy,
    Truncated,
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
    #[error("o modelo escolhido nao existe neste provider")]
    ModelNotFound,
    #[error("recusado por politica de conteudo")]
    ContentPolicy,
    #[error("resposta truncada pelo limite de tokens")]
    Truncated,
    #[error("falha a ler as chaves do cofre de credenciais")]
    KeyStore,
    #[error("resposta do provider sem texto utilizavel")]
    EmptyResponse,
    #[error("falha a interpretar a resposta do provider: {0}")]
    Parse(String),
}
