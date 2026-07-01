// Evita a consola extra no Windows em release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod config;
mod flow;
mod foreground;
mod profile;
mod providers;
mod secrets;
mod selection;
mod state;

use std::sync::atomic::Ordering;

use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, PhysicalPosition, WebviewWindow, WebviewWindowBuilder};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

/// Offset do orb em relacao ao cursor (centro do orb ~ cursor + isto), em px fisicos.
const ORB_OFFSET: i32 = 18;

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

/// Top-left desejado da janela do orb para o cursor atual: poe o centro do orb
/// (centro da janela) junto ao cursor + offset, clampado ao monitor.
fn orb_target(app: &AppHandle, w: &WebviewWindow) -> Option<(i32, i32)> {
    let c = app.cursor_position().ok()?;
    let (ww, wh) = match w.outer_size() {
        Ok(s) => (s.width as i32, s.height as i32),
        Err(_) => (300, 140),
    };
    let tlx = c.x as i32 + ORB_OFFSET - ww / 2;
    let tly = c.y as i32 + ORB_OFFSET - wh / 2;
    let (ax, ay, aw, ah) = monitor_at_point(w, c.x as i32, c.y as i32);
    Some(ember_core::selection::clamp_pos(
        tlx, tly, ww, wh, ax, ay, aw, ah,
    ))
}

/// Posiciona o orb junto ao cursor (snap), mostra-o sem foco e arranca o loop de seguimento.
pub(crate) fn show_orb_at_cursor(app: &AppHandle) {
    let Some(w) = get_or_create_window(app, "overlay") else {
        return;
    };
    let _ = w.set_always_on_top(true);
    // Transparente sobre outras apps: nunca intercetar cliques.
    let _ = w.set_ignore_cursor_events(true);
    if let Some((x, y)) = orb_target(app, &w) {
        let _ = w.set_position(PhysicalPosition::new(x, y));
    }
    let _ = w.show();
    // NB: nao chamamos set_focus — o paste tem de aterrar na app em foco, nao na nossa.

    // Loop de seguimento: corre enquanto o orb estiver visivel, com suavizacao (lerp).
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move { orb_follow_loop(app2).await });
}

/// Segue o cursor suavemente enquanto o orb esta visivel. Termina quando `hide_orb` esconde.
async fn orb_follow_loop(app: AppHandle) {
    let Some(w) = app.get_webview_window("overlay") else {
        return;
    };
    let mut pos = w
        .outer_position()
        .map(|p| (p.x as f64, p.y as f64))
        .unwrap_or((0.0, 0.0));
    loop {
        if !matches!(w.is_visible(), Ok(true)) {
            break;
        }
        if let Some((tx, ty)) = orb_target(&app, &w) {
            pos.0 += (tx as f64 - pos.0) * 0.28;
            pos.1 += (ty as f64 - pos.1) * 0.28;
            let _ = w.set_position(PhysicalPosition::new(
                pos.0.round() as i32,
                pos.1.round() as i32,
            ));
        }
        tokio::time::sleep(std::time::Duration::from_millis(16)).await;
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
    }
}

/// (Re)regista o atalho global a partir de uma string (ex: "CmdOrCtrl+Shift+Space").
pub(crate) fn register_hotkey(app: &AppHandle, hotkey: &str) -> Result<(), String> {
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    gs.on_shortcut(hotkey, move |app, _shortcut, event| {
        if event.state == ShortcutState::Pressed {
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
            tauri::async_runtime::spawn(async move { flow::run(app, terminal, timing).await });
        }
    })
    .map_err(|e| e.to_string())
}

fn build_tray(app: &tauri::App) -> tauri::Result<()> {
    let open = MenuItemBuilder::with_id("open_settings", "Settings").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "Quit").build(app)?;
    let menu = MenuBuilder::new(app).items(&[&open, &quit]).build()?;
    let icon = app.default_window_icon().cloned().unwrap();
    TrayIconBuilder::new()
        .icon(icon)
        .tooltip("Ember")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open_settings" => show_settings(app),
            "quit" => {
                // Marca a saida deliberada para o handler de ExitRequested deixar sair.
                app.state::<state::AppState>()
                    .quitting
                    .store(true, Ordering::SeqCst);
                app.exit(0);
            }
            _ => {}
        })
        .build(app)?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // single-instance TEM de ser o primeiro plugin.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_settings(app);
        }))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_positioner::init())
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
        ])
        .setup(|app| {
            build_tray(app)?;
            let handle = app.handle().clone();
            // Pre-cria a janela overlay (escondida) para o listener do orb estar pronto
            // antes do primeiro hotkey (senao o evento "refining" perde-se).
            let _ = get_or_create_window(&handle, "overlay");
            let cfg = config::load(&handle);
            let _ = register_hotkey(&handle, &cfg.hotkey);
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
