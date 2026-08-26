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
    openai_model: String,
    openai_base_url: String,
    gemini_model_auto: bool,
    hotkey: String,
    hotkey_polish: String,
    hotkey_turbo: String,
    autostart: bool,
    has_gemini_key: bool,
    has_openai_key: bool,
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
    project_context: bool,
    preview_before_paste: bool,
    theme: String,
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
        secrets::try_has(Provider::Gemini),
        secrets::try_has(Provider::OpenAi),
    ) {
        (Ok(g), Ok(o)) => (g, o, None),
        (e_g, e_o) => {
            // Pelo menos um falhou a ler o cofre. Loga para diagnostico; a UI mostra banner.
            let any_err = e_g.err().or_else(|| e_o.err());
            log::warn!("settings: credential vault read failed: {:?}", any_err);
            (false, false, Some("credential vault unreadable".to_string()))
        }
    };
    SettingsDto {
        gemini_model: cfg.gemini_model.clone(),
        openai_model: cfg.openai_model.clone(),
        openai_base_url: cfg.openai_base_url.clone(),
        gemini_model_auto: cfg.gemini_model_auto,
        hotkey: cfg.hotkey.clone(),
        hotkey_polish: cfg.hotkey_polish.clone(),
        hotkey_turbo: cfg.hotkey_turbo.clone(),
        autostart: cfg.autostart,
        has_gemini_key: has_g,
        has_openai_key: has_o,
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
        project_context: cfg.project_context,
        preview_before_paste: cfg.preview_before_paste,
        theme: cfg.theme.clone(),
    }
}

