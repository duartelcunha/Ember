// Evita a consola extra no Windows em release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod atomic_file;
#[cfg(windows)]
mod clipboard_snapshot;
mod commands;
mod config;
mod connection;
mod floating;
mod flow;
mod foreground;
mod logging;
mod models_cache;
mod oauth;
mod picker;
mod preview_hook;
mod profile;
mod project;
mod projects;
mod prompt_log;
mod providers;
mod refine_store;
mod secrets;
mod selection;
mod selection_guard;
mod state;

use std::sync::atomic::Ordering;

use ember_core::model::RefineMode;
use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::TrayIconBuilder;
use tauri::window::Color;
use tauri::{AppHandle, Emitter, Manager, WebviewWindow, WebviewWindowBuilder};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

/// Medidas do overlay (faisca, pilula, padding, tamanho da janela) vivem em
/// `ember_core::overlay_geom::DEFAULT_LAYOUT`, com os testes de geometria ao lado delas. Estao
/// ESPELHADAS no frontend: `SPARK_SIZE` (Orb.tsx), `p-2` (Overlay.tsx), `ml-10` (Pill.tsx) e o
/// `width`/`height` da janela "overlay" (tauri.conf.json). Muda uma, muda a outra, senao a
/// orbita descentra-se do ponteiro.
use ember_core::overlay_geom as geom;

/// Um monitor como o SO o descreve: retangulo completo (para saber onde o cursor esta), area
/// util (para clampar sem meter a pilula por baixo da barra de tarefas) e a ESCALA DELE.
///
/// A escala e por monitor e nao da janela de proposito. Perguntar `w.scale_factor()` era o bug:
/// isso descreve o ecra onde a janela ESTA, e o Windows so a corrige no WM_DPICHANGED seguinte.
/// Durante a travessia, os offsets sairiam a escala do ecra anterior.
struct MonitorInfo {
    full: geom::Rect,
    work: geom::Rect,
    scale: f64,
}

fn monitors_of(w: &WebviewWindow) -> Vec<MonitorInfo> {
    let Ok(list) = w.available_monitors() else {
        return Vec::new();
    };
    list.iter()
        .map(|m| {
            let p = m.position();
            let s = m.size();
            let wa = m.work_area();
            MonitorInfo {
                full: geom::Rect::new(p.x, p.y, s.width as i32, s.height as i32),
                work: geom::Rect::new(
                    wa.position.x,
                    wa.position.y,
                    wa.size.width as i32,
                    wa.size.height as i32,
                ),
                scale: m.scale_factor(),
            }
        })
        .collect()
}

/// Monitor (area util) e escala a usar para um ponto, tipicamente o cursor.
///
/// A deteccao usa o retangulo COMPLETO (o cursor pode estar por cima da barra de tarefas) e o
/// clamp usa a area util. Quando o ponto nao cai em monitor nenhum - acontece a serio: com um
/// 1920x1080 encostado a um 2560x1440 o secundario comeca 87px mais abaixo, e a faixa acima dele
/// nao pertence a ecra nenhum - vai-se ao mais PROXIMO. Antes caia-se no monitor da JANELA, que
/// durante o seguimento e o de onde ela veio: o cursor passava para o outro ecra e a orb ficava
/// colada a fronteira do anterior, que e exatamente o "a orb nao passa para o segundo monitor".
pub(crate) fn get_or_create_window(app: &AppHandle, label: &str) -> Option<WebviewWindow> {
    if let Some(w) = app.get_webview_window(label) {
        return Some(w);
    }
    let cfg = app
        .config()
        .app
        .windows
        .iter()
        .find(|w| w.label == label)
        .cloned()?;
    let w = WebviewWindowBuilder::from_config(app, &cfg)
        .ok()?
        .build()
        .ok()?;
    // Fecho da janela settings tratado NATIVAMENTE: o X (ou Alt+F4) esconde a janela em vez de
    // a destruir, para a app continuar na tray. Feito aqui no Rust, nao no JS: o onCloseRequested
    // do lado do webview e fragil (depende do webview estar vivo e responsivo, e deixava a janela
    // presa a preto quando falhava). O evento nativo nunca falha.
    if label == "settings" {
        // Pinta o fundo nativo com a cor do tema guardado ANTES da 1a exibicao: se o tema for
        // creme, a janela nao pisca o escuro do backgroundColor default antes de o CSS aplicar.
        let theme = config::load(app).theme;
        let _ = w.set_background_color(Some(theme_bg(&theme)));

        let win = w.clone();
        w.on_window_event(move |event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = win.hide();
            }
        });
    }
    Some(w)
}

