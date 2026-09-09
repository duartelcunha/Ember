//! The tray icon and the menu it opens.
//!
//! The menu is a webview window of ours rather than the Win32 context menu. It was the one
//! surface the user meets every day that spoke another visual language, and it could not be
//! animated at all: it appeared, and it vanished. Now it opens out of the icon with the house
//! clip-path and folds back into it before the window hides, the same gesture as the overlay.
//!
//! Focus rules: this window MAY take focus. It is not the overlay nor the picker (nothing is
//! pasted from here), and the native menu it replaces took focus too, so a refine in flight is
//! no worse off than before.

use std::sync::atomic::Ordering;
use std::time::Instant;

use ember_core::overlay_geom as geom;
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize};

use crate::state::AppState;

/// Logical size of the popup. Mirrored by `width`/`height` of the "tray" window in
/// tauri.conf.json and by `HEADER_H`/`ITEM_H`/`PAD` in `src/tray/TrayMenu.tsx`, which have to
/// add up to the height; change one, change the others, or the surface is clipped by its window.
const POPUP: (f64, f64) = (208.0, 128.0);
/// How long the surface has to fold back into the icon before the window hides. Mirrors the
/// 140ms `.ember-tray [data-leave]` takes in `src/styles/globals.css`.
const CLOSE_MS: u64 = 140;
/// Payload `{ open: bool, below: bool }`, to the "tray" window only. `below` says which edge faces
/// the icon, so the surface opens out of it.
pub(crate) const EVENT: &str = "ember://tray";

/// Why the menu is closing. Only a blur arms the reopen guard: the click that blurred us is
/// about to arrive as a tray event, and it must not open the menu it just closed. After Esc or a
/// choice there is no such click in flight, and a real click within the guard is a real request.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClosedBy {
    Blur,
    Request,
}

pub(crate) fn build_tray(app: &tauri::App) -> tauri::Result<()> {
    let Some(icon) = app.default_window_icon().cloned() else {
        // Without an icon there is no tray (rather than a crash). The app stays alive and the log
        // says why. In practice the icon always comes from the config; this is defensive.
        log::error!("tray: no default window icon, skipping tray build");
        return Ok(());
    };
    TrayIconBuilder::new()
        .icon(icon)
        .tooltip("Ember")
        // No `.menu()` at all: with one attached, the right button would still get the native
        // menu and the app would have two different menus on the same icon.
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left | MouseButton::Right,
                button_state: MouseButtonState::Up,
                rect,
                ..
            } = event
            {
                open_menu(tray.app_handle(), rect);
            }
        })
        .build(app)?;
    Ok(())
}

fn open_menu(app: &AppHandle, icon: tauri::Rect) {
    let state = app.state::<AppState>();
    let since_hide = state
        .tray_hidden_at
        .lock()
        .ok()
        .and_then(|t| *t)
        .map(|t| t.elapsed().as_millis() as u64);
    if !ember_core::tray::reopens_after(since_hide) {
        log::debug!("tray: click inside the blur guard ({since_hide:?}ms), not reopening");
        return;
    }
    let Some(w) = crate::get_or_create_window(app, "tray") else {
        log::error!("tray: could not get or create the menu window");
        return;
    };
    if state.tray_open.swap(true, Ordering::SeqCst) {
        // Already open (a second click that somehow did not blur): bring it back. Unless the
        // window is not actually visible, in which case the flag is stale (a show or focus that
        // failed, a blur that never came) and the honest thing is to place and show it again,
        // rather than to focus a hidden window on every click from now on.
        if w.is_visible().unwrap_or(false) {
            if let Err(e) = w.set_focus() {
                log::warn!("tray: refocus failed: {e}");
            }
            return;
        }
        log::warn!("tray: open flag set but the window is hidden; reopening");
    }
    // The icon rect comes in the tray's own coordinates, physical on Windows. The monitor is the
    // one under the icon's centre, found the way floating.rs finds the cursor's, so a taskbar on
    // a secondary screen with another DPI is placed with THAT screen's scale.
    let monitors = crate::monitors_of(&w);
    let scale_hint = monitors.first().map(|m| m.scale).unwrap_or(1.0);
    let pos = icon.position.to_physical::<i32>(scale_hint);
    let size = icon.size.to_physical::<i32>(scale_hint);
    let icon_px = geom::Rect::new(pos.x, pos.y, size.width.max(1), size.height.max(1));
    let rects: Vec<_> = monitors.iter().map(|m| m.full).collect();
    let centre = (icon_px.x + icon_px.w / 2, icon_px.y + icon_px.h / 2);
    let Some((full, _)) = geom::monitor_for_point(centre.0, centre.1, &rects) else {
        log::warn!("tray: no monitor under the icon at {centre:?}");
        state.tray_open.store(false, Ordering::SeqCst);
        return;
    };
    let Some(monitor) = monitors.iter().find(|m| m.full == full) else {
        state.tray_open.store(false, Ordering::SeqCst);
        return;
    };
    let popup_px = (
        (POPUP.0 * monitor.scale).round() as i32,
        (POPUP.1 * monitor.scale).round() as i32,
    );
    let at = ember_core::tray::popup_origin(icon_px, popup_px, monitor.work);
    // Position first so the window lands on the icon's monitor, then a PHYSICAL size: a logical
    // one would be resolved against whichever DPI the window had a frame ago.
    let _ = w.set_position(PhysicalPosition::new(at.x, at.y));
    let _ = w.set_size(PhysicalSize::new(popup_px.0 as u32, popup_px.1 as u32));
    // Clickable again: the close makes it click-through while it folds.
    let _ = w.set_ignore_cursor_events(false);
    let _ = app.emit_to("tray", EVENT, serde_json::json!({ "open": true, "below": at.below }));
    // These two used to be dropped with `let _ =`. A failing show on an always-on-top window is
    // exactly the case that leaves the open flag stale, and the log is where that has to show.
    if let Err(e) = w.show() {
        log::error!("tray: show failed: {e}");
    }
    if let Err(e) = w.set_focus() {
        log::warn!("tray: set_focus failed: {e}");
    }
    log::debug!(
        "tray: menu opened at ({},{}) below={} on scale {}",
        at.x, at.y, at.below, monitor.scale
    );
}

