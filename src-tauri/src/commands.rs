//! Comandos Tauri das settings + o helper de refinamento usado pelo loop nativo.

use ember_core::model::{ProfileSource, Provider, RefineMode};
use ember_core::prompt::build_llm_request;
use ember_core::retry::RetryConfig;
use serde::Serialize;
use tauri::{AppHandle, State, Manager};

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
    debug_mode: bool,
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
        debug_mode: cfg.debug_mode,
    }
}

fn parse_provider(s: &str) -> Result<Provider, String> {
    match s {
        "gemini" => Ok(Provider::Gemini),
        "claude" => Ok(Provider::Claude),
        _ => Err(format!("invalid provider: {s}")),
    }
}

/// Niveis de thinking aceites pela API Gemini 3.x. Validar aqui evita persistir uma string
/// arbitraria que depois iria no corpo do pedido e seria rejeitada pelo provider.
fn valid_thinking_level(s: &str) -> bool {
    matches!(s, "minimal" | "low" | "medium" | "high")
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
    let previous = cfg.hotkey.clone();
    // Regista PRIMEIRO, persiste depois. Se o novo atalho for invalido ou estiver ocupado,
    // restaura o anterior (o register faz unregister_all, logo sem restauro ficava sem
    // nenhum) e NAO grava o atalho partido em disco (senao persistia partido entre arranques).
    crate::register_hotkey(&app, &hotkey).map_err(|e| {
        let _ = crate::register_hotkey(&app, &previous);
        e
    })?;
    cfg.hotkey = hotkey;
    config::save(&app, &cfg).map_err(|e| e.to_string())
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
    if !valid_thinking_level(&level) {
        return Err(format!("invalid thinking level: {level}"));
    }
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
) -> Result<SettingsDto, String> {
    let mut cfg = config::load(&app);
    cfg.capture_polls = polls.clamp(config::CAPTURE_POLLS.0, config::CAPTURE_POLLS.1);
    cfg.capture_step_ms = step_ms.clamp(config::CAPTURE_STEP_MS.0, config::CAPTURE_STEP_MS.1);
    cfg.paste_settle_ms = settle_ms.clamp(config::PASTE_SETTLE_MS.0, config::PASTE_SETTLE_MS.1);
    config::save(&app, &cfg).map_err(|e| e.to_string())?;
    // Devolve o DTO com os valores ja clampados, para a UI refletir o que ficou gravado
    // em vez de manter os numeros que o utilizador escreveu fora da gama.
    Ok(build_dto(&app, &cfg))
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
pub async fn validate_key(
    state: State<'_, AppState>,
    provider: String,
) -> Result<providers::KeyCheck, String> {
    let p = parse_provider(&provider)?;
    let Some(key) = secrets::get(p) else {
        return Ok(providers::KeyCheck::Invalid);
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

#[tauri::command]
pub fn close_splash(app: AppHandle) {
    if let Some(splash) = app.get_webview_window("splash") {
        let _ = splash.close();
    }
}

/// Chamado pela janela de animacao de quit quando a animacao termina, para a saida acoplar ao
/// fim real da animacao em vez de um sleep de duracao fixa (ver `lib.rs`, tray "quit").
#[tauri::command]
pub fn finalize_quit(app: AppHandle) {
    crate::finalize_quit_now(&app);
}

// ---------------------------------------------------------------------------------------
// Debug / diagnostico
// ---------------------------------------------------------------------------------------

/// Liga/desliga o modo debug. Persiste e aplica ja: abre ou fecha as devtools da janela de
/// settings (se estiver aberta), para o efeito ser imediato sem reabrir.
#[tauri::command]
pub fn set_debug_mode(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut cfg = config::load(&app);
    cfg.debug_mode = enabled;
    config::save(&app, &cfg).map_err(|e| e.to_string())?;
    crate::apply_devtools(&app, enabled);
    log::info!("debug_mode set to {enabled}");
    Ok(())
}

/// Ultimas `lines` linhas do ficheiro de log, para o painel de diagnostico in-app.
#[tauri::command]
pub fn read_recent_logs(app: AppHandle, lines: usize) -> String {
    crate::logging::read_recent(&app, lines.clamp(1, 5000))
}

/// Abre a pasta de logs no explorador de ficheiros do SO.
#[tauri::command]
pub fn reveal_log_dir(app: AppHandle) -> Result<(), String> {
    let dir = app
        .path()
        .app_log_dir()
        .map_err(|e| format!("no log dir: {e}"))?;
    let _ = std::fs::create_dir_all(&dir);
    #[cfg(target_os = "windows")]
    let cmd = ("explorer", dir.as_os_str().to_owned());
    #[cfg(target_os = "macos")]
    let cmd = ("open", dir.as_os_str().to_owned());
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    let cmd = ("xdg-open", dir.as_os_str().to_owned());
    std::process::Command::new(cmd.0)
        .arg(cmd.1)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Bloco de diagnostico copiavel: versao, SO, presenca de chaves, caminho do log, modo debug.
/// Sem segredos (so presenca das chaves), pronto a colar num report de bug.
#[tauri::command]
pub fn get_diagnostics(app: AppHandle) -> String {
    let cfg = config::load(&app);
    let version = app.package_info().version.to_string();
    let log_path = crate::logging::log_file_path(&app)
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "unknown".into());
    format!(
        "Ember {version}\nOS: {} ({})\nGemini key: {}\nClaude key: {}\nMode: {}  Thinking: {} ({})  Debug: {}\nLog: {log_path}",
        std::env::consts::OS,
        std::env::consts::ARCH,
        if secrets::has(Provider::Gemini) { "set" } else { "missing" },
        if secrets::has(Provider::Claude) { "set" } else { "missing" },
        mode_str(cfg.mode),
        cfg.thinking_enabled,
        cfg.thinking_level,
        cfg.debug_mode,
    )
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
        Truncated => "Selection too long for the model. Nothing changed.".into(),
        KeyStore => "Couldn't read your saved keys. Reopen and re-save them.".into(),
        AllProvidersFailed => "Providers failed (network or limits). Try again.".into(),
        _ => "Couldn't refine. Try again.".into(),
    }
}

/// Refina `input` com a chain Gemini->Claude. Devolve (texto CRU do modelo, `Prepared`,
/// provider) ou CoreError: o pos-processamento do motor corre em `flow.rs`, para um output que
/// degrada cair no ramo de restauro do clipboard (nao colar por cima da seleccao). `on_attempt`
/// recebe (provider, indice, tentativa) antes de cada chamada; `on_delta` e no-op (preview off).
pub(crate) async fn refine_text(
    app: &AppHandle,
    state: &AppState,
    input: &str,
    on_attempt: &(dyn Fn(Provider, usize, u32) + Send + Sync),
    on_delta: &(dyn Fn(&str) + Send + Sync),
) -> Result<(String, ember_core::Prepared, String), ember_core::CoreError> {
    let cfg = config::load(app);
    let mut chain: Vec<(Provider, String)> = Vec::new();
    let mut key_store_failed = false;
    for provider in [Provider::Gemini, Provider::Claude] {
        match secrets::try_get(provider) {
            Ok(Some(k)) => chain.push((provider, k)),
            Ok(None) => {}
            // Falha do cofre: nao retirar o provider em silencio. Se ficarmos sem nenhum,
            // reportamos KeyStore (honesto) em vez de "sem providers".
            Err(_) => key_store_failed = true,
        }
    }
    if chain.is_empty() {
        return Err(if key_store_failed {
            ember_core::CoreError::KeyStore
        } else {
            ember_core::CoreError::NoProvidersConfigured
        });
    }

    let resolved = profile::resolve(app, cfg.profile_override.as_deref(), cfg.ignore_claude_md);
    // Motor Ember, fase 1: normaliza o input, mascara codigo/URLs e escapa marcadores. O modelo
    // ve o `masked_input`; o `prepared` volta para o `flow.rs` reconstruir o output.
    let prepared = ember_core::precondition(input, cfg.mode);
    let req = build_llm_request(
        &prepared.masked_input,
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
        on_attempt,
        on_delta,
    )
    .await?;
    Ok((resp.text, prepared, resp.provider.display_name().to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_mode_round_trips_and_rejects_junk() {
        for m in [RefineMode::Adaptive, RefineMode::Polish, RefineMode::Turbo] {
            assert_eq!(parse_mode(mode_str(m)).unwrap(), m);
        }
        assert!(parse_mode("nope").is_err());
    }

    #[test]
    fn parse_provider_accepts_known_rejects_unknown() {
        assert_eq!(parse_provider("gemini").unwrap(), Provider::Gemini);
        assert_eq!(parse_provider("claude").unwrap(), Provider::Claude);
        assert!(parse_provider("openai").is_err());
    }

    #[test]
    fn thinking_level_validation() {
        for lvl in ["minimal", "low", "medium", "high"] {
            assert!(valid_thinking_level(lvl));
        }
        assert!(!valid_thinking_level("extreme"));
        assert!(!valid_thinking_level(""));
    }

    #[test]
    fn friendly_error_is_distinct_and_nonempty() {
        use ember_core::CoreError::*;
        let cases = [
            NoProvidersConfigured,
            Auth,
            ContentPolicy,
            Truncated,
            KeyStore,
            AllProvidersFailed,
        ];
        for e in &cases {
            assert!(!friendly_error(e).is_empty());
        }
        // Mensagens diferentes por classe (o utilizador tem de perceber o que falhou).
        assert_ne!(friendly_error(&Auth), friendly_error(&Truncated));
        assert_ne!(friendly_error(&KeyStore), friendly_error(&NoProvidersConfigured));
    }
}
