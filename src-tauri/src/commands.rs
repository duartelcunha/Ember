//! Comandos Tauri das settings + o helper de refinamento usado pelo loop nativo.

use ember_core::model::{ProfileSource, Provider, RefineMode};
use ember_core::prompt::build_llm_request;
use ember_core::retry::RetryConfig;
use serde::Serialize;
use tauri::{AppHandle, State};

use crate::state::AppState;
use crate::{config, profile, providers, secrets};

// ---------------------------------------------------------------------------------------
// DTO + helpers
// ---------------------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsDto {
    gemini_model: String,
    claude_model: String,
    hotkey: String,
    autostart: bool,
    has_gemini_key: bool,
    has_claude_key: bool,
    profile_text: String,
    profile_source: &'static str,
    profile_path: Option<String>,
    mode: &'static str,
    thinking_enabled: bool,
    thinking_level: String,
    terminal_handling: bool,
    capture_polls: u32,
    capture_step_ms: u64,
    paste_settle_ms: u64,
}

fn source_str(s: ProfileSource) -> &'static str {
    match s {
        ProfileSource::ClaudeMd => "claude_md",
        ProfileSource::UserEdited => "user_edited",
        ProfileSource::Default => "default",
    }
}

fn mode_str(m: RefineMode) -> &'static str {
    match m {
        RefineMode::Adaptive => "adaptive",
        RefineMode::Polish => "polish",
        RefineMode::Turbo => "turbo",
    }
}

fn parse_mode(s: &str) -> Result<RefineMode, String> {
    match s {
        "adaptive" => Ok(RefineMode::Adaptive),
        "polish" => Ok(RefineMode::Polish),
        "turbo" => Ok(RefineMode::Turbo),
        _ => Err(format!("invalid mode: {s}")),
    }
}

fn build_dto(app: &AppHandle, cfg: &config::Config) -> SettingsDto {
    let resolved = profile::resolve(app, cfg.profile_override.as_deref(), cfg.ignore_claude_md);
    SettingsDto {
        gemini_model: cfg.gemini_model.clone(),
        claude_model: cfg.claude_model.clone(),
        hotkey: cfg.hotkey.clone(),
        autostart: cfg.autostart,
        has_gemini_key: secrets::has(Provider::Gemini),
        has_claude_key: secrets::has(Provider::Claude),
        profile_text: resolved.profile.text,
        profile_source: source_str(resolved.profile.source),
        profile_path: resolved.path,
        mode: mode_str(cfg.mode),
        thinking_enabled: cfg.thinking_enabled,
        thinking_level: cfg.thinking_level.clone(),
        terminal_handling: cfg.terminal_handling,
        capture_polls: cfg.capture_polls,
        capture_step_ms: cfg.capture_step_ms,
        paste_settle_ms: cfg.paste_settle_ms,
    }
}

fn parse_provider(s: &str) -> Result<Provider, String> {
    match s {
        "gemini" => Ok(Provider::Gemini),
        "claude" => Ok(Provider::Claude),
        _ => Err(format!("invalid provider: {s}")),
    }
}

// ---------------------------------------------------------------------------------------
// Comandos de settings
// ---------------------------------------------------------------------------------------

#[tauri::command]
pub fn get_settings(app: AppHandle) -> SettingsDto {
    let cfg = config::load(&app);
    build_dto(&app, &cfg)
}