/// O que a overlay mostra agora, para a geometria saber que caixa manter visivel.
pub(crate) fn show_orb_at_cursor(app: &AppHandle) {
    let Some(w) = get_or_create_window(app, "overlay") else {
        return;
    };
    // Cada hotkey novo comeca sempre pelo orb: marca ja aqui (sincrono), antes do loop de
    // seguimento arrancar, para o primeiro frame nao usar a caixa de conteudo da pilula
    // que possa ter ficado de um ciclo anterior.
    {
        let st = app.state::<state::AppState>();
        st.orb_visible.store(true, Ordering::SeqCst);
        st.follow_cursor.store(true, Ordering::SeqCst);
    }
    let _ = w.set_always_on_top(true);
    // Transparente sobre outras apps: nunca intercetar cliques.
    let _ = w.set_ignore_cursor_events(true);
    let mut surface = floating::Surface::new(app.clone(), w.clone(), "ember://overlay-at");
    surface.follow();
    let _ = w.show();
    // NB: nao chamamos set_focus. O paste tem de aterrar na app em foco, nao na nossa.

    // Loop de seguimento: corre enquanto o orb estiver visivel, colado ao cursor.
    //
    // A geracao aposenta o ciclo anterior. Sem isto, com dois refines sobrepostos (a pilula de
    // um ainda no ecra, o outro ja a arrancar) ficavam dois loops vivos a disputar a mesma
    // janela a 120fps, cada um com a sua suavizacao.
    let gen = app
        .state::<state::AppState>()
        .follow_gen
        .fetch_add(1, Ordering::SeqCst)
        + 1;
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move { orb_follow_loop(app2, gen, surface).await });
}

async fn orb_follow_loop(app: AppHandle, gen: u64, mut surface: floating::Surface) {
    let mut tick = tokio::time::interval(std::time::Duration::from_millis(16));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tick.tick().await;
        let state = app.state::<state::AppState>();
        if state.follow_gen.load(Ordering::SeqCst) != gen
            || !state.follow_cursor.load(Ordering::SeqCst)
        {
            break;
        }
        surface.follow();
    }
}

pub(crate) fn hide_orb(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("overlay") {
        let _ = w.hide();
    }
}

/// Cor de fundo nativa da janela por tema (RGBA opaco). Casa com `--color-panel` do CSS de cada
/// tema, para o canvas do WebView2 estar ja da cor certa no frame zero (sem flash antes do CSS).
fn theme_bg(theme: &str) -> Color {
    match theme {
        "cream" => Color(247, 242, 233, 255), // #f7f2e9
        _ => Color(17, 16, 20, 255),          // #111014 (dark, default)
    }
}

/// Pinta o fundo nativo da janela settings com a cor do tema guardado. Chamado na criacao da
/// janela e sempre que o tema muda (set_theme), para nenhuma abertura piscar a cor do outro tema.
pub(crate) fn apply_window_theme(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("settings") {
        let theme = config::load(app).theme;
        let _ = w.set_background_color(Some(theme_bg(&theme)));
    }
}

