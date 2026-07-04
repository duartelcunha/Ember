// Evita a consola extra no Windows em release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod config;
mod flow;
mod foreground;
mod logging;
mod profile;
mod providers;
mod secrets;
mod selection;
mod state;

use std::sync::atomic::Ordering;

use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, PhysicalPosition, WebviewWindow, WebviewWindowBuilder, Emitter};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

/// Offset do orb em relacao ao cursor (centro do orb ~ cursor + isto), em px fisicos.
/// X positivo = mais a direita, Y positivo = mais para baixo.
const ORB_OFFSET_X: i32 = 24;
const ORB_OFFSET_Y: i32 = 4;

/// Tamanho do conteudo visivel do orb (o pontinho + alguma folga), usado para clampar
/// ao monitor. A janela do overlay e fixa em 300x140 (para caber a pilula de erro), mas
/// o orb e so um pontinho de 13px centrado la dentro: clampar a janela toda fazia o orb
/// afastar-se muito do cursor perto das bordas do ecra.
const ORB_CONTENT_SIZE: (i32, i32) = (20, 20);

/// Tamanho da janela do overlay, espelhando a declaracao em `tauri.conf.json` (label
/// "overlay"). So usado como fallback se `w.outer_size()` falhar (raro); nomeado para nao
/// ter o mesmo par de numeros duplicado sem explicacao em dois ficheiros.
const OVERLAY_FALLBACK_SIZE: (i32, i32) = (300, 140);

/// Obtem (ou cria) uma janela declarada com `create:false`. NAO a mostra (o caller decide
/// posicao/foco antes de `show`, para o orb nao piscar na posicao errada).
fn get_or_create_window(app: &AppHandle, label: &str) -> Option<WebviewWindow> {
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
    WebviewWindowBuilder::from_config(app, &cfg)
        .ok()?
        .build()
        .ok()
}

/// Geometria do monitor atual da janela (para clampar o orb ao ecra).
fn monitor_work_area(w: &WebviewWindow) -> (i32, i32, i32, i32) {
    if let Ok(Some(mon)) = w.current_monitor() {
        let p = mon.position();
        let s = mon.size();
        (p.x, p.y, s.width as i32, s.height as i32)
    } else {
        (0, 0, 1920, 1080)
    }
}

/// Geometria do monitor que contem o ponto (px,py), tipicamente o cursor. Ao contrario
/// de `monitor_work_area`, nao depende de onde a janela esta agora, por isso o orb
/// consegue atravessar para outro ecra em vez de ficar preso na borda do monitor de
/// origem quando o cursor muda de ecra a meio do seguimento.
fn monitor_at_point(w: &WebviewWindow, px: i32, py: i32) -> (i32, i32, i32, i32) {
    let monitors: Vec<(i32, i32, i32, i32)> = w
        .available_monitors()
        .map(|ms| {
            ms.iter()
                .map(|m| {
                    let p = m.position();
                    let s = m.size();
                    (p.x, p.y, s.width as i32, s.height as i32)
                })
                .collect()
        })
        .unwrap_or_default();
    ember_core::selection::monitor_containing(px, py, &monitors)
        .unwrap_or_else(|| monitor_work_area(w))
}

/// Top-left desejado da janela do overlay para o cursor atual. O conteudo esta alinhado a
/// esquerda e centrado na vertical (ver Overlay.tsx), com o padding `p-2` (8px logicos) a
/// separar do canto. Ancoramos o BORDO ESQUERDO do conteudo (nao o centro) junto ao cursor
/// + offset, para o conteudo crescer para a direita: a pilula e larga e, centrada, cairia
/// por cima do rato em vez de aparecer ao lado como o orb.
fn orb_target(app: &AppHandle, w: &WebviewWindow) -> Option<(i32, i32)> {
    let c = app.cursor_position().ok()?;
    let (ww, wh) = match w.outer_size() {
        Ok(s) => (s.width as i32, s.height as i32),
        Err(_) => OVERLAY_FALLBACK_SIZE,
    };
    let pad = (8.0 * w.scale_factor().unwrap_or(1.0)).round() as i32;
    let anchor_x = c.x as i32 + ORB_OFFSET_X;
    let anchor_y = c.y as i32 + ORB_OFFSET_Y;
    let win_x = anchor_x - pad;
    let win_y = anchor_y - wh / 2;
    let (ax, ay, aw, ah) = monitor_at_point(w, c.x as i32, c.y as i32);
    let is_orb = app
        .state::<state::AppState>()
        .orb_visible
        .load(Ordering::SeqCst);
    if is_orb {
        // Orb: minusculo dentro de uma janela grande. Clampa so a caixa visivel ao ecra,
        // senao perto das bordas a janela era contida e o orb afastava-se do cursor.
        let (cw, ch) = ORB_CONTENT_SIZE;
        Some(ember_core::selection::clamp_window_for_content(
            win_x,
            win_y,
            pad,
            (wh - ch) / 2,
            cw,
            ch,
            ax,
            ay,
            aw,
            ah,
        ))
    } else {
        // Pilula: ocupa a janela quase toda, basta manter a janela dentro do ecra.
        Some(ember_core::selection::clamp_pos(
            win_x, win_y, ww, wh, ax, ay, aw, ah,
        ))
    }
}