#[tauri::command]
pub fn set_model(app: AppHandle, provider: String, model: String) -> Result<(), String> {
    let mut cfg = config::load(&app);
    match provider.as_str() {
        "gemini" => cfg.gemini_model = model,
        "claude" => cfg.claude_model = model,
        _ => return Err(format!("invalid provider: {provider}")),
    }
    config::save(&app, &cfg).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_hotkey(app: AppHandle, hotkey: String) -> Result<(), String> {
    let mut cfg = config::load(&app);
    cfg.hotkey = hotkey.clone();
    config::save(&app, &cfg).map_err(|e| e.to_string())?;
    crate::register_hotkey(&app, &hotkey)
}

#[tauri::command]
pub fn set_autostart(app: AppHandle, enabled: bool) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;
    let mut cfg = config::load(&app);
    cfg.autostart = enabled;
    config::save(&app, &cfg).map_err(|e| e.to_string())?;
    let m = app.autolaunch();
    let r = if enabled { m.enable() } else { m.disable() };
    r.map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_mode(app: AppHandle, mode: String) -> Result<(), String> {
    let mut cfg = config::load(&app);
    cfg.mode = parse_mode(&mode)?;
    config::save(&app, &cfg).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_thinking(app: AppHandle, enabled: bool, level: String) -> Result<(), String> {
    let mut cfg = config::load(&app);
    cfg.thinking_enabled = enabled;
    cfg.thinking_level = level;
    config::save(&app, &cfg).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_terminal_handling(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut cfg = config::load(&app);
    cfg.terminal_handling = enabled;
    config::save(&app, &cfg).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_capture_timing(
    app: AppHandle,
    polls: u32,
    step_ms: u64,
    settle_ms: u64,
) -> Result<(), String> {
    let mut cfg = config::load(&app);
    cfg.capture_polls = polls.clamp(5, 200);
    cfg.capture_step_ms = step_ms.clamp(1, 100);
    cfg.paste_settle_ms = settle_ms.clamp(0, 1000);
    config::save(&app, &cfg).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_api_key(provider: String, key: String) -> Result<(), String> {
    let p = parse_provider(&provider)?;
    secrets::set(p, &key).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn clear_api_key(provider: String) -> Result<(), String> {
    let p = parse_provider(&provider)?;
    secrets::delete(p).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn validate_key(state: State<'_, AppState>, provider: String) -> Result<bool, String> {
    let p = parse_provider(&provider)?;
    let Some(key) = secrets::get(p) else {
        return Ok(false);
    };
    Ok(providers::validate(&state.http, p, &key).await)
}

#[tauri::command]
pub fn set_profile(app: AppHandle, text: String) -> Result<(), String> {
    let mut cfg = config::load(&app);
    cfg.profile_override = if text.trim().is_empty() {
        None
    } else {
        Some(text)
    };
    cfg.ignore_claude_md = false;
    config::save(&app, &cfg).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn reload_profile(app: AppHandle) -> Result<SettingsDto, String> {
    let mut cfg = config::load(&app);
    cfg.profile_override = None;
    cfg.ignore_claude_md = false;
    config::save(&app, &cfg).map_err(|e| e.to_string())?;
    Ok(build_dto(&app, &cfg))
}

#[tauri::command]
pub fn reset_profile(app: AppHandle) -> Result<SettingsDto, String> {
    let mut cfg = config::load(&app);
    cfg.profile_override = None;
    cfg.ignore_claude_md = true;
    config::save(&app, &cfg).map_err(|e| e.to_string())?;
    Ok(build_dto(&app, &cfg))
}

// ---------------------------------------------------------------------------------------
// Refine helper (chamado pelo loop nativo em flow.rs)
// ---------------------------------------------------------------------------------------

pub(crate) fn friendly_error(e: &ember_core::CoreError) -> String {
    use ember_core::CoreError::*;
    match e {
        NoProvidersConfigured => "No API key set. Opening settings…".into(),
        Auth => "Invalid API key. Check settings.".into(),
        ContentPolicy => "Blocked by the provider's content policy.".into(),
        AllProvidersFailed => "Providers failed (network or limits). Try again.".into(),
        _ => "Couldn't refine. Try again.".into(),
    }
}

/// Refina `input` com a chain Gemini->Claude. Devolve (texto, provider) ou CoreError.
pub(crate) async fn refine_text(
    app: &AppHandle,
    state: &AppState,
    input: &str,
) -> Result<(String, String), ember_core::CoreError> {
    let cfg = config::load(app);
    let mut chain: Vec<(Provider, String)> = Vec::new();
    if let Some(k) = secrets::get(Provider::Gemini) {
        chain.push((Provider::Gemini, k));
    }
    if let Some(k) = secrets::get(Provider::Claude) {
        chain.push((Provider::Claude, k));
    }
    if chain.is_empty() {
        return Err(ember_core::CoreError::NoProvidersConfigured);
    }

    let resolved = profile::resolve(app, cfg.profile_override.as_deref(), cfg.ignore_claude_md);
    let req = build_llm_request(
        input,
        &resolved.profile,
        &cfg.gemini_model,
        cfg.mode,
        cfg.thinking_enabled,
        &cfg.thinking_level,
    );
    let rcfg = RetryConfig {
        provider_count: chain.len(),
        ..RetryConfig::default()
    };
    let resp = providers::refine(
        &state.http,
        &rcfg,
        &chain,
        &req,
        &cfg.gemini_model,
        &cfg.claude_model,
    )
    .await?;
    Ok((resp.text, resp.provider.display_name().to_string()))
}
