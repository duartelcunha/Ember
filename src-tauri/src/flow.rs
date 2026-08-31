//! Loop nativo: hotkey -> orb no cursor -> capturar seleccao -> refinar -> substituir.

use std::sync::atomic::Ordering;

use tauri::{AppHandle, Emitter, Manager};

use crate::selection::{ClipImage, RealIo, SENTINEL};
use crate::state::AppState;
use crate::{commands, hide_orb, show_settings};
use ember_core::model::{Provider, RefineMode};
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

/// Tudo o que um ciclo precisa de saber, decidido no instante em que a hotkey dispara (a app em
/// foco ainda e o alvo). Agrupado num struct em vez de argumentos posicionais: sao seis, e uma
/// troca acidental entre dois `bool` vizinhos nao daria erro de compilacao.
#[derive(Debug, Clone)]
pub struct RunOpts {
    /// A app em foco e um terminal (Ctrl+Shift+C/V, achatar o paste, sem select-all).
    pub terminal: bool,
    pub timing: CaptureTiming,
    /// Titulo da janela em foco, para contexto de projeto. `None` = desligado.
    pub project_title: Option<String>,
    /// Gate de aprovacao antes de colar (Enter aplica, Esc mantem).
    pub preview: bool,
    /// Sem seleccao, seleciona o campo em foco e refina-o todo.
    pub select_all_fallback: bool,
    /// Teto de chars aceite numa captura vinda do select-all.
    pub select_all_max_chars: usize,
    /// O modo com que refinar. Vem do atalho que disparou, nao da config: os atalhos de modo
    /// fixam-no, o principal usa o que esta escolhido nas settings.
    pub mode: RefineMode,
}

