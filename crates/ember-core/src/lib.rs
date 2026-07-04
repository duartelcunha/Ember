//! `ember-core`: logica pura e sem I/O do Ember.
//!
//! Nada aqui toca em tauri, rede ou SO. Toda a ramificacao de decisao (classificacao
//! de erros, backoff, fallback, normalizacao de modificadores, resolucao do perfil,
//! construcao do prompt, mapping de wire-format) vive aqui e testa-se de forma
//! deterministica com `cargo test -p ember-core`.

pub mod engine;
pub mod error;
pub mod model;
pub mod modifiers;
pub mod overlay;
pub mod profile_path;
pub mod prompt;
pub mod providers;
pub mod retry;
pub mod selection;

pub use engine::{postprocess, precondition, DegradeReason, EngineResult, Prepared};
pub use error::{CoreError, OutcomeClass};
pub use model::{LlmRequest, LlmResponse, Profile, ProfileSource, Provider, RefineMode};
pub use modifiers::{decide_neutralize, Modifier, ModifierState, NeutralizeDecision};
pub use prompt::{build_llm_request, build_system_prompt};
pub use retry::{backoff_ms, classify, plan, Decision, LoopState, RetryConfig};
