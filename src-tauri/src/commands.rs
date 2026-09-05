//! Comandos Tauri das settings + o helper de refinamento usado pelo loop nativo.

use ember_core::model::{ProfileSource, Provider, RefineMode};
use ember_core::prompt::build_llm_request;
use ember_core::retry::RetryConfig;
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::state::AppState;
use crate::{config, profile, providers, secrets};

// ---------------------------------------------------------------------------------------
// DTO + helpers
// ---------------------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsDto {
    gemini_model: String,
    openai_model: String,
    openai_base_url: String,
    gemini_model_auto: bool,
    hotkey: String,
    hotkey_polish: String,
    hotkey_turbo: String,
    hotkey_picker: String,
    autostart: bool,
    has_gemini_key: bool,
    has_openai_key: bool,
    /// Como o slot de fallback se autentica: `"api_key"` ou `"chat_gpt"`.
    openai_auth: &'static str,
    /// Qual provider e tentado primeiro: `"gemini"` ou `"openai"`.
    primary_provider: Provider,
    /// Ha uma sessao ChatGPT gravada. Independente do `openai_auth`: quem faz login e depois volta
    /// a um servico por chave nao perde a sessao, e a UI deve continuar a oferecer o sign out.
    chatgpt_signed_in: bool,
    /// A conta ligada, quando o token a diz. `None` nao quer dizer que nao ha sessao.
    chatgpt_account: Option<String>,
    /// `Some(msg)` quando nao foi possivel ler o cofre de credenciais (bloqueado/partido). A UI
    /// mostra um banner persistente. Honra a regra de nao degradar em silencio: em vez de mentir
    /// "sem chave", diz que nao conseguiu verificar.
    key_store_error: Option<String>,
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
    save_prompts: bool,
    keep_results: bool,
    project_context: bool,
    preview_before_paste: bool,
    theme: String,
    projects: Vec<ember_core::projects::Project>,
    active_project: Option<String>,
    /// A paleta e a lista de icones vao daqui para a UI em vez de serem reescritas em TS. Sao a
    /// mesma verdade nos dois lados, e duplica-las era garantir que um dia divergiam.
    accents: Vec<AccentDto>,
    icons: Vec<&'static str>,
    /// The band the colour wheel picks inside, so the frontend paints exactly the colours the
    /// derivation can produce without keeping its own copy of these numbers.
    accent_wheel: WheelDto,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WheelDto {
    lightness: f64,
    max_chroma: f64,
    /// Hue ring, first stop repeated at the end so the conic gradient closes without a seam.
    ring: Vec<String>,
    centre: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AccentDto {
    pub raw: &'static str,
    pub mid: &'static str,
    pub glow: &'static str,
    pub label: &'static str,
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
    // Le as 3 chaves honestamente: uma falha do cofre (Err) nao se colapsa em "sem chave".
    // Se o cofre estiver bloqueado, todas ficam false e key_store_error informa a UI.
    let (has_g, has_o, key_store_error) = match (
        secrets::try_has(Provider::Gemini, &cfg.openai_base_url),
        secrets::try_has(Provider::OpenAi, &cfg.openai_base_url),
    ) {
        (Ok(g), Ok(o)) => (g, o, None),
        (e_g, e_o) => {
            // Pelo menos um falhou a ler o cofre. Loga para diagnostico; a UI mostra banner.
            let any_err = e_g.err().or_else(|| e_o.err());
            log::warn!("settings: credential vault read failed: {:?}", any_err);
            (
                false,
                false,
                Some("credential vault unreadable".to_string()),
            )
        }
    };
    let session = secrets::get_oauth().unwrap_or(None);
    SettingsDto {
        gemini_model: cfg.gemini_model.clone(),
        openai_model: cfg.openai_model.clone(),
        openai_base_url: cfg.openai_base_url.clone(),
        gemini_model_auto: cfg.gemini_model_auto,
        hotkey: cfg.hotkey.clone(),
        hotkey_polish: cfg.hotkey_polish.clone(),
        hotkey_turbo: cfg.hotkey_turbo.clone(),
        hotkey_picker: cfg.hotkey_picker.clone(),
        autostart: cfg.autostart,
        has_gemini_key: has_g,
        has_openai_key: has_o,
        openai_auth: match cfg.openai_auth {
            config::OpenAiAuth::ApiKey => "api_key",
            config::OpenAiAuth::ChatGpt => "chat_gpt",
        },
        primary_provider: cfg.primary_provider,
        // Uma falha do cofre aqui nao pode rebentar as settings inteiras: fica "sem sessao", e o
        // `key_store_error` acima ja e o canal honesto para o cofre ilegivel.
        chatgpt_signed_in: session.is_some(),
        // O nome, nunca o id: quem abre as settings quer saber QUE conta esta ligada, e um
        // identificador opaco nao responde a isso. Sem nome, a UI diz so que ha sessao.
        chatgpt_account: session.and_then(|s| s.account_label),
        key_store_error,
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
        save_prompts: cfg.save_prompts,
        keep_results: cfg.keep_results,
        project_context: cfg.project_context,
        preview_before_paste: cfg.preview_before_paste,
        theme: cfg.theme.clone(),
        projects: cfg.projects.clone(),
        active_project: cfg.active_project.clone(),
        accents: ember_core::projects::ACCENTS
            .iter()
            .map(|a| AccentDto {
                raw: a.raw,
                mid: a.mid,
                glow: a.glow,
                label: a.label,
            })
            .collect(),
        icons: ember_core::projects::ICONS.to_vec(),
        accent_wheel: WheelDto {
            lightness: ember_core::projects::WHEEL_LIGHTNESS,
            max_chroma: ember_core::projects::WHEEL_MAX_CHROMA,
            // 36 stops: ten degrees apart is below what the eye resolves in a 176px disc, and the
            // gradient interpolates the rest.
            ring: ember_core::projects::wheel_ring(36),
            centre: ember_core::projects::wheel_centre(),
        },
    }
}

/// Presenca de chave para o diagnostico, honesta: distingue configurada / ausente / cofre
/// ilegivel. O diagnostico e best-effort (nao devemos rebentar se o cofre estiver bloqueado).
fn key_state(p: Provider, base_url: &str) -> &'static str {
    match secrets::try_has(p, base_url) {
        Ok(true) => "set",
        Ok(false) => "missing",
        Err(_) => {
            log::warn!("diagnostics: couldn't read {p:?} key from the vault");
            "unreadable"
        }
    }
}