/// Posiciona o orb junto ao cursor (snap), mostra-o sem foco e arranca o loop de seguimento.
pub(crate) fn show_orb_at_cursor(app: &AppHandle) {
    let Some(w) = get_or_create_window(app, "overlay") else {
        return;
    };
    // Cada hotkey novo comeca sempre pelo orb: marca ja aqui (sincrono), antes do loop de
    // seguimento arrancar, para o primeiro frame nao usar a caixa de conteudo da pilula
    // que possa ter ficado de um ciclo anterior.
    app.state::<state::AppState>()
        .orb_visible
        .store(true, Ordering::SeqCst);
    let _ = w.set_always_on_top(true);
    // Transparente sobre outras apps: nunca intercetar cliques.
    let _ = w.set_ignore_cursor_events(true);
    if let Some((x, y)) = orb_target(app, &w) {
        let _ = w.set_position(PhysicalPosition::new(x, y));
    }
    let _ = w.show();
    // NB: nao chamamos set_focus. O paste tem de aterrar na app em foco, nao na nossa.

    // Loop de seguimento: corre enquanto o orb estiver visivel, colado ao cursor.
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move { orb_follow_loop(app2).await });
}

/// Segue o cursor com suavizacao exponencial (lerp) enquanto o orb esta visivel, para um
/// arrasto fluido tipo Apple em vez de saltos. Termina quando `hide_orb` esconde. Usa um
/// `interval` a 120fps (nao `sleep`, que acumula deriva). A suavizacao usa o dt REAL via
/// `alpha = 1 - exp(-dt/tau)`: assim mantem a mesma sensacao mesmo que um tick atrase (um
/// factor fixo por frame mudava de velocidade com o frame-rate, um bug subtil de engasgo).
async fn orb_follow_loop(app: AppHandle) {
    let Some(w) = app.get_webview_window("overlay") else {
        return;
    };
    let mut tick = tokio::time::interval(std::time::Duration::from_secs_f64(1.0 / 120.0));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    // Constante de tempo da suavizacao: cobre ~63% da distancia ao alvo a cada SMOOTH_TAU s.
    // Mais baixo = mais colado ao cursor; mais alto = mais fluido/preguicoso.
    const SMOOTH_TAU: f64 = 0.05;
    let mut current: Option<(f64, f64)> = None;
    let mut last = tokio::time::Instant::now();

    loop {
        if !matches!(w.is_visible(), Ok(true)) {
            break;
        }
        let now = tokio::time::Instant::now();
        let dt = (now - last).as_secs_f64();
        last = now;
        if let Some((tx, ty)) = orb_target(&app, &w) {
            let (tx, ty) = (tx as f64, ty as f64);
            let (nx, ny) = match current {
                // Primeiro frame: snap ao alvo (sem arrasto a partir do canto).
                None => (tx, ty),
                Some((cx, cy)) => {
                    let alpha = 1.0 - (-dt / SMOOTH_TAU).exp();
                    (cx + (tx - cx) * alpha, cy + (ty - cy) * alpha)
                }
            };
            let _ = w.set_position(PhysicalPosition::new(nx.round() as i32, ny.round() as i32));
            current = Some((nx, ny));
        }
        tick.tick().await;
    }
}

