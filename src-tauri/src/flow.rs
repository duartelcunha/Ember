//! Loop nativo: hotkey -> orb no cursor -> capturar seleccao -> refinar -> substituir.

use std::sync::atomic::Ordering;

use tauri::{AppHandle, Emitter, Manager};

use crate::selection::{ClipImage, RealIo, SENTINEL};
use crate::state::AppState;
use crate::{commands, hide_orb, show_settings};
use ember_core::model::Provider;
use ember_core::overlay::{feedback_for, FlowOutcome};
use ember_core::selection as seq;

const STATE_EVENT: &str = "ember://state";

/// Quanto tempo esperar pela libertacao natural dos modificadores do hotkey antes de forcar
/// os key-ups (ver `ember_core::selection::capture`).
const NEUTRALIZE_TIMEOUT_MS: u64 = 200;

/// Tamanho maximo da pre-visualizacao mostrada junto ao orb enquanto o texto e gerado.
/// A legenda e uma bolha pequena junto ao cursor, nao uma janela de chat: mostra so a
/// CAUDA do texto (o que acabou de ser escrito), cortada, nao o texto inteiro. Curto de
/// proposito: durante o orb (fase "refining") a janela clampa so a caixa minuscula do orb
/// ao ecra (ver `ORB_CONTENT_SIZE` em lib.rs), nao a legenda; um texto longo arriscava sair
/// da janela fixa de 300px perto de uma borda do ecra.
const STREAM_PREVIEW_MAX_CHARS: usize = 36;

/// Timing de captura/paste, configuravel nas settings (Advanced).
#[derive(Debug, Clone, Copy)]
pub struct CaptureTiming {
    pub polls: u32,
    pub step_ms: u64,
    pub settle_ms: u64,
}

fn emit(app: &AppHandle, phase: &str, message: Option<String>, provider: Option<String>) {
    app.state::<AppState>()
        .orb_visible
        .store(phase == "refining", Ordering::SeqCst);
    let _ = app.emit_to(
        "overlay",
        STATE_EVENT,
        serde_json::json!({ "phase": phase, "message": message, "provider": provider }),
    );
}

/// Resultado da captura: a seleccao sequenciada, um snapshot de imagem a repor (quando o
/// clipboard original era uma imagem) e `unpreservable` = o clipboard tem conteudo que nao
/// sabemos preservar (ficheiros/RTF), caso em que nada foi tocado e o fluxo aborta.
struct CaptureOutput {
    captured: seq::Captured,
    image: Option<ClipImage>,
    unpreservable: bool,
}

/// Bloqueante: cria RealIo, captura a seleccao preservando um clipboard de imagem.
fn blocking_capture(terminal: bool, timing: CaptureTiming) -> Result<CaptureOutput, String> {
    let mut io = RealIo::new(terminal)?;
    // Conteudo que nao conseguimos repor (ficheiros do Explorer, etc.): nem toca no clipboard.
    if io.has_unpreservable_content() {
        return Ok(CaptureOutput {
            captured: seq::Captured {
                text: None,
                saved: None,
                armed: false,
            },
            image: None,
            unpreservable: true,
        });
    }
    // Snapshot da imagem ANTES de a captura escrever o sentinela (senao perdia-se).
    let image = io.snapshot_image();
    let captured = seq::capture(
        &mut io,
        SENTINEL,
        timing.polls,
        timing.step_ms,
        NEUTRALIZE_TIMEOUT_MS,
    );
    Ok(CaptureOutput {
        captured,
        image,
        unpreservable: false,
    })
}

/// Bloqueante: substitui a seleccao pelo refinado e restaura o clipboard original. Se o
/// original era uma imagem (sem texto guardado), repoe a imagem por cima do refinado depois
/// do paste. Devolve `true` se o refinado chegou mesmo ao clipboard (ver `seq::replace`).
fn blocking_replace(
    refined: String,
    saved: Option<String>,
    image: Option<ClipImage>,
    terminal: bool,
    settle_ms: u64,
) -> Result<bool, String> {
    let mut io = RealIo::new(terminal)?;
    let armed = seq::replace(&mut io, &refined, &saved, settle_ms);
    if saved.is_none() {
        if let Some(img) = &image {
            io.restore_image(img);
        }
    }
    Ok(armed)
}

/// Bloqueante: restaura o clipboard original (ramos de erro/hint): texto se havia, senao a
/// imagem snapshot.
fn blocking_restore(
    saved: Option<String>,
    image: Option<ClipImage>,
    terminal: bool,
) -> Result<(), String> {
    let mut io = RealIo::new(terminal)?;
    if saved.is_some() {
        seq::restore(&mut io, &saved);
    } else if let Some(img) = &image {
        io.restore_image(img);
    }
    Ok(())
}

/// `true` se foi pedido cancelamento (segunda tecla) ao ciclo em curso.
fn cancelled(app: &AppHandle) -> bool {
    app.state::<AppState>().cancel.load(Ordering::SeqCst)
}

/// Emite o feedback e agenda o esconder a partir de um resultado terminal do fluxo. Um so
/// sitio a decidir "o que mostrar e por quanto tempo" (`ember_core::overlay::feedback_for`),
/// em vez de cada chamador embutir a sua propria string e o seu proprio numero magico.
async fn finish(app: &AppHandle, outcome: FlowOutcome) {
    let fb = feedback_for(outcome);
    emit(app, fb.phase, fb.message, fb.provider);
    hide_after(app, fb.hide_after_ms).await;
}