fn emit(app: &AppHandle, phase: &str, message: Option<String>, provider: Option<String>) {
    let state = app.state::<AppState>();
    state
        .orb_visible
        .store(phase == "refining", Ordering::SeqCst);
    // Segue o cursor enquanto ha trabalho a decorrer E enquanto o preview espera resposta: nos
    // dois casos o utilizador ainda esta no meio da accao, e a overlay tem de estar onde ele
    // esta a olhar. As pilulas de resultado ficam onde nasceram: sao passageiras e persegui-las
    // com os olhos custava mais do que valia.
    state
        .follow_cursor
        .store(phase == "refining" || phase == "preview", Ordering::SeqCst);
    // A cor do projeto ativo viaja com o estado: e o unico sinal que diz, em cada refine, com que
    // projeto ele esta a ser feito. Sem isto, um projeto ativo e invisivel e da para refinar uma
    // semana com o contexto errado sem dar por nada.
    let accent = state.orb_accent.lock().ok().and_then(|a| a.clone());
    let project = state.orb_project.lock().ok().and_then(|a| a.clone());
    let _ = app.emit_to(
        "overlay",
        STATE_EVENT,
        serde_json::json!({
            "phase": phase, "message": message, "provider": provider,
            "accent": accent, "project": project
        }),
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
fn blocking_capture(
    terminal: bool,
    timing: CaptureTiming,
    select_all_fallback: bool,
) -> Result<CaptureOutput, String> {
    let mut io = RealIo::new(terminal)?;
    // Conteudo que nao conseguimos repor (ficheiros do Explorer, etc.): nem toca no clipboard.
    if io.has_unpreservable_content() {
        return Ok(CaptureOutput {
            captured: seq::Captured {
                text: None,
                saved: None,
                armed: false,
                via_select_all: false,
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
        select_all_fallback,
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

/// Orquestra todo o fluxo: hotkey -> orb -> capturar -> refinar -> colar. Ver `RunOpts`.
pub async fn run(app: AppHandle, opts: RunOpts) {
    let RunOpts {
        terminal,
        timing,
        project_title,
        preview,
        select_all_fallback,
        select_all_max_chars,
        mode,
    } = opts;
    emit(&app, "refining", None, None);

    let out = match tauri::async_runtime::spawn_blocking(move || {
        blocking_capture(terminal, timing, select_all_fallback)
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
        "capture: terminal={} armed={} via_select_all={} text_len={:?} saved_len={:?}",
        terminal,
        captured.armed,
        captured.via_select_all,
        captured.text.as_ref().map(|t| t.chars().count()),
        saved.as_ref().map(|s| s.chars().count()),
    );

    if !captured.armed {
        // Nao foi possivel armar o sentinela: o clipboard estava ocupado por outra app. A
        // seleccao do utilizador ficou intacta. Diz a verdade em vez de "Select text first".
        finish(&app, FlowOutcome::ClipboardBusy).await;
        return;
    }

    let via_select_all = captured.via_select_all;

    let Some(selected) = captured.text else {
        // Nada selecionado: restaura clipboard, hint subtil.
        let s = saved.clone();
        let _ = tauri::async_runtime::spawn_blocking(move || blocking_restore(s, image, terminal))
            .await;
        finish(&app, FlowOutcome::NoSelectionFound).await;
        return;
    };

    // Nada que se refine: acaba o ciclo sem CHAMAR O MODELO, que e o que custa dinheiro e os
    // ~4 segundos. O orb ja apareceu e a captura ja foi feita, e assim tem de ser: so depois de
    // ter o texto em maos e possivel decidir se ha alguma coisa para melhorar nele.
    if !ember_core::is_worth_refining(&selected, mode) {
        log::info!(
            "preflight: seleccao sem nada a refinar ({} chars); sem chamada ao modelo",
            selected.chars().count()
        );
        let s = saved.clone();
        let _ = tauri::async_runtime::spawn_blocking(move || blocking_restore(s, image, terminal))
            .await;
        finish(&app, FlowOutcome::NothingToRefine).await;
        return;
    }

    // Guarda do fallback: o texto veio de um Ctrl+A nosso, nao de uma escolha do utilizador. Se
    // o foco nao estava num campo editavel, esse Ctrl+A seleciona o DOCUMENTO todo e o que temos
    // em maos e uma pagina inteira. Colar por cima disso destruia-a; abortamos e dizemos porque.
    if via_select_all && !seq::plausible_field_capture(&selected, select_all_max_chars) {
        log::info!(
            "capture: select-all rejeitado (len={} > teto={})",
            selected.chars().count(),
            select_all_max_chars
        );
        let s = saved.clone();
        let _ = tauri::async_runtime::spawn_blocking(move || blocking_restore(s, image, terminal))
            .await;
        finish(&app, FlowOutcome::SelectAllTooBig).await;
        return;
    }

    // Uma captura por select-all passa SEMPRE pelo gate, mesmo com o preview global desligado: o
    // utilizador nunca escolheu este texto, por isso tem de o ver antes de ser substituido.
    let preview = preview || via_select_all;

    if cancelled(&app) {
        abort_cancelled(&app, saved, image, terminal).await;
        return;
    }

    // Esc cancela a espera pelo modelo, que e a unica parte deste ciclo com duracao a serio.
    //
    // Nasce AQUI e nao no inicio do ciclo, e isso e uma correcao: com o watcher a viver desde o
    // arranque, cada saida precoce (captura falhada, nada selecionado, clipboard ocupado) ainda
    // corria o `finish` com o hook instalado, e durante esse segundo e meio de pilula o Esc do
    // utilizador era consumido por um refine que ja tinha acabado. A captura demora ~300ms e nao
    // e o que alguem quer cancelar; a chamada ao modelo pode demorar dezenas de segundos.
    let esc_watch = crate::preview_hook::spawn_esc_watcher(app.clone());

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
        mode,
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

    // O refine terminou (bem, mal ou cancelado): o watcher ja nao tem nada a vigiar, e TEM de
    // cair antes de o gate do preview instalar o hook dele. O join e curto (o pump acorda a cada
    // 50ms) e garante a ordem hook-a-hook.
    esc_watch.stop_and_join();

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
                            log::info!(
                                "paste: starting (terminal={} preview={} len={} has_newline={})",
                                terminal,
                                preview,
                                refined.chars().count(),
                                refined.contains('\n')
                            );
                            let pasted = tauri::async_runtime::spawn_blocking(move || {
                                blocking_replace(refined, s, image, terminal, settle_ms)
                            })
                            .await;
                            log::info!("paste: done (armed={pasted:?})");
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
                    // A traducao tem mensagem propria: a accao util e ir ver o perfil, e nao
                    // "tenta outra vez", que e o que um erro generico sugere.
                    let outcome = match reason {
                        ember_core::DegradeReason::LanguageFlipped => FlowOutcome::RefineTranslated,
                        _ => FlowOutcome::RefineUnclean,
                    };
                    finish(&app, outcome).await;
                }
            }
        }
        Err(e) => {
            // Sem isto, um "provider error" na overlay nao deixava rasto NENHUM no ficheiro de
            // log: o utilizador via a mensagem amigavel e nos ficavamos sem a causa (que
            // provider, que codigo HTTP, que corpo). Um erro que o utilizador ve tem de ser
            // sempre diagnosticavel a posteriori.
            log::error!("refine failed: {e:?}");
            let s = saved.clone();
            let _ =
                tauri::async_runtime::spawn_blocking(move || blocking_restore(s, image, terminal))
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