/// Folds the menu back into the icon and hides the window. Safe to call twice: blur and Esc can
/// both arrive for the same close, and hiding a focused window blurs it again.
pub(crate) fn close_menu(app: &AppHandle, by: ClosedBy) {
    let state = app.state::<AppState>();
    if !state.tray_open.swap(false, Ordering::SeqCst) {
        return;
    }
    // Recorded at the START of the close, and only for a blur: the click that caused it arrives
    // right after, well before the surface has finished folding.
    if let Ok(mut t) = state.tray_hidden_at.lock() {
        *t = (by == ClosedBy::Blur).then(Instant::now);
    }
    log::debug!("tray: menu closing (blur={})", by == ClosedBy::Blur);
    // While it folds, the window is still on screen and on top of everything: a click landing
    // on it must not choose Quit, so it stops being a target now, not in 140ms.
    if let Some(w) = app.get_webview_window("tray") {
        let _ = w.set_ignore_cursor_events(true);
    }
    let _ = app.emit_to("tray", EVENT, serde_json::json!({ "open": false }));
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(CLOSE_MS)).await;
        // Reopened during the fold? Then the window is theirs now.
        if app.state::<AppState>().tray_open.load(Ordering::SeqCst) {
            return;
        }
        if let Some(w) = app.get_webview_window("tray") {
            let _ = w.hide();
        }
    });
}

/// The menu's only way to talk back. One command with a verb rather than three commands: it is
/// one capability entry, one manifest entry, and the verbs are visible in one place.
#[tauri::command]
pub fn tray_action(app: AppHandle, action: String) -> bool {
    match action.as_str() {
        // The webview asks on mount whether it missed an `open` emitted before it had a listener
        // (the first click can land before the warm-up finished loading the page).
        "ready" => app.state::<AppState>().tray_open.load(Ordering::SeqCst),
        "close" => {
            close_menu(&app, ClosedBy::Request);
            false
        }
        "settings" => {
            close_menu(&app, ClosedBy::Request);
            // The menu folds first and only then the window shows: both at once made two
            // animations fight for the same frames, and the window's arrival was the one that
            // stuttered.
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(CLOSE_MS)).await;
                crate::show_settings(&app);
            });
            false
        }
        "quit" => {
            // Once. An Enter held down repeats the key; the second arrival finds the menu
            // already closed and must not start a second quit animation and a second timer.
            if app.state::<AppState>().tray_open.load(Ordering::SeqCst) {
                close_menu(&app, ClosedBy::Request);
                crate::begin_quit(&app);
            }
            false
        }
        other => {
            log::warn!("tray: unknown action {other:?}");
            false
        }
    }
}
