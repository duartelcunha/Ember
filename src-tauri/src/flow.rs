//! Loop nativo: hotkey -> orb no cursor -> capturar seleccao -> refinar -> substituir.

use tauri::{AppHandle, Emitter, Manager};

use crate::selection::{RealIo, SENTINEL};
use crate::state::AppState;
use crate::{commands, hide_orb, show_settings};
use ember_core::selection as seq;

const STATE_EVENT: &str = "ember://state";

/// Timing de captura/paste, configuravel nas settings (Advanced).
#[derive(Debug, Clone, Copy)]
pub struct CaptureTiming {
    pub polls: u32,
    pub step_ms: u64,
    pub settle_ms: u64,
}

fn emit(app: &AppHandle, phase: &str, message: Option<String>, provider: Option<String>) {
    let _ = app.emit_to(
        "overlay",
        STATE_EVENT,
        serde_json::json!({ "phase": phase, "message": message, "provider": provider }),
    );
}

/// Bloqueante: cria RealIo, captura a seleccao, devolve (texto, clipboard_original).
fn blocking_capture(terminal: bool, timing: CaptureTiming) -> Result<seq::Captured, String> {
    let mut io = RealIo::new(terminal)?;
    Ok(seq::capture(
        &mut io,
        SENTINEL,
        timing.polls,
        timing.step_ms,
    ))
}

/// Bloqueante: substitui a seleccao pelo refinado e restaura o clipboard.
fn blocking_replace(
    refined: String,
    saved: Option<String>,
    terminal: bool,
    settle_ms: u64,
) -> Result<(), String> {
    let mut io = RealIo::new(terminal)?;
    seq::replace(&mut io, &refined, &saved, settle_ms);
    Ok(())
}

/// Bloqueante: restaura o clipboard original (ramos de erro/hint).
fn blocking_restore(saved: Option<String>, terminal: bool) -> Result<(), String> {
    let mut io = RealIo::new(terminal)?;
    seq::restore(&mut io, &saved);
    Ok(())
}

/// Orquestra todo o fluxo. `terminal` = a app em foco e um terminal (Ctrl+Shift+C/V).
pub async fn run(app: AppHandle, terminal: bool, timing: CaptureTiming) {
    emit(&app, "refining", None, None);

    let captured = match tauri::async_runtime::spawn_blocking(move || {
        blocking_capture(terminal, timing)
    })
    .await
    {
        Ok(Ok(c)) => c,
        _ => {
            emit(
                &app,
                "error",
                Some("Couldn't read the selection.".into()),
                None,
            );
            hide_after(&app, 1400).await;
            return;
        }
    };

    let saved = captured.saved.clone();

    let Some(selected) = captured.text else {
        // Nada selecionado: restaura clipboard, hint subtil.
        let s = saved.clone();
        let _ = tauri::async_runtime::spawn_blocking(move || blocking_restore(s, terminal)).await;
        emit(&app, "hint", Some("Select text first".into()), None);
        hide_after(&app, 1400).await;
        return;
    };

    let state = app.state::<AppState>();
    match commands::refine_text(&app, state.inner(), &selected).await {
        Ok((refined, provider)) => {
            let s = saved.clone();
            let r = refined.clone();
            let settle_ms = timing.settle_ms;
            let _ = tauri::async_runtime::spawn_blocking(move || {
                blocking_replace(r, s, terminal, settle_ms)
            })
            .await;
            emit(&app, "success", None, Some(provider));
            hide_after(&app, 650).await;
        }
        Err(e) => {
            let s = saved.clone();
            let _ =
                tauri::async_runtime::spawn_blocking(move || blocking_restore(s, terminal)).await;
            let msg = commands::friendly_error(&e);
            if matches!(e, ember_core::CoreError::NoProvidersConfigured) {
                show_settings(&app);
            }
            emit(&app, "error", Some(msg), None);
            hide_after(&app, 1600).await;
        }
    }
}

async fn hide_after(app: &AppHandle, ms: u64) {
    tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
    hide_orb(app);
}
