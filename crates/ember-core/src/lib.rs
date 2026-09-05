//! `ember-core`: logica pura e sem I/O do Ember.
//!
//! Nada aqui toca em tauri, rede ou SO. Toda a ramificacao de decisao (classificacao
//! de erros, backoff, fallback, normalizacao de modificadores, resolucao do perfil,
//! construcao do prompt, mapping de wire-format) vive aqui e testa-se de forma
//! deterministica com `cargo test -p ember-core`.

pub mod codex;
pub mod cycle;
pub mod engine;
pub mod error;
pub mod health;
pub mod hotkey;
pub mod input;
pub mod model;
pub mod models;
pub mod modifiers;
pub mod oklch;
pub mod overlay;
pub mod overlay_geom;
pub mod preview;
pub mod profile_import;
pub mod profile_path;
pub mod project;
pub mod projects;
pub mod prompt;
pub mod providers;
pub mod refine_cache;
pub mod retry;
pub mod selection;

pub use cycle::{hotkey_action, may_hide, may_release_guard, owns_overlay, HotkeyAction};
pub use engine::{
    is_worth_refining, postprocess, precondition, DegradeReason, EngineResult, Prepared,
};
pub use error::{CoreError, OutcomeClass};
pub use health::{assess_providers, KeyCheck, ProviderStatus, Readiness, SystemHealth};
pub use hotkey::{evaluate as evaluate_hotkey, HotkeyVerdict, Os};
pub use model::{LlmRequest, LlmResponse, Profile, ProfileSource, Provider, RefineMode};
pub use models::{pick_default, rank, reconcile, ModelInfo};
pub use modifiers::{decide_neutralize, Modifier, ModifierState, NeutralizeDecision};
pub use overlay_geom::{overlay_geometry, Layout, Rect, DEFAULT_LAYOUT};
pub use prompt::{build_llm_request, build_system_prompt};
pub use refine_cache::{CacheEntry, CacheKey, Hit, RefineCache};
pub use retry::{backoff_ms, classify, plan, Decision, LoopState, RetryConfig};