pub(crate) fn show_settings(app: &AppHandle) {
    // A janela ja existia? So nesse caso emitimos settings-opened, que faz o React re-animar a
    // entrada (remount por `openKey`) E recarregar o estado do Rust. Se a estamos a criar agora,
    // NAO emitimos: o React acabou de montar, ja anima sozinho e ja foi buscar os dados; um emit
    // aqui dava um segundo remount (o conteudo aparecia, desaparecia e voltava) e chegaria antes
    // de o webview ter listener.
    //
    // Com o pre-aquecimento no arranque, o caminho normal e este: a janela existe, esta escondida
    // e ja hidratada, portanto abrir e mostrar + recarregar.
    let existed = app.get_webview_window("settings").is_some();
    let Some(w) = get_or_create_window(app, "settings") else {
        // Never reported before: the window did not exist and could not be created. Silent, this
        // was indistinguishable from "it opened and you cannot see it".
        log::error!("settings: could not get or create the window");
        return;
    };
    {
        // These three calls used to have their errors dropped with `let _ =`. A failing `show()`
        // gave exactly what we saw while debugging this: no window, no clue, nothing in the log.
        // You do not discard what you need to read when things go wrong.
        if let Err(e) = w.center() {
            log::warn!("settings: center failed: {e}");
        }
        if let Err(e) = w.show() {
            log::error!("settings: show failed: {e}");
        }
        if let Err(e) = w.set_focus() {
            log::warn!("settings: set_focus failed: {e}");
        }
        log::info!(
            "settings: shown (already existed={existed}, visible={:?})",
            w.is_visible()
        );
        if existed {
            let _ = w.emit("settings-opened", ());
        }
        // Se o modo debug estiver ligado, abre ja as devtools ao abrir as settings.
        if config::load(app).debug_mode {
            w.open_devtools();
        }
    }
}

/// Timestamp atual em ms (epoch), para o cache de probes de saude dos providers.
pub(crate) fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Pre-valida os fallbacks A ENTRADA (em background, nao bloqueia o arranque): prova, antes de
/// ser preciso, se ha um fallback conhecido-bom, e escreve no cache de saude. Cumpre a regra da
/// casa: o fallback e validado a entrada, nao no momento da falha.
async fn prevalidate_providers(app: AppHandle) {
    let cfg = config::load(&app);
    commands::refresh_orb_accent(&app.state::<state::AppState>(), &cfg);
    for provider in ["gemini", "openai"] {
        let _ =
            commands::validate_key(app.clone(), app.state::<state::AppState>(), provider.into())
                .await;
    }
}