/// Presenca de chave para o diagnostico, honesta: distingue configurada / ausente / cofre
/// ilegivel. O diagnostico e best-effort (nao devemos rebentar se o cofre estiver bloqueado).
fn key_state(p: Provider) -> &'static str {
    match secrets::try_has(p) {
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
        let catalog = crate::models_cache::catalog(&state, Provider::Gemini, &cfg.openai_base_url);
        if catalog.live {
            if let Some(best) = ember_core::models::pick_default(Provider::Gemini, &catalog.models) {
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
    let mut cfg = config::load(&app);
    cfg.openai_base_url = base_url;
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
    if !matches!(which.as_str(), "main" | "polish" | "turbo") {
        return Err(format!("invalid hotkey slot: {which}"));
    }
    let cfg = config::load(&app);
    let os = crate::current_os();

    // Regravar a MESMA combinacao no mesmo slot passa sem tocar em nada: ela ja esta registada
    // por nos, e o teste de registo iria falhar com "already registered" e mentir ao utilizador.
    let current = match which.as_str() {
        "main" => &cfg.hotkey,
        "polish" => &cfg.hotkey_polish,
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

/// Grava os tres atalhos de uma vez. `which` = "main" | "polish" | "turbo".
///
/// Regista PRIMEIRO, persiste depois. Se o novo atalho for invalido ou estiver ocupado, restaura
/// o conjunto anterior (o registo faz `unregister_all`, logo sem restauro a app ficava sem atalho
/// nenhum) e NAO grava o atalho partido em disco, senao persistia partido entre arranques. O erro
/// que sobe traz a combinacao e a mensagem do SO, para a UI poder dizer QUAL falhou e porque, em
/// vez de um "nao deu" generico.
#[tauri::command]
pub fn set_hotkey(app: AppHandle, which: String, hotkey: String) -> Result<(), String> {
    let mut cfg = config::load(&app);
    let previous = cfg.clone();
    match which.as_str() {
        "main" => cfg.hotkey = hotkey,
        "polish" => cfg.hotkey_polish = hotkey,
        "turbo" => cfg.hotkey_turbo = hotkey,
        _ => return Err(format!("invalid hotkey slot: {which}")),
    }
    crate::register_hotkeys(&app, &cfg).map_err(|e| {
        let _ = crate::register_hotkeys(&app, &previous);
        e
    })?;
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
pub fn set_api_key(state: State<'_, AppState>, provider: String, key: String) -> Result<(), String> {
    let p = parse_provider(&provider)?;
    secrets::set(p, &key).map_err(|e| e.to_string())?;
    // A chave mudou: o probe antigo deixa de valer. Tira do cache (fica "por revalidar").
    if let Ok(mut m) = state.key_checks.lock() {
        m.remove(&p);
    }
    Ok(())
}

#[tauri::command]
pub fn clear_api_key(state: State<'_, AppState>, provider: String) -> Result<(), String> {
    let p = parse_provider(&provider)?;
    secrets::delete(p).map_err(|e| e.to_string())?;
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
    // Bug A: ler pelo try_get, nao pelo get engolidor. Um cofre bloqueado devolve Err -> a UI
    // mostra o erro (toast), em vez de tratar silenciosamente como "chave invalida".
    let key = secrets::try_get(p).map_err(|_| {
        "Couldn't read the key from the credential vault (it may be locked).".to_string()
    })?;
    let Some(key) = key else {
        return Ok(ember_core::health::KeyCheck::Invalid);
    };
    let cfg = config::load(&app);
    let pctx = providers::ProviderCtx {
        gemini_model: &cfg.gemini_model,
        openai_model: &cfg.openai_model,
        openai_base_url: &cfg.openai_base_url,
    };
    let probe = providers::validate(&state.http, p, &key, &pctx).await;
    // Guarda o resultado no cache de saude, para a pre-validacao/o veredicto refletirem ja.
    if let Ok(mut m) = state.key_checks.lock() {
        m.insert(p, (probe.check, crate::now_ms()));
    }
    // A mesma resposta trouxe a listagem de modelos: guarda-a e reconcilia a escolha gravada.
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
    Ok(crate::models_cache::catalog(&state, p, &cfg.openai_base_url))
}

/// Veredicto de saude dos providers, para as settings mostrarem um aviso honesto quando nao ha
/// um fallback pre-validado (ex.: so um provider configurado). Le o cache de probes + a presenca
/// das chaves; a decisao e pura (`ember_core::health::assess_providers`).
/// Devolve `Err` se o cofre estiver ilegivel (Bug A): a saude e genuinamente desconhecida.
#[tauri::command]
pub fn get_provider_health(
    state: State<'_, AppState>,
) -> Result<ember_core::health::Readiness, String> {
    let cache = state.key_checks.lock();
    let cache_ref = cache.as_ref().ok();
    let mut entries = Vec::new();
    for p in [Provider::Gemini, Provider::OpenAi] {
        let configured = secrets::try_has(p)
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

/// URL do repositorio do projeto (fixo, sem input do utilizador). Fonte unica para o link
/// discreto no About e para nao espalhar a string.
const REPO_URL: &str = "https://github.com/duartelcunha/ember";

/// Abre um URL no browser do SO. PRIVADO de proposito: so e chamado com constantes deste
/// ficheiro, nunca com uma string vinda do frontend. Passar um URL arbitrario do webview para um
/// `start`/`open` do SO seria uma superficie de ataque (o `start` do Windows aceita caminhos e
/// protocolos, nao so http).
fn open_in_browser(url: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    let result = std::process::Command::new("cmd")
        .args(["/C", "start", "", url])
        .spawn();
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open").arg(url).spawn();
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    let result = std::process::Command::new("xdg-open").arg(url).spawn();
    result.map(|_| ()).map_err(|e| e.to_string())
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
        key_state(Provider::Gemini),
        key_state(Provider::OpenAi),
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

pub(crate) fn friendly_error(e: &ember_core::CoreError) -> String {
    use ember_core::CoreError::*;
    match e {
        NoProvidersConfigured => "No API key set. Opening settings…".into(),
        Auth => "Invalid API key. Check settings.".into(),
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

/// Refina `input` com a chain Gemini->fallback (filtrada pelos configurados). Devolve
/// (texto CRU do modelo, `Prepared`, provider) ou CoreError: o pos-processamento do motor corre
/// em `flow.rs`, para um output que degrada cair no ramo de restauro do clipboard (nao colar por
/// cima da seleccao). `on_attempt` recebe (provider, indice, tentativa) antes de cada chamada;
/// `on_delta` e no-op (preview off).
pub(crate) async fn refine_text(
    app: &AppHandle,
    state: &AppState,
    input: &str,
    foreground_title: Option<&str>,
    // O modo deste refine vem do atalho que disparou (ver `flow::RunOpts`), nao de `cfg.mode`:
    // com atalhos por modo, a config so decide o que faz o atalho principal.
    mode: RefineMode,
    on_attempt: &(dyn Fn(Provider, usize, u32) + Send + Sync),
    on_delta: &(dyn Fn(&str) + Send + Sync),
) -> Result<(String, ember_core::Prepared, String), ember_core::CoreError> {
    let cfg = config::load(app);
    let mut chain: Vec<(Provider, String)> = Vec::new();
    let mut key_store_failed = false;
    // Ordem de prioridade: Gemini primario (gratuito), depois o slot de fallback.
    for provider in [Provider::Gemini, Provider::OpenAi] {
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
    // Contexto de projeto (best-effort, so quando ligado): junta o CLAUDE.md/AGENTS.md do projeto
    // em foco ao perfil global. Qualquer falha -> None -> segue so com o global (comportamento
    // de sempre). O `foreground_title` so vem preenchido quando `config.project_context` esta on.
    let project_ctx = foreground_title.and_then(|t| {
        let home = app.path().home_dir().ok();
        crate::project::resolve(t, home.as_deref())
    });
    match &project_ctx {
        Some(pc) => log::info!("project context: merged {}", pc.source_path),
        None if foreground_title.is_some() => {
            log::debug!("project context: enabled, none detected for the foreground window")
        }
        None => {}
    }
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
        project_ctx.as_ref().map(|pc| pc.block.as_str()),
    );
    let rcfg = RetryConfig {
        provider_count: chain.len(),
        ..RetryConfig::default()
    };
    let pctx = providers::ProviderCtx {
        gemini_model: &cfg.gemini_model,
        openai_model: &cfg.openai_model,
        openai_base_url: &cfg.openai_base_url,
    };
    let resp = providers::refine(&state.http, &rcfg, &chain, &req, &pctx, on_attempt, on_delta)
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
        assert_ne!(friendly_error(&KeyStore), friendly_error(&NoProvidersConfigured));
    }
}