fn parse_provider(s: &str) -> Result<Provider, String> {
    match s {
        "gemini" => Ok(Provider::Gemini),
        "openai" => Ok(Provider::OpenAi),
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

/// Fixa um modelo a mao. Escolher o do Gemini desliga o automatico: a partir daqui a escolha
/// e dele e a descoberta deixa de lhe mexer (ver `config::Config::gemini_model_auto`).
#[tauri::command]
pub fn set_model(app: AppHandle, provider: String, model: String) -> Result<(), String> {
    let mut cfg = config::load(&app);
    match provider.as_str() {
        "gemini" => {
            cfg.gemini_model = model;
            cfg.gemini_model_auto = false;
        }
        "openai" => cfg.openai_model = model,
        _ => return Err(format!("invalid provider: {provider}")),
    }
    config::save(&app, &cfg).map_err(|e| e.to_string())
}

/// Devolve a escolha do modelo do Gemini ao automatico. O modelo passa a acompanhar o melhor
/// gratuito que o provider anunciar, e muda sozinho quando a Google lanca uma geracao nova.
#[tauri::command]
pub fn set_gemini_model_auto(
    app: AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<SettingsDto, String> {
    let mut cfg = config::load(&app);
    cfg.gemini_model_auto = enabled;
    // Aplica JA a partir da listagem em cache, para o dropdown mudar no mesmo instante em vez de
    // so no proximo probe. Sem listagem, fica o que estava e o proximo probe resolve.
    if enabled {
        let catalog = crate::models_cache::catalog(
            &state,
            Provider::Gemini,
            &cfg.openai_base_url,
            cfg.openai_auth,
        );
        if catalog.live {
            if let Some(best) = ember_core::models::pick_default(Provider::Gemini, &catalog.models)
            {
                cfg.gemini_model = best;
            }
        }
    }
    config::save(&app, &cfg).map_err(|e| e.to_string())?;
    Ok(get_settings(app))
}

#[tauri::command]
pub fn set_openai_base_url(
    app: AppHandle,
    state: State<'_, AppState>,
    base_url: String,
) -> Result<(), String> {
    let mut generation = state
        .connection_generation
        .lock()
        .map_err(|_| "Connection unavailable")?;
    *generation += 1;

    let mut cfg = config::load(&app);
    let connection = crate::connection::ProviderConnection::parse(&base_url)?;
    secrets::migrate_legacy_openai(&cfg.openai_base_url)
        .map_err(|_| "Credential migration failed")?;
    cfg.openai_base_url = connection.base_url;
    // Re-sanitiza so este campo (vazio -> default, tira barra final) antes de gravar.
    let d = config::Config::default();
    let trimmed = cfg.openai_base_url.trim().trim_end_matches('/');
    cfg.openai_base_url = if trimmed.is_empty() {
        d.openai_base_url
    } else {
        trimmed.to_string()
    };
    // Endpoint novo, listagem velha: os modelos do servico anterior nao existem neste. Esquece,
    // e a UI volta a lista embutida (marcada como nao-viva) ate o proximo probe trazer a certa.
    crate::models_cache::forget(&state, Provider::OpenAi);
    config::save(&app, &cfg).map_err(|e| e.to_string())
}

/// Os slots de atalho JA OCUPADOS, excluindo `editing`. Sem excluir o slot que esta a ser
/// editado, regravar a mesma combinacao onde ela ja estava acusava conflito consigo propria.
fn other_slots(cfg: &config::Config, editing: &str) -> Vec<(String, String)> {
    [
        ("main", &cfg.hotkey),
        ("polish", &cfg.hotkey_polish),
        ("turbo", &cfg.hotkey_turbo),
        ("picker", &cfg.hotkey_picker),
    ]
    .into_iter()
    .filter(|(slot, _)| *slot != editing)
    .map(|(slot, accel)| (slot.to_string(), accel.clone()))
    .collect()
}

/// Avalia uma combinacao ANTES de a gravar, para a UI a poder recusar na hora em vez de a
/// aceitar e o utilizador so descobrir mais tarde que o atalho nunca dispara.
///
/// Duas metades, porque nenhuma chega sozinha. A politica pura (`ember_core::hotkey`) apanha o
/// que o SO nao recusa: os atalhos do sistema no macOS, e a combinacao ja dada a outro modo do
/// Ember. O teste de registo real apanha o resto: qualquer outra aplicacao que ja tenha a
/// combinacao, coisa que nenhuma lista escrita a mao pode saber.
#[tauri::command]
pub fn check_hotkey(
    app: AppHandle,
    which: String,
    hotkey: String,
) -> Result<ember_core::hotkey::HotkeyVerdict, String> {
    use ember_core::hotkey::{self, HotkeyVerdict};
    if !matches!(which.as_str(), "main" | "polish" | "turbo" | "picker") {
        return Err(format!("invalid hotkey slot: {which}"));
    }
    let cfg = config::load(&app);
    let os = crate::current_os();

    // O picker tem uma regra so dele: a tecla principal nao pode ser uma das que ele proprio
    // consome. Vem ANTES do atalho de saida rapida abaixo, porque um `Shift+Up` ja gravado tem
    // de continuar a ser recusado quando ele o volta a submeter.
    if which == "picker" {
        if let Some(key) = hotkey::picker_key_clash(&hotkey) {
            return Ok(HotkeyVerdict::ClashesWithPicker { key });
        }
    }

    // Regravar a MESMA combinacao no mesmo slot passa sem tocar em nada: ela ja esta registada
    // por nos, e o teste de registo iria falhar com "already registered" e mentir ao utilizador.
    let current = match which.as_str() {
        "main" => &cfg.hotkey,
        "polish" => &cfg.hotkey_polish,
        "picker" => &cfg.hotkey_picker,
        _ => &cfg.hotkey_turbo,
    };
    if hotkey::same_hotkey(&hotkey, current, os) {
        return Ok(HotkeyVerdict::Available);
    }

    let others = other_slots(&cfg, &which);
    let refs: Vec<(&str, &str)> = others
        .iter()
        .map(|(s, a)| (s.as_str(), a.as_str()))
        .collect();
    match hotkey::evaluate(&hotkey, os, &refs) {
        HotkeyVerdict::Available => {
            if crate::probe_hotkey_free(&app, &hotkey) {
                Ok(HotkeyVerdict::Available)
            } else {
                Ok(HotkeyVerdict::ReservedByOs {
                    owner: "another application".into(),
                })
            }
        }
        verdict => Ok(verdict),
    }
}

/// Grava um dos atalhos. `which` = "main" | "polish" | "turbo" | "picker".
///
/// Regista PRIMEIRO, persiste depois. Se o novo atalho for invalido ou estiver ocupado, restaura
/// o conjunto anterior (o registo faz `unregister_all`, logo sem restauro a app ficava sem atalho
/// nenhum) e NAO grava o atalho partido em disco, senao persistia partido entre arranques. O erro
/// que sobe traz a combinacao e a mensagem do SO, para a UI poder dizer QUAL falhou e porque, em
/// vez de um "nao deu" generico.
#[tauri::command]
pub fn set_hotkey(app: AppHandle, which: String, hotkey: String) -> Result<(), String> {
    static TRANSACTION: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _transaction = TRANSACTION
        .lock()
        .map_err(|_| "Hotkey settings unavailable")?;

    let mut cfg = config::load(&app);
    let previous = cfg.clone();
    // A POLITICA corre aqui, e nao so no `check_hotkey`, porque este comando e a porta que grava
    // de verdade e uma validacao que so vive na UI nao e validacao. O `check` pode falhar (fora
    // do Tauri, ou um erro de IPC) e a UI grava na mesma nesse caso; sem esta rede, um `Enter`
    // sozinho podia ficar gravado e roubar a tecla ao sistema inteiro. Limpar (string vazia) e
    // sempre legal: quer dizer "nao registes nada neste slot".
    if !hotkey.trim().is_empty() {
        if which == "picker" {
            if let Some(key) = ember_core::hotkey::picker_key_clash(&hotkey) {
                return Err(format!(
                    "the project picker needs {key} to navigate; pick a combination without it"
                ));
            }
        }
        let outros = other_slots(&cfg, &which);
        let refs: Vec<(&str, &str)> = outros
            .iter()
            .map(|(s, a)| (s.as_str(), a.as_str()))
            .collect();
        match ember_core::hotkey::evaluate(&hotkey, crate::current_os(), &refs) {
            ember_core::hotkey::HotkeyVerdict::Available => {}
            outro => return Err(format!("{hotkey} can't be used: {outro:?}")),
        }
    }
    match which.as_str() {
        "main" => cfg.hotkey = hotkey,
        "polish" => cfg.hotkey_polish = hotkey,
        "turbo" => cfg.hotkey_turbo = hotkey,
        "picker" => cfg.hotkey_picker = hotkey,
        _ => return Err(format!("invalid hotkey slot: {which}")),
    }
    crate::register_hotkeys(&app, &cfg).inspect_err(|_| {
        let _ = crate::register_hotkeys(&app, &previous);
    })?;
    if let Err(error) = config::save(&app, &cfg) {
        let _ = crate::register_hotkeys(&app, &config::load(&app));
        return Err(error.to_string());
    }
    Ok(())
}

#[tauri::command]
pub fn set_autostart(app: AppHandle, enabled: bool) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;
    static TRANSACTION: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _transaction = TRANSACTION
        .lock()
        .map_err(|_| "Autostart settings unavailable")?;
    let mut cfg = config::load(&app);
    let manager = app.autolaunch();
    let previous = manager.is_enabled().map_err(|e| e.to_string())?;
    if enabled {
        manager.enable()
    } else {
        manager.disable()
    }
    .map_err(|e| e.to_string())?;
    cfg.autostart = enabled;
    if let Err(error) = config::save(&app, &cfg) {
        let rollback = if previous {
            manager.enable()
        } else {
            manager.disable()
        };
        if rollback.is_err() {
            return Err("Autostart changed but settings could not be saved or restored. Reopen settings to reconcile.".into());
        }
        return Err(error.to_string());
    }
    Ok(())
}

#[tauri::command]
pub fn set_mode(app: AppHandle, mode: String) -> Result<(), String> {
    let mut cfg = config::load(&app);
    cfg.mode = parse_mode(&mode)?;
    config::save(&app, &cfg).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_theme(app: AppHandle, theme: String) -> Result<(), String> {
    if theme != "dark" && theme != "cream" {
        return Err(format!("invalid theme: {theme}"));
    }
    let mut cfg = config::load(&app);
    cfg.theme = theme;
    config::save(&app, &cfg).map_err(|e| e.to_string())?;
    // Pinta o canvas nativo da janela ja com a cor do tema: a proxima abertura nao pisca a cor
    // antiga antes de o CSS aplicar (o CSS so corre depois do webview carregar).
    crate::apply_window_theme(&app);
    Ok(())
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
pub fn set_project_context(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut cfg = config::load(&app);
    cfg.project_context = enabled;
    if enabled {
        cfg.active_project = None;
    }
    config::save(&app, &cfg).map_err(|e| e.to_string())
}

/// Liga/desliga o fallback de select-all (refinar o campo em foco quando nada esta selecionado).
#[tauri::command]
pub fn set_select_all_fallback(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut cfg = config::load(&app);
    cfg.select_all_fallback = enabled;
    config::save(&app, &cfg).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_preview_before_paste(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut cfg = config::load(&app);
    cfg.preview_before_paste = enabled;
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
pub fn set_api_key(
    app: AppHandle,
    state: State<'_, AppState>,
    provider: String,
    key: String,
) -> Result<(), String> {
    let mut generation = state
        .connection_generation
        .lock()
        .map_err(|_| "Connection unavailable")?;
    *generation += 1;

    let p = parse_provider(&provider)?;
    secrets::set(p, &key, &config::load(&app).openai_base_url).map_err(|e| e.to_string())?;
    crate::models_cache::forget(&state, p);
    // A chave mudou: o probe antigo deixa de valer. Tira do cache (fica "por revalidar").
    if let Ok(mut m) = state.key_checks.lock() {
        m.remove(&p);
    }
    Ok(())
}

#[tauri::command]
pub fn clear_api_key(
    app: AppHandle,
    state: State<'_, AppState>,
    provider: String,
) -> Result<(), String> {
    let mut generation = state
        .connection_generation
        .lock()
        .map_err(|_| "Connection unavailable")?;
    *generation += 1;

    let p = parse_provider(&provider)?;
    secrets::delete(p, &config::load(&app).openai_base_url).map_err(|e| e.to_string())?;
    crate::models_cache::forget(&state, p);
    if let Ok(mut m) = state.key_checks.lock() {
        m.remove(&p);
    }
    Ok(())
}

#[tauri::command]
pub async fn validate_key(
    app: AppHandle,
    state: State<'_, AppState>,
    provider: String,
) -> Result<ember_core::health::KeyCheck, String> {
    let p = parse_provider(&provider)?;
    let (generation, cfg, key) = {
        let generation = state
            .connection_generation
            .lock()
            .map_err(|_| "Connection unavailable")?;
        let cfg = config::load(&app);
        let key = if p == Provider::OpenAi && cfg.openai_auth == config::OpenAiAuth::ChatGpt {
            None
        } else {
            secrets::try_get(p, &cfg.openai_base_url).map_err(|_| "Credential vault unavailable")?
        };
        (*generation, cfg, key)
    };
    let probe = if p == Provider::OpenAi && cfg.openai_auth == config::OpenAiAuth::ChatGpt {
        crate::oauth::probe(state.inner()).await
    } else {
        let Some(key) = key else {
            return Ok(ember_core::health::KeyCheck::Invalid);
        };
        let pctx = providers::ProviderCtx {
            openai_base_url: &cfg.openai_base_url,
        };
        providers::validate(&state.http, p, &key, &pctx).await
    };
    // Commit only to the same connection generation that supplied the credential and URL.
    let current = state
        .connection_generation
        .lock()
        .map_err(|_| "Connection unavailable")?;
    if *current != generation {
        return Err("Connection changed during validation. Try again.".into());
    }
    if let Ok(mut m) = state.key_checks.lock() {
        m.insert(p, (probe.check, crate::now_ms()));
    }
    crate::models_cache::absorb(&app, &state, p, &probe.models);
    Ok(probe.check)
}

/// Listagem de modelos de um provider, para a UI deixar de ter ids escritos a mao. Serve o que
/// foi descoberto no ultimo probe; sem descoberta (offline, sem chave, endpoint sem `/models`)
/// serve a lista embutida com `live: false`, para a UI o poder dizer em vez de fingir.
#[tauri::command]
pub fn list_models(
    app: AppHandle,
    state: State<'_, AppState>,
    provider: String,
) -> Result<crate::models_cache::ModelCatalog, String> {
    let p = parse_provider(&provider)?;
    let cfg = config::load(&app);
    Ok(crate::models_cache::catalog(
        &state,
        p,
        &cfg.openai_base_url,
        cfg.openai_auth,
    ))
}

// ---------------------------------------------------------------------------------------
// Sessao ChatGPT (modo subscricao do slot de fallback)
// ---------------------------------------------------------------------------------------

/// Abre o browser, faz o login com a conta ChatGPT e grava a sessao. Devolve a conta ligada
/// quando o token a diz, para a UI mostrar QUAL e em vez de um "signed in" anonimo.
#[tauri::command]
pub async fn chatgpt_login(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<SettingsDto, String> {
    crate::oauth::login(state.inner()).await?;
    let mut generation = state
        .connection_generation
        .lock()
        .map_err(|_| "Connection unavailable")?;
    *generation += 1;
    // A sessao nova invalida qualquer veredicto anterior sobre este slot.
    if let Ok(mut m) = state.key_checks.lock() {
        m.remove(&Provider::OpenAi);
    }
    let mut cfg = config::load(&app);
    // Fazer login E escolher este modo: obrigar a duas accoes separadas so daria um estado em que
    // ele fez login e continua a ver erros de chave.
    cfg.openai_auth = config::OpenAiAuth::ChatGpt;
    if cfg.openai_model.trim().is_empty()
        || !ember_core::codex::CODEX_MODELS.contains(&cfg.openai_model.as_str())
    {
        cfg.openai_model = ember_core::codex::DEFAULT_CODEX_MODEL.to_string();
    }
    config::save(&app, &cfg).map_err(|e| e.to_string())?;
    crate::models_cache::forget(&state, Provider::OpenAi);
    Ok(build_dto(&app, &cfg))
}

/// Termina a sessao e apaga os tokens do cofre. Ao contrario da chave orfa do Claude, isto
/// apaga-se sem hesitar: e exatamente o que o utilizador pediu ao carregar em sign out.
#[tauri::command]
pub async fn chatgpt_logout(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<SettingsDto, String> {
    state
        .oauth_logged_out
        .store(true, std::sync::atomic::Ordering::SeqCst);
    {
        let mut generation = state
            .connection_generation
            .lock()
            .map_err(|_| "Connection unavailable")?;
        *generation += 1;
    }
    {
        let _commit = state
            .oauth_commit
            .lock()
            .map_err(|_| "Session state unavailable")?;
        state
            .oauth_logged_out
            .store(true, std::sync::atomic::Ordering::SeqCst);
        state
            .oauth_generation
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        crate::secrets::clear_oauth()
            .map_err(|_| "Couldn't clear the session from the credential vault.")?;
    }
    *state.oauth_access.lock().await = None;
    let mut cfg = config::load(&app);
    cfg.openai_auth = config::OpenAiAuth::ApiKey;
    // Volta a um modelo que exista no endpoint por chave, mas so se o que esta la for da
    // subscricao: um id escrito a mao para o endpoint por chave nao se apaga a troco de nada.
    if ember_core::codex::CODEX_MODELS.contains(&cfg.openai_model.as_str()) {
        cfg.openai_model = String::new();
    }
    config::save(&app, &cfg).map_err(|e| e.to_string())?;
    let cfg = config::load(&app);
    if let Ok(mut m) = state.key_checks.lock() {
        m.remove(&Provider::OpenAi);
    }
    crate::models_cache::forget(&state, Provider::OpenAi);
    Ok(build_dto(&app, &cfg))
}

/// Troca qual dos dois providers e tentado primeiro.
///
/// Nao mexe em chaves nem em modelos: os dois slots continuam exatamente como estavam, so muda a
/// ordem por que sao tentados. E por isso que trocar e reversivel a custo zero, e e essa a razao
/// de ser um botao e nao um assistente.
#[tauri::command]
pub fn set_primary_provider(app: AppHandle, provider: String) -> Result<SettingsDto, String> {
    let p = parse_provider(&provider)?;
    let mut cfg = config::load(&app);
    if cfg.primary_provider == p {
        return Ok(build_dto(&app, &cfg));
    }
    cfg.primary_provider = p;
    config::save(&app, &cfg).map_err(|e| e.to_string())?;
    // Nada a invalidar: as chaves, os probes e as listagens pertencem aos PROVIDERS e nao a
    // ordem por que sao tentados, portanto continuam todos validos depois da troca.
    log::info!("provider primario passa a {p:?}");
    Ok(build_dto(&app, &cfg))
}

/// Escolhe como o slot de fallback se autentica, sem passar pelo login. Serve os dois sentidos:
/// voltar a uma chave de API sem perder a sessao gravada, e voltar a subscricao depois disso.
#[tauri::command]
pub fn set_openai_auth(
    app: AppHandle,
    state: State<'_, AppState>,
    mode: String,
) -> Result<SettingsDto, String> {
    let mut generation = state
        .connection_generation
        .lock()
        .map_err(|_| "Connection unavailable")?;
    *generation += 1;

    let mut cfg = config::load(&app);
    cfg.openai_auth = match mode.as_str() {
        "api_key" => config::OpenAiAuth::ApiKey,
        "chat_gpt" => config::OpenAiAuth::ChatGpt,
        other => return Err(format!("unknown auth mode: {other}")),
    };
    // O modelo esta colado ao modo (os `gpt-5.x` so existem na subscricao, e os do Groq/OpenAI so
    // existem por chave), mas so se apaga o que NAO serve no modo novo. Apagar sempre destruia um
    // id escrito a mao a cada ida e volta entre modos, que e a mesma classe de bug que o sanitize
    // do `gemini-3.5-flash` tinha e que acabou de sair daqui.
    let belongs_to_subscription =
        ember_core::codex::CODEX_MODELS.contains(&cfg.openai_model.as_str());
    let wrong_side = match cfg.openai_auth {
        config::OpenAiAuth::ChatGpt => !belongs_to_subscription,
        config::OpenAiAuth::ApiKey => belongs_to_subscription,
    };
    if wrong_side {
        cfg.openai_model = String::new();
    }
    let cfg = {
        config::save(&app, &cfg).map_err(|e| e.to_string())?;
        config::load(&app)
    };
    // A listagem e o veredicto anteriores sao do OUTRO backend: servi-los agora seria mostrar
    // modelos que este nao tem. Mesma higiene que uma mudanca de base URL.
    crate::models_cache::forget(&state, Provider::OpenAi);
    if let Ok(mut m) = state.key_checks.lock() {
        m.remove(&Provider::OpenAi);
    }
    Ok(build_dto(&app, &cfg))
}

/// Veredicto de saude dos providers, para as settings mostrarem um aviso honesto quando nao ha
/// um fallback pre-validado (ex.: so um provider configurado). Le o cache de probes + a presenca
/// das chaves; a decisao e pura (`ember_core::health::assess_providers`).
/// Devolve `Err` se o cofre estiver ilegivel (Bug A): a saude e genuinamente desconhecida.
#[tauri::command]
pub fn get_provider_health(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<ember_core::health::Readiness, String> {
    let auth = config::load(&app).openai_auth;
    let cache = state.key_checks.lock();
    let cache_ref = cache.as_ref().ok();
    let mut entries = Vec::new();
    for p in [Provider::Gemini, Provider::OpenAi] {
        // "Configurado" em modo subscricao quer dizer que ha sessao ChatGPT, e nao que ha chave:
        // sem isto, quem faz login continuava a ver o aviso de "so tens um provider".
        let configured = if p == Provider::OpenAi && auth == config::OpenAiAuth::ChatGpt {
            secrets::has_oauth()
        } else {
            secrets::try_has(p, &config::load(&app).openai_base_url)
        }
        .map_err(|_| "Couldn't read saved keys (credential vault may be locked).".to_string())?;
        entries.push(ember_core::health::ProviderStatus {
            provider: p,
            configured,
            last_check: cache_ref.and_then(|m| m.get(&p).copied()),
        });
    }
    Ok(ember_core::health::assess_providers(
        &entries,
        crate::now_ms(),
        ember_core::health::DEFAULT_TTL_MS,
    ))
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
    // As DUAS janelas de animacao de entrada, nao so a de instalacao. O arranque normal usa a
    // `startup_anim` (ver `lib.rs`: `if is_install { "splash" } else { "startup_anim" }`) e este
    // comando so fechava a `splash`, por isso em todos os arranques que nao eram o primeiro a
    // janela ficava viva para sempre: ecra inteiro, transparente, sempre-a-frente, e uma
    // instancia de WebView2 inteira presa por lancamento. Invisivel no fim da animacao (acaba a
    // opacidade 0) e a deixar passar os cliques, logo ninguem reparava.
    for label in ["splash", "startup_anim"] {
        if let Some(w) = app.get_webview_window(label) {
            let _ = w.close();
        }
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

/// Cria ou atualiza um projeto. Id vazio = e novo, e o id nasce aqui.
///
/// O id e gerado no Rust e nao no lado do JS de proposito: e ele que liga o projeto ativo ao
/// registo, e um id que a UI pudesse escolher acabaria colidido ou derivado do nome (e mudar o
/// nome desligaria o projeto).
#[tauri::command]
pub fn save_project(
    app: AppHandle,
    mut project: ember_core::projects::Project,
) -> Result<SettingsDto, String> {
    let mut cfg = config::load(&app);
    if project.id.trim().is_empty() {
        project.id = new_project_id();
    }
    if project.name.trim().is_empty() {
        return Err("give the project a name first".into());
    }
    match cfg.projects.iter_mut().find(|p| p.id == project.id) {
        Some(existente) => *existente = project,
        None => {
            if cfg.projects.len() >= ember_core::projects::MAX_PROJECTS {
                return Err(format!(
                    "you can have at most {} projects",
                    ember_core::projects::MAX_PROJECTS
                ));
            }
            cfg.projects.push(project);
        }
    }
    save_and_reload(&app, cfg)
}

/// Apaga um projeto. Se era o ativo, o `sanitize` do load limpa o id orfao sozinho, por isso nao
/// ha aqui um segundo sitio a decidir a mesma coisa.
#[tauri::command]
pub fn delete_project(app: AppHandle, id: String) -> Result<SettingsDto, String> {
    let mut cfg = config::load(&app);
    cfg.projects.retain(|p| p.id != id);
    save_and_reload(&app, cfg)
}

/// Select a pinned project or explicitly disable project context.
#[tauri::command]
pub fn set_active_project(app: AppHandle, id: Option<String>) -> Result<SettingsDto, String> {
    let mut cfg = config::load(&app);
    cfg.active_project = id.filter(|i| !i.trim().is_empty());
    cfg.project_context = false;
    let dto = save_and_reload(&app, cfg)?;
    Ok(dto)
}

/// Le a pasta e diz que ficheiro serviria, SEM enviar nada para lado nenhum.
///
/// Existe como comando separado do `distill_project` de proposito: a pessoa tem de conseguir ver
/// o que seria enviado (que ficheiro, quantas linhas, e porque e que aquele ganhou aos outros)
/// ANTES de um repo de cliente sair da maquina dela.
/// The three stops a custom colour would paint with, for the preview next to the hex field.
///
/// A command rather than the same maths written again in TypeScript. The derivation is calibrated
/// against the sixteen fixed accents and lives in one tested place; a second copy in the frontend
/// would drift from it the first time either side is touched, and the user would be previewing a
/// colour the orb never paints.
///
/// `None` for an unparseable hex, which is what lets the field show "not a colour" while the user
/// is still typing instead of flashing an error at every keystroke.
/// The three stops plus where the colour sits on the wheel, so opening the picker puts its marker
/// on the current colour instead of at the centre. The frontend has no colour conversion of its
/// own by design, so the position has to come from here with the stops.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccentPreview {
    #[serde(flatten)]
    stops: ember_core::projects::ResolvedAccent,
    chroma: f64,
    hue: f64,
}

#[tauri::command]
pub fn preview_accent(hex: String) -> Option<AccentPreview> {
    let stops = ember_core::projects::derive_accent(&hex)?;
    let v = ember_core::oklch::to_oklch(ember_core::oklch::parse_hex(&hex)?);
    Some(AccentPreview {
        stops,
        chroma: v.c,
        hue: v.h,
    })
}

/// The stops for a point on the colour wheel. The wheel knows an angle and a radius; turning that
/// into a colour is the same conversion the derivation already owns, so it stays on this side.
#[tauri::command]
pub fn accent_from_wheel(chroma: f64, hue: f64) -> AccentPreview {
    AccentPreview {
        stops: ember_core::projects::accent_from_oklch(
            ember_core::projects::WHEEL_LIGHTNESS,
            chroma,
            hue,
        ),
        chroma,
        hue,
    }
}

#[tauri::command]
pub fn scan_project_folder(path: String) -> Result<crate::projects::Scan, String> {
    let p = std::path::Path::new(&path);
    if !p.is_dir() {
        return Err("that isn't a folder".into());
    }
    Ok(crate::projects::scan(p))
}

/// Le o ficheiro escolhido e devolve um brief. NAO grava nada: o brief volta para a textarea como
/// RASCUNHO, e so entra na config quando a pessoa carregar em Save.
///
/// E essa a defesa de verdade contra um CLAUDE.md envenenado. As molduras e a validacao sao
/// profundidade; o humano a ler antes de gravar e o que impede o problema.
#[tauri::command]
pub async fn distill_project(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
    expected_fingerprint: Option<String>,
) -> Result<String, String> {
    let p = std::path::PathBuf::from(&path);
    crate::projects::distill(&app, state.inner(), &p, expected_fingerprint.as_deref())
        .await
        .map(|(brief, _)| brief)
        .map_err(|e| e.message())
}

/// Recalcula a cor do orb a partir do projeto ativo e guarda-a no estado.
///
/// Chamada no arranque e sempre que a lista ou o projeto ativo mudam. Um sitio so a decidir isto:
/// se cada comando calculasse a sua, um deles acabaria por esquecer e o orb ficava com a cor do
/// projeto anterior sem ninguem perceber porque.
pub(crate) fn refresh_orb_accent(state: &AppState, cfg: &config::Config) {
    let ativo = ember_core::projects::active(&cfg.projects, cfg.active_project.as_deref());
    let cor = ativo.map(|p| {
        // `resolve_accent` decides between the project's custom colour and its palette index; it
        // is pure and tested, and this is the only place the orb colour is built.
        let a = ember_core::projects::resolve_accent(p);
        [a.raw, a.mid, a.glow]
    });
    if let Ok(mut slot) = state.orb_accent.lock() {
        *slot = cor;
    }
    if let Ok(mut slot) = state.orb_project.lock() {
        *slot = ativo.map(|p| p.name.clone());
    }
}

/// Grava e volta a LER do disco antes de devolver o estado.
///
/// A releitura nao e cerimonia: o `sanitize` corre no load e pode ter mexido (descartar um id
/// duplicado, cortar um brief, limpar um ativo orfao). Devolver o que se gravou em vez do que
/// ficou daria uma UI a mostrar algo que o disco nao tem.
fn save_and_reload(app: &AppHandle, cfg: config::Config) -> Result<SettingsDto, String> {
    config::save(app, &cfg).map_err(|e| e.to_string())?;
    let fresca = config::load(app);
    refresh_orb_accent(&app.state::<AppState>(), &fresca);
    Ok(build_dto(app, &fresca))
}

/// Id opaco e aleatorio. Nao deriva do nome nem do caminho: mudar o nome de um projeto, ou mover
/// a pasta, nao pode desligar o que esta ativo.
fn new_project_id() -> String {
    let mut b = [0u8; 8];
    if getrandom::getrandom(&mut b).is_err() {
        // Sem CSPRNG nao ha id unico garantido, mas isto nao e seguranca: e uma etiqueta. O
        // relogio chega, e o `sanitize` descarta duplicados se algum dia colidir.
        return format!("p{}", crate::now_ms());
    }
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Liga/desliga o registo de prompts. Ao DESLIGAR nao apaga o que ja esta gravado: apagar
/// ficheiros do utilizador sem ele pedir e a decisao errada por omissao, e o ficheiro esta no log
/// dir, a um clique do botao que ja abre essa pasta.
#[tauri::command]
pub fn set_save_prompts(app: AppHandle, enabled: bool) -> Result<(), String> {
    crate::prompt_log::set_enabled(&app, enabled)
}

/// Liga/desliga a memoria de refinados. Ao DESLIGAR apaga o ficheiro e esvazia a cache em
/// memoria, ao contrario do `set_save_prompts`: aquele ficheiro existe para o utilizador o ir ler
/// (esta a um clique na pasta de logs), este e interno, e um interruptor de privacidade que so
/// valesse para o futuro deixava o texto antigo em disco sem ninguem dar por isso.
#[tauri::command]
pub fn set_keep_results(app: AppHandle, enabled: bool) -> Result<(), String> {
    crate::refine_store::set_enabled(&app, enabled)
}

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

/// URL do repositorio do projeto (fixo, sem input do utilizador). Fonte unica para o link
/// discreto no About e para nao espalhar a string.
const REPO_URL: &str = "https://github.com/duartelcunha/ember";

/// Abre um URL no browser do SO. Interno ao crate de proposito: so e chamado com URLs que NOS
/// construimos (as constantes deste ficheiro e o URL de autorizacao do `oauth.rs`, feito de
/// constantes mais valores que geramos), nunca com uma string vinda do frontend. Passar um URL
/// arbitrario do webview para um `start`/`open` do SO seria uma superficie de ataque (o `start`
/// do Windows aceita caminhos e protocolos, nao so http).
pub(crate) fn open_in_browser(url: &str) -> Result<(), String> {
    // Windows: usa ShellExecuteW diretamente com a operacao "open", sem passar por cmd.exe.
    // Elimina parsing de linha de comandos e qualquer problema com caracteres especiais na query string.
    #[cfg(target_os = "windows")]
    let result = {
        use windows::core::PCWSTR;
        use windows::Win32::UI::Shell::ShellExecuteW;
        use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

        let op: Vec<u16> = "open\0".encode_utf16().collect();
        let wide_url: Vec<u16> = url.encode_utf16().chain(std::iter::once(0)).collect();

        let hinst = unsafe {
            ShellExecuteW(
                None,
                PCWSTR(op.as_ptr()),
                PCWSTR(wide_url.as_ptr()),
                PCWSTR::null(),
                PCWSTR::null(),
                SW_SHOWNORMAL,
            )
        };
        // ShellExecuteW devolve HINSTANCE; valores superiores a 32 indicam sucesso no Win32.
        if (hinst.0 as usize) > 32 {
            Ok(())
        } else {
            Err(format!(
                "ShellExecuteW failed with error code {:?}",
                hinst.0
            ))
        }
    };
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open")
        .arg(url)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string());
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    let result = std::process::Command::new("xdg-open")
        .arg(url)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string());
    result
}

/// Le um ficheiro de perfil escolhido pelo utilizador no seletor de ficheiros e devolve o texto,
/// para a UI o pôr na textarea (onde ele o pode rever e editar antes de gravar).
///
/// Snapshot, nao ligacao viva: o texto passa a ser o override do utilizador. Se o ficheiro mudar
/// depois, o perfil nao acompanha. E deliberado, porque a textarea continua a ser a verdade
/// visivel do que vai no prompt: um perfil que mudasse pelas costas do utilizador seria pior.
///
/// O caminho vem do seletor NATIVO do SO (o utilizador escolheu-o com o rato), nao de uma string
/// arbitraria do webview, mas mesmo assim: so texto, com teto de tamanho, e nunca um binario.
#[tauri::command]
pub fn read_profile_file(path: String) -> Result<String, String> {
    const MAX_BYTES: u64 = 512 * 1024;
    let p = std::path::Path::new(&path);
    let meta = std::fs::metadata(p).map_err(|e| format!("couldn't read that file: {e}"))?;
    if !meta.is_file() {
        return Err("that isn't a file".into());
    }
    if meta.len() > MAX_BYTES {
        return Err("that file is too big (max 512 KB)".into());
    }
    // `read_to_string` falha em bytes invalidos, que e o que queremos: um PDF ou um .exe
    // escolhido por engano da erro em vez de encher o prompt de lixo.
    std::fs::read_to_string(p).map_err(|_| "that file isn't text (pick a .md or .txt)".to_string())
}

/// Abre o repositorio no browser do SO. URL fixo (constante), por isso seguro para o `start`.
#[tauri::command]
pub fn open_repo() -> Result<(), String> {
    open_in_browser(REPO_URL)
}

/// Consola onde se cria a chave. O frontend so manda o NOME de uma consola conhecida (nunca um
/// URL): assim o webview nunca consegue mandar o SO abrir um endereco arbitrario.
///
/// O provider de fallback e OpenAI-COMPATIBLE e serve varios servicos, por isso a consola nao se
/// deriva do provider mas da Base URL escolhida (o frontend resolve isso e manda o nome).
#[tauri::command]
pub fn open_key_console(provider: String) -> Result<(), String> {
    let url = match provider.as_str() {
        "gemini" => "https://aistudio.google.com/apikey",
        "groq" => "https://console.groq.com/keys",
        "openai" => "https://platform.openai.com/api-keys",
        "openrouter" => "https://openrouter.ai/keys",
        "anthropic" => "https://console.anthropic.com/settings/keys",
        _ => return Err(format!("unknown key console: {provider}")),
    };
    open_in_browser(url)
}

/// Abre a pasta de logs no explorador de ficheiros do SO. Nao partilha o `open_in_browser` de
/// proposito: no Windows, pastas abrem-se com `explorer` direto (um path com `&` ou `^`
/// sobreviveria mal ao parsing do `cmd /C start`); URLs constantes e que vao pelo `start`.
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
    // Atalhos: quais estao configurados e, no Windows, o aviso da elevacao. As duas causas mais
    // comuns de "a hotkey nao faz nada" sao um conflito com outra app (que ja falha no registo,
    // com erro visivel) e a janela em foco ser de um processo elevado (que falha em SILENCIO).
    let elevation = if cfg!(windows) {
        if crate::foreground::is_elevated() {
            "elevated (fires over elevated windows too)"
        } else {
            "not elevated (the hotkey will NOT fire while an elevated window has focus)"
        }
    } else {
        "n/a"
    };
    // Chave orfa do Claude: a app deixou de ter esse provider, mas a credencial pode continuar
    // no cofre. Nao a apagamos sozinhos (e do utilizador), mas esconder que existe seria pior.
    let legacy = if secrets::has_legacy_claude_key() {
        "\nLeftover: an old Claude key is still in your credential vault (unused; remove it there if you want)"
    } else {
        ""
    };
    let slot = |s: &str| {
        if s.trim().is_empty() {
            "(off)".to_string()
        } else {
            s.to_string()
        }
    };
    format!(
        "Ember {version}\nOS: {} ({})\nGemini key: {}\nFallback key: {}\nMode: {}  Thinking: {} ({})  Debug: {}\nFallback endpoint: {}\nHotkeys: main={} polish={} turbo={}\nProcess: {elevation}\nSelect-all fallback: {}{legacy}\nLog: {log_path}",
        std::env::consts::OS,
        std::env::consts::ARCH,
        key_state(Provider::Gemini, &cfg.openai_base_url),
        key_state(Provider::OpenAi, &cfg.openai_base_url),
        mode_str(cfg.mode),
        cfg.thinking_enabled,
        cfg.thinking_level,
        cfg.debug_mode,
        cfg.openai_base_url,
        slot(&cfg.hotkey),
        slot(&cfg.hotkey_polish),
        slot(&cfg.hotkey_turbo),
        if !cfg.select_all_fallback {
            "off".to_string()
        } else if crate::foreground::select_all_is_safe_here() {
            format!("on (max {} chars)", cfg.select_all_max_chars)
        } else {
            // Estado que o utilizador nao consegue explicar sozinho: o toggle esta ligado nas
            // settings e mesmo assim nao corre. Diz porque, em vez de o deixar a adivinhar.
            "on in settings, but inactive on this OS (no terminal detection yet)".to_string()
        },
    )
}

// ---------------------------------------------------------------------------------------
// Refine helper (chamado pelo loop nativo em flow.rs)
// ---------------------------------------------------------------------------------------

/// Quantos modelos alternativos da mesma familia entram na cadeia atras do escolhido.
///
/// Um, e nao tres: cada passo extra custa ate `max_retries_per_step + 1` pedidos e o respetivo
/// backoff ANTES de se chegar a familia de fallback, e o Ember refina no momento (ninguem espera
/// meio minuto por um paragrafo). Um alternativo chega para o caso que motivou isto (o modelo
/// mais recente cheio enquanto o da geracao anterior esta livre) sem transformar uma falha rapida
/// numa espera longa. Subir isto exige medir primeiro quanto tempo custa de verdade.
const MAX_MODEL_ALTERNATES: usize = 1;

/// Constroi a cadeia de tentativa: que providers, com que credencial, com que modelos e por que
/// ordem. Extraida do `refine_text` para a destilacao de um projeto poder usar EXATAMENTE a mesma
/// (ordem escolhida nas settings, sessao ChatGPT renovada, triagem honesta de quem falhou e
/// porque). Uma segunda copia disto ia divergir, e a parte que divergia era a que custou a
/// acertar.
pub(crate) async fn build_chain(
    // `app` nao e usado hoje (a config vem ja carregada), mas fica na assinatura porque a
    // destilacao e o refine partilham esta funcao e o `_` marca-o sem o esconder de vez.
    _app: &AppHandle,
    state: &AppState,
    cfg: &config::Config,
) -> Result<Vec<providers::ChainStep>, ember_core::CoreError> {
    let mut chain: Vec<providers::ChainStep> = Vec::new();
    let mut key_store_failed = false;
    let mut chatgpt_unusable: Option<ember_core::CoreError> = None;
    // A ordem e a que o utilizador escolheu nas settings (default: Gemini primeiro, por ser
    // gratuito). Quem esta primeiro leva o pedido; o outro so entra quando este falha.
    for provider in cfg.provider_order() {
        // Modo subscricao: a credencial nao e uma chave, e um token da sessao ChatGPT, resolvido
        // (e renovado se preciso) UMA vez por refine em vez de uma vez por tentativa.
        let subscription =
            provider == Provider::OpenAi && cfg.openai_auth == crate::config::OpenAiAuth::ChatGpt;
        let credential = if subscription {
            match crate::oauth::access_token(state).await {
                Ok((access_token, account_id)) => providers::Credential::ChatGpt {
                    access_token,
                    account_id,
                },
                // Sessao acabada ou sem rede: este provider fica de fora desta cadeia, mas o
                // Gemini continua a poder servir. Nao rebenta o refine, so degrada com rasto.
                //
                // O motivo fica guardado. Se a cadeia acabar vazia, dizer "sem chave configurada"
                // seria mentira duas vezes: ele configurou o fallback, e o problema e a sessao ter
                // expirado. Sem isto, a app abria as settings a dizer-lhe para pôr uma chave que
                // ele nunca vai pôr, em vez de lhe dizer para voltar a fazer login.
                Err(e) => {
                    log::warn!("sessao ChatGPT indisponivel nesta cadeia: {e:?}");
                    chatgpt_unusable = Some(e);
                    continue;
                }
            }
        } else {
            match secrets::try_get(provider, &cfg.openai_base_url) {
                Ok(Some(k)) => providers::Credential::Key(k),
                Ok(None) => continue,
                // Falha do cofre: nao retirar o provider em silencio. Se ficarmos sem nenhum,
                // reportamos KeyStore (honesto) em vez de "sem providers".
                Err(_) => {
                    key_store_failed = true;
                    continue;
                }
            }
        };
        let chosen = match provider {
            Provider::Gemini => cfg.gemini_model.clone(),
            Provider::OpenAi => cfg.openai_model.clone(),
        };
        // Um passo por MODELO, nao por provider: o escolhido primeiro e, atras dele, alternativos
        // gratuitos da mesma familia tirados da listagem viva. Sem isto, um unico modelo cheio
        // (o `gemini-3.7-flash` devolvia 503 "high demand" em serie, por ser o mais recente e o
        // mais concorrido do free tier) dava a familia inteira por perdida, com a chave boa e
        // mais quinze modelos disponiveis na mesma conta.
        let mut models = vec![chosen.clone()];
        // SO no Gemini, e nao no slot de fallback. Duas razoes, ambas concretas: no OpenRouter
        // isto empilhava-se em cima do failover que ja mandamos DENTRO do pedido (o campo
        // `models`, que ja tenta varios upstreams por chamada), e o segundo lugar da ordenacao
        // num catalogo de centenas de modelos pode ser um modelo de CODIGO, que e mau para prosa
        // (ja aconteceu com o `qwen3-coder:free`). O Gemini nao tem nem uma coisa nem outra: o
        // catalogo e pequeno, e todo de modelos generalistas.
        if provider == Provider::Gemini {
            let catalog = crate::models_cache::catalog(
                state,
                provider,
                &cfg.openai_base_url,
                cfg.openai_auth,
            );
            models.extend(ember_core::models::alternates(
                provider,
                &chosen,
                &catalog.models,
                MAX_MODEL_ALTERNATES,
            ));
        } else if subscription && chosen != ember_core::codex::DEFAULT_CODEX_MODEL {
            // Rede para um modelo que a OpenAI retirou entretanto. Ali nao ha listagem garantida
            // de onde saber isso a tempo, portanto o 404 do refine e que ensina: `ModelNotFound`
            // manda a cadeia para o passo seguinte, e o passo seguinte e um modelo que sabemos
            // existir hoje. Custa zero quando o escolhido funciona (nunca chega a ser tentado) e
            // e o que impede um id morto de transformar todos os refines em erro.
            //
            // Sem risco de gastar dinheiro que ele nao pediu, ao contrario do slot por chave:
            // aqui os modelos vem todos do mesmo plano ja pago.
            models.push(ember_core::codex::DEFAULT_CODEX_MODEL.to_string());
        }
        for model in models {
            chain.push(providers::ChainStep {
                provider,
                credential: credential.clone(),
                model,
            });
        }
    }
    if chain.is_empty() {
        return Err(if key_store_failed {
            ember_core::CoreError::KeyStore
        } else if let Some(e) = chatgpt_unusable {
            // O fallback ESTA configurado; o que falhou foi a sessao. Propaga a razao real
            // (Auth -> "o login expirou", transitorio -> "sem rede") em vez de "sem chave".
            e
        } else {
            ember_core::CoreError::NoProvidersConfigured
        });
    }
    Ok(chain)
}

pub(crate) fn friendly_error(e: &ember_core::CoreError) -> String {
    use ember_core::CoreError::*;
    match e {
        Uncertain => "Request incomplete. It may have been charged. The original is unchanged; retry only when ready.".into(),
        NoProvidersConfigured => "No API key set. Opening settings…".into(),
        // Cobre os dois modos do slot de fallback: uma chave recusada e uma sessao ChatGPT que
        // expirou ou foi revogada dao o mesmo erro, e a accao util e a mesma (ir as settings).
        Auth => "Invalid API key, or your ChatGPT sign-in expired. Check settings.".into(),
        // Acontece de verdade: os providers descontinuam modelos (a Google matou o
        // `gemini-2.5-flash-lite`). O utilizador tem de saber que o problema e o MODELO, nao a
        // chave nem a rede, senao anda a trocar chaves boas as cegas (aconteceu).
        ModelNotFound => "That model no longer exists. Pick another one in settings.".into(),
        ContentPolicy => "Blocked by the provider's content policy.".into(),
        Truncated => "Selection too long for the model. Nothing changed.".into(),
        KeyStore => "Couldn't read your saved keys. Reopen and re-save them.".into(),
        // O caso esmagadoramente comum aqui e o rate-limit das free tiers (Gemini e os modelos
        // `:free` do OpenRouter). Dizer "network or limits" mandava o utilizador a procurar um
        // problema de rede que nao existe; a accao util e esperar ou por uma chave paga.
        AllProvidersFailed => {
            "Rate limited (free tiers) or offline. Wait a moment, or add another key.".into()
        }
        _ => "Couldn't refine. Try again.".into(),
    }
}

/// Tudo o que uma chamada ao modelo precisa, ja resolvido: a cadeia de providers, o pedido, o
/// `Prepared` do motor e a CHAVE DE CACHE deste refine.
///
/// Separado da execucao de proposito. A chave tem de existir ANTES de se gastar dinheiro, para
/// se poder perguntar "isto ja foi refinado?" e para o ciclo seguinte saber que ha uma chamada
/// igual a decorrer e se poder juntar a ela em vez de pagar a mesma coisa outra vez.
pub(crate) struct PreparedRefine {
    pub chain: Vec<providers::ChainStep>,
    pub req: ember_core::LlmRequest,
    pub prepared: ember_core::Prepared,
    pub rcfg: RetryConfig,
    pub openai_base_url: String,
    pub save_prompts: bool,
    pub mode: RefineMode,
    pub project_source: Option<String>,
    pub key: ember_core::CacheKey,
    /// Gravar o refinado em disco quando ele chegar (ver `refine_store`).
    pub keep_results: bool,
    pub retention_generation: u64,
    pub prompt_generation: u64,
}

/// Resolve perfil, contexto de projeto, cadeia e prompt. NAO chama o modelo nem gasta nada.
pub(crate) async fn prepare_refine(
    app: &AppHandle,
    state: &AppState,
    input: &str,
    foreground_title: Option<&str>,
    // O modo deste refine vem do atalho que disparou (ver `flow::RunOpts`), nao de `cfg.mode`:
    // com atalhos por modo, a config so decide o que faz o atalho principal.
    mode: RefineMode,
) -> Result<PreparedRefine, ember_core::CoreError> {
    // Capture policy generations before any await. Re-enabling a policy cannot revive
    // write permissions held by an operation that started before it was disabled.
    let retention_generation = state
        .retention_generation
        .load(std::sync::atomic::Ordering::SeqCst);
    let prompt_generation = state
        .prompt_generation
        .load(std::sync::atomic::Ordering::SeqCst);
    let cfg = config::load(app);
    let resolved = profile::resolve(app, cfg.profile_override.as_deref(), cfg.ignore_claude_md);
    let active = ember_core::projects::active(&cfg.projects, cfg.active_project.as_deref());
    let selection = if active.is_some() {
        "pinned"
    } else if cfg.project_context {
        "auto"
    } else {
        "none"
    };
    let selected = active.or_else(|| {
        if !cfg.project_context {
            return None;
        }
        let home = app.path().home_dir().ok();
        foreground_title
            .and_then(|title| crate::project::resolve(title, home.as_deref(), &cfg.projects))
    });
    let project_block = selected.and_then(|p| ember_core::project::frame_project(&p.brief));
    // Label only the context actually sent. Empty briefs do not impersonate active context.
    let used = selected.filter(|_| project_block.is_some());
    if let Ok(mut label) = state.orb_project.lock() {
        *label = used.map(|p| p.name.clone());
    }
    if let Ok(mut accent) = state.orb_accent.lock() {
        *accent = used.map(|p| {
            let a = ember_core::projects::resolve_accent(p);
            [a.raw.to_string(), a.mid.to_string(), a.glow.to_string()]
        });
    }
    let project_source = used.map(|p| {
        p.source_path
            .clone()
            .unwrap_or_else(|| "User edited brief".into())
    });
    if let Ok(mut snapshot) = state.resolved_context.lock() {
        *snapshot = Some(serde_json::json!({
            "selection": selection,
            "project": used.map(|p| &p.name),
            "projectId": used.map(|p| &p.id),
            "sourceChanged": used.and_then(crate::projects::source_changed),
            "projectSource": project_source,
            "profileSource": resolved.path,
            "profile": ember_core::prompt::cap_profile(&ember_core::project::redact_secrets(&resolved.profile.text), ember_core::prompt::MAX_PROFILE_CHARS),
            "profileTruncated": resolved.profile.text.len() > ember_core::prompt::MAX_PROFILE_CHARS,
            "projectContext": project_block,
            "reason": if selected.is_some() && used.is_none() { "Selected project has an empty brief" }
                else if selection == "none" { "Project context explicitly disabled" }
                else if used.is_none() { "No registered project matches the foreground path" }
                else { "Reviewed brief from a registered project" },
            "configRevision": cfg.revision,
        }));
    }
    let chain = build_chain(app, state, &cfg).await?;
    // Motor Ember, fase 1: normaliza o input, mascara codigo/URLs e escapa marcadores. O modelo
    // ve o `masked_input`; o `prepared` volta para o `flow.rs` reconstruir o output.
    let prepared = ember_core::precondition(input, mode);
    let req = build_llm_request(
        &prepared.masked_input,
        &resolved.profile,
        &cfg.gemini_model,
        mode,
        cfg.thinking_enabled,
        &cfg.thinking_level,
        project_block.as_deref(),
    );
    let rcfg = RetryConfig {
        step_count: chain.len(),
        // A maquina precisa de saber a familia de cada passo para distinguir "tenta outro modelo"
        // de "tenta outro provider": uma chave recusada nao se resolve trocando de modelo.
        step_providers: chain.iter().map(|s| s.provider).collect(),
        ..RetryConfig::default()
    };
    // A chave tem de cobrir TUDO o que muda a resposta, e nao so o texto. O system prompt entra
    // por impressao digital (leva o perfil e o brief do projeto), mas ele sozinho nao chega: o
    // MODELO e as definicoes de thinking nao vivem la dentro, e sem eles trocar de modelo servia
    // o refine do modelo anterior, que e precisamente a experiencia que faria alguem desistir da
    // funcionalidade. O projeto ativo tambem entra, porque muda o contexto sem mudar o texto.
    use sha2::{Digest, Sha256};
    let steps: Vec<_> = chain
        .iter()
        .map(|step| {
            // Only a digest enters the cache identity. Never serialize credentials or account IDs.
            let identity = match &step.credential {
                providers::Credential::Key(key) => key.as_str(),
                providers::Credential::ChatGpt {
                    access_token,
                    account_id,
                } => account_id.as_deref().unwrap_or(access_token),
            };
            serde_json::json!({ "provider": step.provider, "model": step.model,
            "credential": format!("{:x}", Sha256::digest(identity.as_bytes())) })
        })
        .collect();
    let fingerprint_src = serde_json::json!({ "engine": 3, "endpoint": cfg.openai_base_url,
        "auth": cfg.openai_auth, "request": req, "steps": steps })
    .to_string();
    let mut key =
        ember_core::CacheKey::new(input, mode, used.map(|p| p.id.as_str()), &fingerprint_src);
    key.context_digest = Some(format!("{:x}", Sha256::digest(fingerprint_src.as_bytes())));
    Ok(PreparedRefine {
        chain,
        req,
        prepared,
        rcfg,
        openai_base_url: cfg.openai_base_url,
        save_prompts: cfg.save_prompts,
        mode,
        project_source,
        key,
        keep_results: cfg.keep_results,
        retention_generation,
        prompt_generation,
    })
}

/// Chama o modelo com a cadeia preparada. Devolve (texto CRU, provider, modelo): o
/// pos-processamento do motor corre em `flow.rs`, para um output que degrada cair no ramo de
/// restauro do clipboard (nao colar por cima da seleccao).
pub(crate) async fn execute_refine(
    app: &AppHandle,
    state: &AppState,
    p: &PreparedRefine,
    on_attempt: &(dyn Fn(Provider, usize, u32) + Send + Sync),
) -> Result<(String, String, String), ember_core::CoreError> {
    // O preview de streaming fica desligado de proposito: o texto cru pre-engine nao e o que se
    // cola. `on_delta` mantem-se como no-op para a assinatura de `refine`.
    let on_delta = |_delta: &str| {};
    let pctx = providers::ProviderCtx {
        openai_base_url: &p.openai_base_url,
    };
    let started = std::time::Instant::now();
    let resp = providers::refine(
        &state.http,
        &p.rcfg,
        &p.chain,
        &p.req,
        &pctx,
        on_attempt,
        &on_delta,
    )
    .await?;
    // Registo opt-in do que foi mesmo enviado e do que voltou. Depois do `?` de proposito: so se
    // guarda o que chegou a ser uma resposta. Um refine que falhou ja deixa rasto no log normal,
    // e o que aqui interessa estudar e o par prompt/resposta, nao a ausencia dele.
    if p.save_prompts {
        crate::prompt_log::append(
            app,
            &crate::prompt_log::Record {
                generation: p.prompt_generation,
                mode: mode_str(p.mode),
                provider: resp.provider.display_name(),
                model: &resp.model,
                ms: started.elapsed().as_millis(),
                system: &p.req.system,
                input: &p.req.user,
                output: &resp.text,
                project: p.project_source.as_deref(),
            },
        );
    }
    Ok((
        resp.text,
        resp.provider.display_name().to_string(),
        resp.model,
    ))
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
        assert_eq!(parse_provider("openai").unwrap(), Provider::OpenAi);
        assert!(parse_provider("mistral").is_err());
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
        assert_ne!(
            friendly_error(&KeyStore),
            friendly_error(&NoProvidersConfigured)
        );
    }
}

#[tauri::command]
pub fn get_context_snapshot(state: tauri::State<'_, AppState>) -> Option<serde_json::Value> {
    state.resolved_context.lock().ok().and_then(|s| s.clone())
}
