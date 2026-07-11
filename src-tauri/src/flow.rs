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

/// Quanto tempo esperar pela libertacao natural dos modificadores antes de forcar os key-ups
/// (ver `ember_core::selection::capture`). Curto: nao confiamos no `GetAsyncKeyState` como sinal
/// de libertacao (com o hotkey global registado, ele reporta Ctrl+Shift em baixo durante ~1.5s
/// mesmo depois de largar, causa raiz confirmada por logs). O `capture` forca sempre os key-ups
/// + settle logo a seguir, por isso esta espera e so um afago inicial, nao a defesa principal.
const NEUTRALIZE_TIMEOUT_MS: u64 = 60;

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
        terminal,
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
    // No terminal, achata para uma linha: um `\n` no meio submetia o comando a meio (cada linha
    // executaria em separado). Fora do terminal, o texto original (com paragrafos) e preservado.
    let to_paste = if terminal {
        seq::flatten_for_terminal(&refined)
    } else {
        refined
    };
    let armed = seq::replace(&mut io, &to_paste, &saved, settle_ms);
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
/// `project_title` = titulo da janela em foco (para contexto de projeto), ou `None` se desligado.
/// `preview` = mostrar um gate de aprovacao (Enter aplica, Esc mantem) antes de colar.
pub async fn run(
    app: AppHandle,
    terminal: bool,
    timing: CaptureTiming,
    project_title: Option<String>,
    preview: bool,
) {
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

    // Diagnostico do terminal (so comprimentos, nunca o conteudo, e um segredo do utilizador):
    // armed? copiou alguma coisa? quantos chars? E o sinal que separa "nao armou / clipboard
    // ocupado" de "copiou nada" de "copiou tarde".
    log::info!(
        "capture: terminal={} armed={} text_len={:?} saved_len={:?}",
        terminal,
        captured.armed,
        captured.text.as_ref().map(|t| t.chars().count()),
        saved.as_ref().map(|s| s.chars().count()),
    );

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

    // Feedback de progresso honesto: torna visivel o retry e o fallback (nao a cauda do texto
    // a ser gerado, que sao tokens internos e nao o que sera colado). O orb + "Trying/Retrying
    // {provider}" chega para o utilizador perceber que ainda esta a trabalhar.
    let app_cb = app.clone();
    let on_attempt = move |provider: Provider, idx: usize, attempt: u32| {
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

    // O preview de streaming fica desligado de proposito (ver acima): o texto cru pre-engine
    // nao e o que se cola. `on_delta` mantem-se como no-op para a assinatura de `refine`.
    let on_delta = |_delta: &str| {};

    let state = app.state::<AppState>();
    // Refina com cancelamento: corre em `select!` contra o `cancel_notify`, para a segunda
    // tecla poder abortar a chamada HTTP a meio (o drop do future cancela o pedido reqwest).
    let refine_fut = commands::refine_text(
        &app,
        state.inner(),
        &selected,
        project_title.as_deref(),
        &on_attempt,
        &on_delta,
    );
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
        Ok((raw, prepared, provider)) => {
            if cancelled(&app) {
                abort_cancelled(&app, saved, image, terminal).await;
                return;
            }
            // Motor Ember, fase 2: limpa/desmascara/valida o texto CRU do modelo. Um Degrade
            // (output vazio, ou um span de codigo/URL perdido) cai no ramo de restauro: a
            // seleccao fica intacta em vez de colarmos algo partido por cima.
            match ember_core::postprocess(&raw, &prepared) {
                ember_core::EngineResult::Paste(refined) => {
                    // Gate de preview (opt-in): mostra um pill de aprovacao e espera Enter/Esc.
                    // Fora do preview, `Accept` direto (comportamento de sempre). Ramifica-se ANTES
                    // de mover `image` para o `blocking_replace`, porque o reject precisa dele.
                    let decision = if preview {
                        emit(
                            &app,
                            "preview",
                            Some("Enter to apply \u{00b7} Esc to keep original".into()),
                            None,
                        );
                        crate::preview_hook::gate(app.clone()).await
                    } else {
                        crate::preview_hook::Decision::Accept
                    };

                    match decision {
                        crate::preview_hook::Decision::Accept => {
                            let s = saved.clone();
                            let settle_ms = timing.settle_ms;
                            let pasted = tauri::async_runtime::spawn_blocking(move || {
                                blocking_replace(refined, s, image, terminal, settle_ms)
                            })
                            .await;
                            match pasted {
                                Ok(Ok(true)) => {
                                    finish(&app, FlowOutcome::Success { provider }).await;
                                }
                                _ => {
                                    // O refinado nao chegou a ser armado no clipboard (ocupado). A
                                    // seleccao ficou intacta: nao reportar "Refined" falso.
                                    finish(&app, FlowOutcome::PasteFailed).await;
                                }
                            }
                        }
                        crate::preview_hook::Decision::Reject => {
                            // Como o abort de cancelamento: restaura o clipboard, mantem o original.
                            let s = saved.clone();
                            let _ = tauri::async_runtime::spawn_blocking(move || {
                                blocking_restore(s, image, terminal)
                            })
                            .await;
                            finish(&app, FlowOutcome::PreviewRejected).await;
                        }
                    }
                }
                ember_core::EngineResult::Degrade(reason) => {
                    log::warn!("engine degraded ({reason:?}); clipboard restored, nothing pasted");
                    let s = saved.clone();
                    let _ = tauri::async_runtime::spawn_blocking(move || {
                        blocking_restore(s, image, terminal)
                    })
                    .await;
                    finish(&app, FlowOutcome::RefineUnclean).await;
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