/// Marca `quitting` e sai, uma so vez (guarda `swap` para o comando e o fallback de timeout
/// nao chamarem `exit` duas vezes). Chamado quando a animacao de quit termina, ou pelo fallback.
pub(crate) fn finalize_quit_now(app: &AppHandle) {
    if !app
        .state::<state::AppState>()
        .quitting
        .swap(true, Ordering::SeqCst)
    {
        // Se ha uma chamada ao modelo a decorrer, da-se-lhe um instante para acabar e gravar: ela
        // ja esta paga, e sair a meio deitava fora o resultado, que e precisamente o que este
        // trabalho todo existe para evitar. Limitado a 1.5s, porque um stream preso nao pode
        // impedir a app de fechar; a animacao de quit ja demora 1.2s, portanto quase nunca se ve.
        let app2 = app.clone();
        tauri::async_runtime::spawn(async move {
            const GRACE: std::time::Duration = std::time::Duration::from_millis(1500);
            let started = std::time::Instant::now();
            let mut waited = false;
            while started.elapsed() < GRACE {
                let busy = app2
                    .state::<state::AppState>()
                    .inflight
                    .lock()
                    .map(|f| !f.is_empty())
                    .unwrap_or(false);
                if !busy {
                    break;
                }
                waited = true;
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
            if waited {
                log::info!(
                    "quit: esperou {}ms pela chamada em curso",
                    started.elapsed().as_millis()
                );
            }
            app2.exit(0);
        });
    }
}

/// Abre/fecha as devtools da janela de settings conforme o modo debug (efeito imediato do
/// toggle). Requer a feature `devtools` do tauri, ativa tambem em release para isto funcionar.
pub(crate) fn apply_devtools(app: &AppHandle, enabled: bool) {
    if let Some(w) = app.get_webview_window("settings") {
        if enabled {
            w.open_devtools();
        } else {
            w.close_devtools();
        }
    }
}

/// (Re)regista TODOS os atalhos globais a partir da config: o principal (que usa o modo
/// escolhido nas settings) e os dois que fixam um modo.
///
/// Tudo-ou-nada. O `unregister_all` no inicio deixa a app sem atalho nenhum, por isso um
/// registo que falhe a meio deixaria o utilizador sem forma de disparar o refine, e sem
/// perceber porque. Em caso de erro, tudo o que ja tinha sido registado e desfeito e o erro
/// sobe; quem chama restaura a config anterior (ver `commands::set_hotkeys`).
///
/// Um atalho de modo VAZIO nao e um erro: quer dizer "nao registes este". Quem so quer um
/// atalho fica com um, sem arriscar conflitos com outras apps por causa de dois que nao usa.
/// O que um atalho global dispara. Nasceu quando o picker chegou: um `Option<RefineMode>` ja nao
/// chegava, porque abrir o picker nao e um modo de refinar.
#[derive(Clone, Copy)]
pub(crate) enum HotkeyAction {
    /// Refina. `None` = usa o modo das settings, lido a cada disparo.
    Refine(Option<RefineMode>),
    /// Abre o picker de projetos ao cursor.
    Picker,
}

pub(crate) fn register_hotkeys(app: &AppHandle, cfg: &config::Config) -> Result<(), String> {
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    let wanted: [(&str, HotkeyAction); 4] = [
        (cfg.hotkey.as_str(), HotkeyAction::Refine(None)),
        (
            cfg.hotkey_polish.as_str(),
            HotkeyAction::Refine(Some(RefineMode::Polish)),
        ),
        (
            cfg.hotkey_turbo.as_str(),
            HotkeyAction::Refine(Some(RefineMode::Turbo)),
        ),
        (cfg.hotkey_picker.as_str(), HotkeyAction::Picker),
    ];
    for (combo, action) in wanted {
        if combo.trim().is_empty() {
            continue;
        }
        if let Err(e) = register_one(app, combo, action) {
            let _ = gs.unregister_all();
            return Err(format!("{combo}: {e}"));
        }
    }
    Ok(())
}

/// Escolhe o primeiro atalho da lista de candidatos que o sistema aceite, e devolve-o.
///
/// Existe porque um atalho fixo por omissao nao serve: qualquer combinacao pode ja estar tomada
/// na maquina de alguem, o registo e tudo-ou-nada, e o resultado seria uma instalacao limpa a
/// arrancar com um aviso por causa de um atalho que a pessoa nem escolheu. Testar a lista custa
/// microssegundos e resolve isso de vez.
///
/// `None` se nenhum candidato estiver livre, caso em que o caller fica com o que tinha e abre as
/// settings: nesse ponto so o utilizador pode decidir.
pub(crate) fn first_free_hotkey(app: &AppHandle) -> Option<String> {
    for combo in ember_core::hotkey::DEFAULT_HOTKEY_CANDIDATES {
        if probe_hotkey_free(app, combo) {
            return Some(combo.to_string());
        }
        log::info!("first-run hotkey: {combo} ja esta ocupado, tento o seguinte");
    }
    None
}

/// O SO onde estamos, para a politica pura de atalhos (`ember_core::hotkey`).
pub(crate) fn current_os() -> ember_core::hotkey::Os {
    if cfg!(windows) {
        ember_core::hotkey::Os::Windows
    } else if cfg!(target_os = "macos") {
        ember_core::hotkey::Os::MacOs
    } else {
        ember_core::hotkey::Os::Other
    }
}

/// Tenta registar `accel` so para ver se o SO o aceita, e liberta-o logo a seguir.
///
/// E o unico teste que vale no Windows, onde o `RegisterHotKey` falha mesmo quando outra app ja
/// tem a combinacao. No macOS este teste passa quase sempre (o sistema deixa registar e depois
/// ganha em silencio), e por isso e que a lista de atalhos reservados existe: sao as duas metades
/// da mesma resposta, nenhuma chega sozinha.
///
/// Nao mexe nos atalhos ja registados: regista SO o candidato e desfaz. Se o candidato for um dos
/// nossos, o caller ja o apanhou antes de chegar aqui.
pub(crate) fn probe_hotkey_free(app: &AppHandle, accel: &str) -> bool {
    let gs = app.global_shortcut();
    match gs.register(accel) {
        Ok(()) => {
            let _ = gs.unregister(accel);
            true
        }
        Err(_) => false,
    }
}

/// Regista um atalho. `forced_mode` a `None` significa "o modo que estiver nas settings".
fn register_one(app: &AppHandle, hotkey: &str, action: HotkeyAction) -> Result<(), String> {
    let gs = app.global_shortcut();
    gs.on_shortcut(hotkey, move |app, _shortcut, event| {
        if event.state == ShortcutState::Pressed {
            // O picker tem um caminho proprio, muito mais curto que o do refine: sem captura,
            // sem clipboard, sem orb. As guardas (busy, reentrancia) vivem dentro dele.
            if matches!(action, HotkeyAction::Picker) {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    picker::open_picker(app).await;
                });
                return;
            }
            let HotkeyAction::Refine(forced_mode) = action else {
                return;
            };
            // Um refine a arrancar fecha o picker se ele estiver aberto: dois hooks LL vivos a
            // consumir Enter e o perigo real, e o refine tem prioridade (e o trabalho a serio).
            {
                let st = app.state::<state::AppState>();
                if st.picker_open.load(Ordering::SeqCst) {
                    st.picker_cancel.store(true, Ordering::SeqCst);
                }
            }
            // Guarda de reentrancia. Se ja houver um refine a decorrer, esta segunda tecla
            // CANCELA-o (em vez de arrancar um segundo fluxo, que corromperia o clipboard).
            let st = app.state::<state::AppState>();
            let run_id = match st.begin_run() {
                Ok(id) => id,
                Err(id) => {
                    st.request_dismiss(id);
                    return;
                }
            };
            let lease = flow::RunLease::new(app.clone(), run_id);
            let cfg = config::load(app);
            // Deteta o terminal E captura o titulo da janela (para contexto de projeto) ANTES de
            // mostrar o orb: a app em foco ainda e o alvo, o nosso orb nao rouba o foco.
            let terminal = cfg.terminal_handling && foreground::is_terminal_foreground();
            // O alvo do paste fica fixado AQUI, com a app do utilizador ainda em foco. Verifica-se
            // outra vez mesmo antes de colar: entre as duas coisas pode passar uma chamada longa
            // e um preview de dez segundos, e colar as cegas metia o texto na app errada.
            let target_hwnd = foreground::foreground_target();
            log::info!(
                "[run {run_id}] hotkey: mode={:?} terminal_handling={} exe={:?} -> terminal={}",
                forced_mode.unwrap_or(cfg.mode),
                cfg.terminal_handling,
                foreground::debug_foreground_exe(),
                terminal
            );
            let project_title = if cfg.project_context {
                foreground::foreground_title()
            } else {
                None
            };
            let opts = flow::RunOpts {
                terminal,
                timing: flow::CaptureTiming {
                    polls: cfg.capture_polls,
                    step_ms: cfg.capture_step_ms,
                    settle_ms: cfg.paste_settle_ms,
                },
                project_title,
                preview: cfg.preview_before_paste,
                select_all_fallback: cfg.select_all_fallback
                    && foreground::select_all_is_safe_here(),
                select_all_max_chars: cfg.select_all_max_chars,
                mode: forced_mode.unwrap_or(cfg.mode),
                run_id,
                target_hwnd,
            };
            show_orb_at_cursor(app);
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                flow::run(app.clone(), opts, lease).await;
            });
        }
    })
    .map_err(|e| e.to_string())
}