/// Restaura o clipboard (texto ou imagem) e mostra "Cancelled" brevemente. Usado nos ramos
/// de cancelamento, para a seleccao do utilizador ficar sempre intacta.
async fn abort_cancelled(
    app: &AppHandle,
    saved: Option<String>,
    image: Option<ClipImage>,
    terminal: bool,
) {
    let _ = tauri::async_runtime::spawn_blocking(move || blocking_restore(saved, image, terminal))
        .await;
    finish(app, FlowOutcome::Cancelled).await;
}

/// Orquestra todo o fluxo. `terminal` = a app em foco e um terminal (Ctrl+Shift+C/V).
pub async fn run(app: AppHandle, terminal: bool, timing: CaptureTiming) {
    emit(&app, "refining", None, None);

    let out = match tauri::async_runtime::spawn_blocking(move || {
        blocking_capture(terminal, timing)
    })
    .await
    {
        Ok(Ok(o)) => o,
        _ => {
            finish(&app, FlowOutcome::CaptureFailed).await;
            return;
        }
    };

    if out.unpreservable {
        // O clipboard tem conteudo que nao sabemos repor (ficheiros, etc.). Nao lhe tocamos.
        finish(&app, FlowOutcome::UnpreservableClipboard).await;
        return;
    }

    let captured = out.captured;
    let image = out.image;
    let saved = captured.saved.clone();

    if !captured.armed {
        // Nao foi possivel armar o sentinela: o clipboard estava ocupado por outra app. A
        // seleccao do utilizador ficou intacta. Diz a verdade em vez de "Select text first".
        finish(&app, FlowOutcome::ClipboardBusy).await;
        return;
    }

    let Some(selected) = captured.text else {
        // Nada selecionado: restaura clipboard, hint subtil.
        let s = saved.clone();
        let _ =
            tauri::async_runtime::spawn_blocking(move || blocking_restore(s, image, terminal)).await;
        finish(&app, FlowOutcome::NoSelectionFound).await;
        return;
    };

    if cancelled(&app) {
        abort_cancelled(&app, saved, image, terminal).await;
        return;
    }

    // Feedback de progresso: torna visivel o retry, o fallback E o proprio texto a ser
    // gerado, em vez de um orb mudo. `preview` acumula o texto do intento ATUAL; cada novo
    // intento (retry ou fallback) limpa-o, para nao misturar texto descartado com o novo.
    let preview = std::sync::Arc::new(std::sync::Mutex::new(String::new()));

    let app_cb = app.clone();
    let preview_for_attempt = preview.clone();
    let on_attempt = move |provider: Provider, idx: usize, attempt: u32| {
        if let Ok(mut p) = preview_for_attempt.lock() {
            p.clear();
        }
        let msg = if idx == 0 && attempt == 0 {
            None // primeira tentativa do provider primario: o "refining" ja esta a mostra
        } else if attempt > 0 {
            Some(format!("Retrying {}...", provider.display_name()))
        } else {
            Some(format!("Trying {}...", provider.display_name()))
        };
        if let Some(m) = msg {
            emit(&app_cb, "refining", Some(m), None);
        }
    };

    let on_delta = move |_delta: &str| {
        // The user requested not to see the streaming text preview
        // before the "Refined." message appears.
    };

    let state = app.state::<AppState>();
    // Refina com cancelamento: corre em `select!` contra o `cancel_notify`, para a segunda
    // tecla poder abortar a chamada HTTP a meio (o drop do future cancela o pedido reqwest).
    let refine_fut = commands::refine_text(&app, state.inner(), &selected, &on_attempt, &on_delta);
    tokio::pin!(refine_fut);
    let outcome = loop {
        tokio::select! {
            r = &mut refine_fut => break Some(r),
            _ = state.cancel_notify.notified() => {
                if state.cancel.load(Ordering::SeqCst) {
                    break None;
                }
            }
        }
    };

    let Some(refine_result) = outcome else {
        abort_cancelled(&app, saved, image, terminal).await;
        return;
    };

    match refine_result {
        Ok((refined, provider)) => {
            if cancelled(&app) {
                abort_cancelled(&app, saved, image, terminal).await;
                return;
            }
            let s = saved.clone();
            let r = refined.clone();
            let settle_ms = timing.settle_ms;
            let pasted = tauri::async_runtime::spawn_blocking(move || {
                blocking_replace(r, s, image, terminal, settle_ms)
            })
            .await;
            match pasted {
                Ok(Ok(true)) => {
                    finish(&app, FlowOutcome::Success { provider }).await;
                }
                _ => {
                    // O refinado nao chegou a ser armado no clipboard (ocupado). A seleccao
                    // ficou intacta, mas nao a substituimos: nao reportar "Refined" falso.
                    finish(&app, FlowOutcome::PasteFailed).await;
                }
            }
        }
        Err(e) => {
            let s = saved.clone();
            let _ = tauri::async_runtime::spawn_blocking(move || {
                blocking_restore(s, image, terminal)
            })
            .await;
            let message = commands::friendly_error(&e);
            if matches!(e, ember_core::CoreError::NoProvidersConfigured) {
                show_settings(&app);
            }
            finish(&app, FlowOutcome::RefineFailed { message }).await;
        }
    }
}

async fn hide_after(app: &AppHandle, ms: u64) {
    tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
    hide_orb(app);
    // Repoe o overlay em "hidden" para o DOM esvaziar: sem isto, a pilula do ciclo
    // anterior fica montada e, como o orb partilha `layoutId` com ela, o hotkey seguinte
    // faz o orb MORPHAR da pilula velha (desliza, sem fade) em vez de montar de novo e
    // aparecer com fade no sitio certo.
    emit(app, "hidden", None, None);
}