pub(crate) fn hide_orb(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("overlay") {
        let _ = w.hide();
    }
}

pub(crate) fn show_settings(app: &AppHandle) {
    if let Some(w) = get_or_create_window(app, "settings") {
        let _ = w.center();
        let _ = w.show();
        let _ = w.set_focus();
        let _ = w.emit("settings-opened", ());
        // Se o modo debug estiver ligado, abre ja as devtools ao abrir as settings.
        if config::load(app).debug_mode {
            w.open_devtools();
        }
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

/// (Re)regista o atalho global a partir de uma string (ex: "CmdOrCtrl+Shift+Space").
pub(crate) fn register_hotkey(app: &AppHandle, hotkey: &str) -> Result<(), String> {
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    gs.on_shortcut(hotkey, move |app, _shortcut, event| {
        if event.state == ShortcutState::Pressed {
            // Guarda de reentrancia. Se ja houver um refine a decorrer, esta segunda tecla
            // CANCELA-o (em vez de arrancar um segundo fluxo, que corromperia o clipboard).
            let st = app.state::<state::AppState>();
            if st.busy.swap(true, Ordering::SeqCst) {
                st.cancel.store(true, Ordering::SeqCst);
                st.cancel_notify.notify_waiters();
                return;
            }
            // Arranque limpo: sem cancelamento pendente de um ciclo anterior.
            st.cancel.store(false, Ordering::SeqCst);
            let cfg = config::load(app);
            // Deteta o terminal ANTES de mostrar o orb (a app em foco e ainda o alvo).
            let terminal = cfg.terminal_handling && foreground::is_terminal_foreground();
            let timing = flow::CaptureTiming {
                polls: cfg.capture_polls,
                step_ms: cfg.capture_step_ms,
                settle_ms: cfg.paste_settle_ms,
            };
            show_orb_at_cursor(app);
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                flow::run(app.clone(), terminal, timing).await;
                // Liberta a guarda so no fim do ciclo (o orb ja foi escondido dentro de run):
                // ate aqui, o hide_after deste ciclo nao pode ser pisado por outra tecla.
                app.state::<state::AppState>()
                    .busy
                    .store(false, Ordering::SeqCst);
            });
        }
    })
    .map_err(|e| e.to_string())
}

fn build_tray(app: &tauri::App) -> tauri::Result<()> {
    let open = MenuItemBuilder::with_id("open_settings", "Settings").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "Quit").build(app)?;
    let menu = MenuBuilder::new(app).items(&[&open, &quit]).build()?;
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
            "quit" => {
                if let Some(quit_anim) = get_or_create_window(app, "quit_anim") {
                    let _ = quit_anim.set_ignore_cursor_events(true);
                    let _ = quit_anim.show();
                }
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(650)).await;
                    app.state::<state::AppState>()
                        .quitting
                        .store(true, Ordering::SeqCst);
                    app.exit(0);
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
            show_settings(app);
        }))
        // Log logo a seguir, para captar a inicializacao dos plugins seguintes.
        .plugin(logging::plugin())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        .manage(state::AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::set_model,
            commands::set_hotkey,
            commands::set_autostart,
            commands::set_mode,
            commands::set_thinking,
            commands::set_terminal_handling,
            commands::set_capture_timing,
            commands::set_api_key,
            commands::clear_api_key,
            commands::validate_key,
            commands::set_profile,
            commands::reload_profile,
            commands::reset_profile,
            commands::close_splash,
            commands::set_debug_mode,
            commands::read_recent_logs,
            commands::reveal_log_dir,
            commands::get_diagnostics,
        ])
        .setup(|app| {
            build_tray(app)?;
            let handle = app.handle().clone();
            
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
            if let Some(anim) = get_or_create_window(&handle, window_name) {
                let _ = anim.set_ignore_cursor_events(true);
                let _ = anim.show();
            }
            
            // Pre-cria a janela overlay (escondida) para o listener do orb estar pronto
            // antes do primeiro hotkey (senao o evento "refining" perde-se).
            let _ = get_or_create_window(&handle, "overlay");
            let cfg = config::load(&handle);
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
            if let Err(e) = register_hotkey(&handle, &cfg.hotkey) {
                log::warn!("hotkey '{}' failed to register ({e}); opening settings", cfg.hotkey);
                show_settings(&handle);
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