fn build_tray(app: &tauri::App) -> tauri::Result<()> {
    let open = MenuItemBuilder::with_id("open_settings", "Settings").build(app)?;
    // A saida para tudo o que interrompeu um refine: dispensado, recusado no preview, clipboard
    // ocupado, janela trocada. Guardar o resultado so serve para alguma coisa se houver uma
    // maneira de o aplicar depois.
    let reapply = MenuItemBuilder::with_id("reapply_last", "Reapply last refine").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "Quit").build(app)?;
    let menu = MenuBuilder::new(app)
        .items(&[&open, &reapply, &quit])
        .build()?;
    let Some(icon) = app.default_window_icon().cloned() else {
        // Sem icone nao construimos a tray (em vez de rebentar). A app continua viva; o log
        // deixa rasto. Na pratica o icone vem sempre da config, por isso isto e defensivo.
        log::error!("tray: no default window icon, skipping tray build");
        return Ok(());
    };
    TrayIconBuilder::new()
        .icon(icon)
        .tooltip("Ember")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open_settings" => {
                show_settings(app);
            }
            "reapply_last" => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move { flow::reapply_last(app).await });
            }
            "quit" => {
                if let Some(quit_anim) = get_or_create_window(app, "quit_anim") {
                    let _ = quit_anim.set_ignore_cursor_events(true);
                    let _ = quit_anim.show();
                }
                // A animacao de quit chama `finalize_quit` quando termina: a saida acopla ao
                // fim REAL da animacao, nao a um numero magico que podia divergir do duration.
                // Fallback: se a webview nao completar (falhou a carregar), forca a saida ao
                // fim de um tempo curto, para nunca ficar preso na tray sem sair.
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
                    finalize_quit_now(&app);
                });
            }
            _ => {}
        })
        .build(app)?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Panic hook antes de tudo: em release a consola esta destacada, por isso sem isto um
    // panic nao deixava rasto nenhum. Grava panic + backtrace no log.
    logging::install_panic_hook();
    tauri::Builder::default()
        // single-instance TEM de ser o primeiro plugin.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // Log ON ENTRY: without this there is no way to tell "the second instance never
            // reached the first one" from "it did, and showing the window failed", and those two
            // send you looking in opposite places.
            log::info!("single-instance: second instance detected, showing settings");
            show_settings(app);
        }))
        // Log logo a seguir, para captar a inicializacao dos plugins seguintes.
        .plugin(logging::plugin())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        .manage(state::AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            refine_store::legacy_results_present,
            refine_store::delete_legacy_results,
            commands::get_context_snapshot,
            floating::floating_position,
            flow::overlay_snapshot,
            picker::picker_snapshot,
            commands::set_model,
            commands::set_openai_base_url,
            commands::set_hotkey,
            commands::set_autostart,
            commands::set_mode,
            commands::set_theme,
            commands::set_thinking,
            commands::set_terminal_handling,
            commands::set_project_context,
            commands::set_preview_before_paste,
            commands::set_capture_timing,
            commands::set_api_key,
            commands::clear_api_key,
            commands::validate_key,
            commands::list_models,
            commands::chatgpt_login,
            commands::chatgpt_logout,
            commands::set_openai_auth,
            commands::set_primary_provider,
            commands::check_hotkey,
            commands::set_gemini_model_auto,
            commands::set_select_all_fallback,
            commands::get_provider_health,
            commands::set_profile,
            commands::reload_profile,
            commands::reset_profile,
            commands::close_splash,
            commands::finalize_quit,
            commands::set_debug_mode,
            commands::set_save_prompts,
            commands::set_keep_results,
            commands::save_project,
            commands::delete_project,
            commands::set_active_project,
            commands::preview_accent,
            commands::accent_from_wheel,
            commands::scan_project_folder,
            commands::distill_project,
            commands::read_recent_logs,
            commands::reveal_log_dir,
            commands::open_repo,
            commands::open_key_console,
            commands::read_profile_file,
            commands::get_diagnostics,
        ])
        .setup(|app| {
            let initial = config::load(app.handle());
            if secrets::migrate_legacy_openai(&initial.openai_base_url).is_err() {
                log::warn!("credentials: existing connection key could not be migrated");
            }
            build_tray(app)?;
            let handle = app.handle().clone();

            // Refinados ja pagos de sessoes anteriores. Sem isto, fechar a app deitava fora
            // dinheiro gasto e o mesmo texto voltava a ser cobrado no arranque seguinte.
            if config::load(&handle).keep_results {
                let cache = refine_store::load(&handle);
                if let Ok(mut slot) = handle.state::<state::AppState>().persisted_store.lock() { *slot = cache.clone(); }
                if let Ok(mut slot) = handle.state::<state::AppState>().store.lock() {
                    *slot = cache;
                }
            }

            let maintenance_app = handle.clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                    if maintenance_app.state::<state::AppState>().quitting.load(Ordering::SeqCst) { break; }
                    let app = maintenance_app.clone();
                    let _ = tauri::async_runtime::spawn_blocking(move || refine_store::maintain(&app)).await;
                }
            });

            let is_install = match handle.path().app_data_dir() {
                Ok(app_dir) => {
                    let marker = app_dir.join(".installed");
                    let first_run = !marker.exists();
                    if first_run {
                        if let Err(e) = std::fs::create_dir_all(&app_dir) {
                            log::warn!("install: create_dir_all failed: {e}");
                        }
                        if let Err(e) = std::fs::write(&marker, b"") {
                            log::warn!("install: writing .installed marker failed: {e}");
                        }
                    }
                    first_run
                }
                Err(e) => {
                    log::warn!("install: app_data_dir unavailable: {e}; treating as non-install");
                    false
                }
            };

            let window_name = if is_install { "splash" } else { "startup_anim" };
            match get_or_create_window(&handle, window_name) {
                Some(anim) => {
                    let _ = anim.set_ignore_cursor_events(true);
                    let _ = anim.show();
                    log::info!("startup animation: showing '{window_name}'");
                }
                // A animacao de arranque E o sinal de vida da app: sem ela nao ha nada a dizer
                // ao utilizador que o Ember arrancou. Se a janela nao nasce, isso tem de ficar
                // no log em vez de a app arrancar muda e parecer que nao correu. A causa mais
                // comum e ja haver outra instancia (o WebView2 devolve "resource is in use").
                None => log::error!(
                    "startup animation: could not create '{window_name}'; Ember started with no                      visible sign of life (another instance running?)"
                ),
            }

            // Pre-cria a janela overlay (escondida) para o listener do orb estar pronto
            // antes do primeiro hotkey (senao o evento "refining" perde-se).
            let _ = get_or_create_window(&handle, "overlay");
            // O picker tambem: sem pre-criacao, o primeiro `ember://picker` disparava antes de o
            // webview ter listener e a primeira abertura mostrava uma caixa vazia.
            let _ = get_or_create_window(&handle, "picker");
            // As settings tambem, mas DEPOIS do arranque estar despachado: e a janela mais
            // pesada (920x640, o bundle inteiro do React) e criar-la so no clique fazia pagar
            // ali tudo de uma vez, arrancar o webview, carregar o bundle, montar e ir buscar as
            // definicoes, com a janela ja visivel a meio disso. Aquecida, abrir passa a ser um
            // `show()`. O atraso deixa o arranque da app respirar primeiro; quem clica na tray
            // nos primeiros dois segundos cai no caminho antigo (cria na hora), que continua a
            // funcionar. Nota: a janela nasce escondida (`visible: false` na config), por isso
            // isto nao pisca nada no ecra.
            {
                let h = handle.clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    if h.get_webview_window("settings").is_none() {
                        let _ = get_or_create_window(&h, "settings");
                        log::info!("settings: janela pre-aquecida");
                    }
                });
            }
            // Pre-valida os fallbacks em background (nao bloqueia o arranque).
            tauri::async_runtime::spawn(prevalidate_providers(handle.clone()));
            let mut cfg = config::load(&handle);
            // Escolhe um atalho que esteja mesmo livre nesta maquina em dois casos: no primeiro
            // arranque (em vez de impor um fixo que pode ja estar ocupado), e quando o que esta
            // gravado nao pode ser registado com seguranca.
            //
            // O segundo caso nao e hipotetico: ficou gravado `"hotkey": "Enter"`, que o SO aceita
            // sem se queixar e que a partir dai rouba o Enter a toda a gente enquanto o Ember
            // estiver aberto. Uma config assim tem de ser corrigida no arranque, porque o
            // utilizador nao consegue ligar o sintoma (o Enter deixou de funcionar) a esta causa.
            let saved_verdict = ember_core::hotkey::evaluate(&cfg.hotkey, current_os(), &[]);
            let unsafe_saved = !matches!(saved_verdict, ember_core::hotkey::HotkeyVerdict::Available);
            if is_install || unsafe_saved {
                if unsafe_saved {
                    log::warn!(
                        "saved hotkey '{}' is not safe to register ({saved_verdict:?}); picking another",
                        cfg.hotkey
                    );
                }
                if let Some(free) = first_free_hotkey(&handle) {
                    if free != cfg.hotkey {
                        log::info!("hotkey chosen automatically: {free}");
                        cfg.hotkey = free;
                        if let Err(e) = config::save(&handle, &cfg) {
                            log::warn!("hotkey: could not persist the chosen one: {e}");
                        }
                    }
                }
            }
            log::info!(
                "Ember {} started (install={is_install}, debug={}, hotkey={})",
                handle.package_info().version,
                cfg.debug_mode,
                cfg.hotkey
            );
            // Reconcilia o bool de autostart com o estado real do plugin (a fonte de verdade
            // do SO). Podiam divergir (config editada a mao, entrada removida por fora); sem
            // isto, get_settings mostrava um valor possivelmente obsoleto.
            {
                use tauri_plugin_autostart::ManagerExt;
                if let Ok(actual) = handle.autolaunch().is_enabled() {
                    if actual != cfg.autostart {
                        log::info!("autostart drift: config={}, actual={actual}; syncing config", cfg.autostart);
                        let mut synced = cfg.clone();
                        synced.autostart = actual;
                        if let Err(e) = config::save(&handle, &synced) {
                            log::warn!("autostart: could not persist reconciled state: {e}");
                        }
                    }
                }
            }
            // Se o atalho guardado nao registar (ocupado por outra app, ou invalido de uma
            // versao anterior), abre as settings em vez de arrancar sem hotkey em silencio.
            if let Err(e) = register_hotkeys(&handle, &cfg) {
                // Um dos tres nao registou (ocupado por outra app, ou invalido de uma versao
                // anterior). O registo e tudo-ou-nada, por isso neste ponto a app esta SEM
                // atalho nenhum: um atalho de modo opcional em conflito teria acabado de levar
                // o principal com ele. Tenta outra vez so com o principal, que e o que torna a
                // app utilizavel, e so abre as settings se nem esse registar.
                log::warn!("hotkeys failed to register ({e}); retrying with the main one only");
                let mut only_main = cfg.clone();
                only_main.hotkey_polish.clear();
                only_main.hotkey_turbo.clear();
                match register_hotkeys(&handle, &only_main) {
                    Ok(()) => {
                        log::warn!("main hotkey is up; the per-mode ones are off until fixed");
                        show_settings(&handle);
                    }
                    Err(e2) => {
                        // Nem o principal registou: outra app tem-no. Em vez de arrancar sem
                        // atalho nenhum (a app fica inutil e sem dizer porque), procura um livre
                        // e fica com esse, exatamente como no primeiro arranque.
                        log::warn!("main hotkey '{}' also failed ({e2})", cfg.hotkey);
                        match first_free_hotkey(&handle) {
                            Some(free) => {
                                only_main.hotkey = free.clone();
                                if register_hotkeys(&handle, &only_main).is_ok() {
                                    log::warn!("main hotkey moved to '{free}'");
                                    cfg.hotkey = free;
                                    let _ = config::save(&handle, &cfg);
                                }
                            }
                            None => log::error!("no free hotkey found; Ember has none registered"),
                        }
                        show_settings(&handle);
                    }
                }
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("erro ao construir o Ember")
        .run(|app, event| {
            // Manter o processo vivo na tray quando se fecham janelas, MAS deixar sair
            // quando o utilizador pede Quit explicitamente.
            if let tauri::RunEvent::ExitRequested { api, .. } = event {
                if !app
                    .state::<state::AppState>()
                    .quitting
                    .load(Ordering::SeqCst)
                {
                    api.prevent_exit();
                }
            }
        });
}

#[cfg(windows)]
pub fn purge_credentials_for_uninstall() -> Result<(), String> {
    secrets::purge_for_uninstall()
}
