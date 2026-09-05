//! Loop nativo: hotkey -> orb no cursor -> capturar seleccao -> refinar -> substituir.

use std::sync::atomic::Ordering;

use tauri::{AppHandle, Emitter, Manager};

use crate::selection::{ClipImage, RealIo, SENTINEL};
use crate::state::AppState;
use crate::{commands, hide_orb, show_settings};
use ember_core::cycle::RunPhase;
use ember_core::model::{Provider, RefineMode};
use ember_core::overlay::{feedback_for, FlowOutcome};
use ember_core::selection as seq;

const STATE_EVENT: &str = "ember://state";

/// Teto para esperar pela chamada de outro ciclo antes de fazer a sua.
const JOIN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);
/// Ritmo a que se reconfirma o pedido de dispensar, para nao depender so do aviso (que se pode
/// perder entre voltas de um `select!`).
const DISMISS_POLL: std::time::Duration = std::time::Duration::from_millis(250);
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
    /// Numero deste ciclo. Prefixa o log e identifica quem foi dispensado: os ciclos sobrepoem-se
    /// (um a mostrar a pilula, outro ja a capturar) e um booleano global cancelava o errado.
    pub run_id: u64,
    /// A janela (HWND, pid) que tinha o foco quando o atalho disparou. E onde o texto TEM de ser
    /// colado; se entretanto o foco mudou de aplicacao, nao se cola as cegas.
    pub target_hwnd: Option<crate::foreground::TargetSnapshot>,
}

/// Native jobs retain the lease if their async caller is aborted while they are still running.
#[derive(Clone)]
pub struct RunLease(std::sync::Arc<RunOwnership>);

struct RunOwnership {
    app: AppHandle,
    id: u64,
}

impl RunLease {
    pub fn new(app: AppHandle, id: u64) -> Self {
        Self(std::sync::Arc::new(RunOwnership { app, id }))
    }

    fn may_apply(&self) -> bool {
        self.0.app.state::<AppState>().may_apply(self.0.id)
    }
}

impl Drop for RunOwnership {
    fn drop(&mut self) {
        self.app.state::<AppState>().complete_run(self.id);
    }
}

struct ApplyTarget {
    window: Option<crate::foreground::TargetSnapshot>,
    selection: Option<crate::selection_guard::SelectionGuard>,
    lease: RunLease,
}

impl ApplyTarget {
    fn matches(&self) -> bool {
        self.lease.may_apply()
            && crate::foreground::same_target(self.window)
            && self.selection.as_ref().is_some_and(|guard| guard.matches())
            && crate::foreground::same_target(self.window)
            && self.lease.may_apply()
    }
}

fn emit(
    app: &AppHandle,
    run_id: u64,
    phase: &str,
    message: Option<String>,
    provider: Option<String>,
) {
    emit_payload(app, run_id, phase, message, provider, None);
}

fn emit_payload(
    app: &AppHandle,
    run_id: u64,
    phase: &str,
    message: Option<String>,
    provider: Option<String>,
    preview: Option<serde_json::Value>,
) {
    let state = app.state::<AppState>();
    if state.hide_gen.load(Ordering::SeqCst) != run_id {
        return;
    }
    state
        .orb_visible
        .store(phase == "refining", Ordering::SeqCst);
    // Ha texto a direita da brasa? So entao vale a pena reservar-lhe espaco ao clampar.
    let has_labels = message.is_some()
        || state
            .orb_project
            .lock()
            .ok()
            .map(|p| p.is_some())
            .unwrap_or(false);
    state.orb_labels.store(has_labels, Ordering::SeqCst);
    // Tudo o que esta visivel segue o cursor (a regra vive em `ember_core::overlay::follows_cursor`).
    // Antes so o orb e o preview seguiam e as pilulas de resultado ficavam onde tinham nascido:
    // quem mexia o rato durante o refine ia buscar a resposta ao sitio onde tinha comecado, e o
    // efeito lido era "as pilulas nao seguem o rato".
    state
        .follow_cursor
        .store(ember_core::overlay::follows_cursor(phase), Ordering::SeqCst);
    // A cor do projeto ativo viaja com o estado: e o unico sinal que diz, em cada refine, com que
    // projeto ele esta a ser feito. Sem isto, um projeto ativo e invisivel e da para refinar uma
    // semana com o contexto errado sem dar por nada.
    let accent = state.orb_accent.lock().ok().and_then(|a| a.clone());
    let project = state.orb_project.lock().ok().and_then(|a| a.clone());
    let payload = serde_json::json!({
        "runId": run_id, "sequence": state.event_seq.fetch_add(1, Ordering::SeqCst) + 1,
        "preview": preview, "phase": phase, "message": message, "provider": provider,
        "accent": accent, "project": project
    });
    if let Ok(mut slot) = state.last_state.lock() {
        *slot = Some(payload.clone());
    }
    let _ = app.emit_to("overlay", STATE_EVENT, payload);
}

/// Resultado da captura: a seleccao sequenciada, um snapshot de imagem a repor (quando o
/// clipboard original era uma imagem) e `unpreservable` = o clipboard tem conteudo que nao
/// sabemos preservar (ficheiros/RTF), caso em que nada foi tocado e o fluxo aborta.
struct CaptureOutput {
    captured: seq::Captured,
    image: Option<ClipImage>,
    unpreservable: bool,
    selection_guard: Option<crate::selection_guard::SelectionGuard>,
}

#[derive(Debug)]
enum CaptureFailure {
    Native,
    Unverifiable,
}

/// Bloqueante: cria RealIo, captura a seleccao preservando um clipboard de imagem.
fn blocking_capture(
    terminal: bool,
    timing: CaptureTiming,
    select_all_fallback: bool,
    target: Option<crate::foreground::TargetSnapshot>,
) -> Result<CaptureOutput, CaptureFailure> {
    let _input_owner = crate::preview_hook::input_lease();
    if !crate::foreground::same_target(target) {
        return Err(CaptureFailure::Unverifiable);
    }
    // Resolve accessibility before copying or selecting all, including password/editability checks.
    let selection_guard = if terminal {
        None
    } else {
        Some(
            crate::selection_guard::SelectionGuard::begin(target)
                .ok_or(CaptureFailure::Unverifiable)?,
        )
    };
    let mut io = RealIo::new(terminal).map_err(|_| CaptureFailure::Native)?;
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
            selection_guard: None,
        });
    }
    // Snapshot da imagem ANTES de a captura escrever o sentinela (senao perdia-se).
    let image = io.snapshot_image();
    #[cfg(windows)]
    if image.is_none() {
        return Err(CaptureFailure::Native);
    }
    let mut captured = seq::capture(
        &mut io,
        SENTINEL,
        timing.polls,
        timing.step_ms,
        NEUTRALIZE_TIMEOUT_MS,
        terminal,
        select_all_fallback,
    );
    // End clipboard ownership before network I/O. Later cancellation must not restore stale data.
    let owned = captured.text.as_deref().unwrap_or(SENTINEL);
    restore_snapshot(&mut io, &captured.saved, image.as_ref(), owned);
    captured.saved = None;
    if !crate::foreground::same_target(target)
        || selection_guard.as_ref().is_some_and(|guard| {
            captured
                .text
                .as_deref()
                .is_some_and(|text| !guard.seal(text, captured.via_select_all))
        })
    {
        return Err(CaptureFailure::Unverifiable);
    }
    Ok(CaptureOutput {
        captured,
        image: None,
        unpreservable: false,
        selection_guard,
    })
}

/// Bloqueante: substitui a seleccao pelo refinado e restaura o clipboard original. Se o
/// original era uma imagem (sem texto guardado), repoe a imagem por cima do refinado depois
/// do paste. Devolve `true` se o refinado chegou mesmo ao clipboard (ver `seq::replace`).
fn restore_snapshot(
    io: &mut RealIo,
    saved: &Option<String>,
    image: Option<&ClipImage>,
    owned: &str,
) {
    use seq::SelectionIo;
    let revision = io.clip_revision();
    if io.clip_get().as_deref() != Some(owned) {
        return;
    }
    if let Some(image) = image {
        #[cfg(windows)]
        {
            if let Some(revision) = revision {
                let _ = image.restore_if_owned(revision);
            }
            return;
        }
        #[cfg(not(windows))]
        if saved.is_none() {
            if io.clip_revision() == revision {
                io.restore_image(image);
            }
            return;
        }
    }
    seq::restore_owned(io, saved, owned, revision);
}

fn blocking_replace(
    refined: String,
    terminal: bool,
    settle_ms: u64,
    target: ApplyTarget,
    expected_selection: String,
) -> Result<bool, String> {
    use seq::SelectionIo;
    let _input_owner = crate::preview_hook::input_lease();
    // Terminal line editing is shell-specific; a generic Ctrl+U can destroy unrelated input.
    if terminal || !target.matches() {
        return Ok(false);
    }
    let mut io = RealIo::new(false)?;
    if io.has_unpreservable_content() {
        return Ok(false);
    }
    let saved = io.clip_get();
    let image = io.snapshot_image();
    #[cfg(windows)]
    if image.is_none() {
        return Ok(false);
    }
    let captured = seq::capture(
        &mut io,
        SENTINEL,
        10,
        10,
        NEUTRALIZE_TIMEOUT_MS,
        false,
        false,
    );
    let owned = captured.text.as_deref().unwrap_or(SENTINEL);
    if captured.text.as_deref() != Some(expected_selection.as_str()) || !target.matches() {
        restore_snapshot(&mut io, &saved, image.as_ref(), owned);
        return Ok(false);
    }
    io.clip_set(&refined);
    let revision = io.clip_revision();
    if io.clip_get().as_deref() != Some(refined.as_str()) || !target.matches() {
        restore_snapshot(&mut io, &saved, image.as_ref(), &refined);
        return Ok(false);
    }
    io.send_paste();
    io.sleep_ms(settle_ms);
    if io.clip_revision() == revision {
        restore_snapshot(&mut io, &saved, image.as_ref(), &refined);
    }
    Ok(io.input_succeeded())
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

/// `true` se o utilizador dispensou ESTE ciclo (Esc ou segunda tecla).
fn dismissed(app: &AppHandle, run_id: u64) -> bool {
    app.state::<AppState>().dismissed(run_id)
}

/// Relogio em ms para as entradas da cache.
fn now_ms() -> u64 {
    crate::refine_store::now_ms()
}

/// Emite o feedback e agenda o esconder a partir de um resultado terminal do fluxo. Um so
/// sitio a decidir "o que mostrar e por quanto tempo" (`ember_core::overlay::feedback_for`),
/// em vez de cada chamador embutir a sua propria string e o seu proprio numero magico.
async fn finish(app: &AppHandle, run_id: u64, outcome: FlowOutcome) {
    let fb = feedback_for(outcome);
    emit(app, run_id, fb.phase, fb.message, fb.provider);
    // Feedback may outlive its run. Only this run's ownership is released, so late cleanup
    // cannot admit a third interaction while a newer run is capturing or applying text.
    app.state::<AppState>().complete_run(run_id);
    hide_after(app, run_id, fb.hide_after_ms).await;
}

/// Restaura o clipboard (texto ou imagem) e mostra "Cancelled" brevemente. Usado nos ramos
/// de cancelamento, para a seleccao do utilizador ficar sempre intacta.
async fn abort_cancelled(
    app: &AppHandle,
    run_id: u64,
    saved: Option<String>,
    image: Option<ClipImage>,
    terminal: bool,
    outcome: FlowOutcome,
) {
    let _ = tauri::async_runtime::spawn_blocking(move || blocking_restore(saved, image, terminal))
        .await;
    finish(app, run_id, outcome).await;
}

/// Orquestra todo o fluxo: hotkey -> orb -> capturar -> refinar -> colar. Ver `RunOpts`.
pub async fn run(app: AppHandle, opts: RunOpts, lease: RunLease) {
    let RunOpts {
        terminal,
        timing,
        project_title,
        preview,
        select_all_fallback,
        select_all_max_chars,
        mode,
        run_id,
        target_hwnd,
    } = opts;
    emit(&app, run_id, "refining", None, None);

    let capture_lease = lease.clone();
    let out = match tauri::async_runtime::spawn_blocking(move || {
        let _capture_lease = capture_lease;
        blocking_capture(terminal, timing, select_all_fallback, target_hwnd)
    })
    .await
    {
        Ok(Ok(o)) => o,
        Ok(Err(CaptureFailure::Unverifiable)) => {
            finish(&app, run_id, FlowOutcome::TargetUnverifiable).await;
            return;
        }
        _ => {
            finish(&app, run_id, FlowOutcome::CaptureFailed).await;
            return;
        }
    };

    if out.unpreservable {
        // O clipboard tem conteudo que nao sabemos repor (ficheiros, etc.). Nao lhe tocamos.
        finish(&app, run_id, FlowOutcome::UnpreservableClipboard).await;
        return;
    }

    let selection_guard = out.selection_guard;
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
        finish(&app, run_id, FlowOutcome::ClipboardBusy).await;
        return;
    }

    let via_select_all = captured.via_select_all;

    let Some(selected) = captured.text else {
        // Nada selecionado: restaura clipboard, hint subtil.
        let s = saved.clone();
        let _ = tauri::async_runtime::spawn_blocking(move || blocking_restore(s, image, terminal))
            .await;
        finish(&app, run_id, FlowOutcome::NoSelectionFound).await;
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
        finish(&app, run_id, FlowOutcome::NothingToRefine).await;
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
        finish(&app, run_id, FlowOutcome::SelectAllTooBig).await;
        return;
    }

    // Uma captura por select-all passa SEMPRE pelo gate, mesmo com o preview global desligado: o
    // utilizador nunca escolheu este texto, por isso tem de o ver antes de ser substituido.
    let preview = preview || via_select_all;

    if dismissed(&app, run_id) {
        abort_cancelled(&app, run_id, saved, image, terminal, FlowOutcome::Cancelled).await;
        return;
    }

    // Esc tira a espera da frente do utilizador, que e a unica parte deste ciclo com duracao a
    // serio.
    //
    // Nasce AQUI e nao no inicio do ciclo, e isso e uma correcao: com o watcher a viver desde o
    // arranque, cada saida precoce (captura falhada, nada selecionado, clipboard ocupado) ainda
    // corria o `finish` com o hook instalado, e durante esse segundo e meio de pilula o Esc do
    // utilizador era consumido por um refine que ja tinha acabado. A captura demora ~300ms e nao
    // e o que alguem quer dispensar; a chamada ao modelo pode demorar dezenas de segundos.
    if !app
        .state::<AppState>()
        .advance_run(run_id, RunPhase::Requesting)
    {
        finish(&app, run_id, FlowOutcome::Cancelled).await;
        return;
    }
    let esc_watch = crate::preview_hook::spawn_esc_watcher(app.clone(), run_id);

    let obtained = obtain_refined(
        &app,
        run_id,
        &selected,
        project_title.as_deref(),
        mode,
        preview,
    )
    .await;

    // A chamada saiu da frente (respondida, falhada ou dispensada): o watcher ja nao tem nada a
    // vigiar, e TEM de cair antes de o gate do preview instalar o hook dele. O join e curto (o
    // pump acorda a cada 50ms) e garante a ordem hook-a-hook.
    esc_watch.stop_and_join();

    let (engine, provider, from_cache) = match obtained {
        Obtained::Ready {
            engine,
            provider,
            from_cache,
        } => (engine, provider, from_cache),
        Obtained::Dismissed => {
            // O dinheiro ja gasto NAO se perde: a chamada segue numa tarefa propria e guarda o
            // refinado. A pilula diz isso, e o atalho seguinte sobre o mesmo texto reaproveita.
            log::info!("[run {run_id}] dispensado; a chamada segue e o resultado fica guardado");
            abort_cancelled(&app, run_id, saved, image, terminal, FlowOutcome::Dismissed).await;
            return;
        }
        Obtained::Failed(e) => {
            // Sem isto, um "provider error" na overlay nao deixava rasto NENHUM no ficheiro de
            // log: o utilizador via a mensagem amigavel e nos ficavamos sem a causa (que
            // provider, que codigo HTTP, que corpo). Um erro que o utilizador ve tem de ser
            // sempre diagnosticavel a posteriori.
            log::error!("[run {run_id}] refine failed: {e:?}");
            let s = saved.clone();
            let _ =
                tauri::async_runtime::spawn_blocking(move || blocking_restore(s, image, terminal))
                    .await;
            let message = commands::friendly_error(&e);
            if matches!(e, ember_core::CoreError::NoProvidersConfigured) {
                show_settings(&app);
            }
            finish(&app, run_id, FlowOutcome::RefineFailed { message }).await;
            return;
        }
    };

    match engine {
        ember_core::EngineResult::Paste(refined) => {
            // Gate de preview (opt-in): mostra um pill de aprovacao e espera Enter/Esc.
            // Fora do preview, `Accept` direto (comportamento de sempre). Ramifica-se ANTES
            // de mover `image` para o `blocking_replace`, porque o reject precisa dele.
            let decision = if preview {
                if !app
                    .state::<AppState>()
                    .advance_run(run_id, RunPhase::Reviewing)
                {
                    finish(&app, run_id, FlowOutcome::Cancelled).await;
                    return;
                }
                // Um reaproveitamento PARECIDO (nao identico) diz-se: os caracteres que diferem
                // sao precisamente a edicao que a pessoa acabou de fazer, e aplicar por cima sem
                // avisar revertia-a em silencio.
                emit_preview(&app, run_id, &selected, &refined);
                crate::preview_hook::gate(app.clone(), run_id).await
            } else {
                crate::preview_hook::Decision::Accept
            };

            match decision {
                crate::preview_hook::Decision::Accept => {
                    if !app
                        .state::<AppState>()
                        .advance_run(run_id, RunPhase::Applying)
                    {
                        finish(&app, run_id, FlowOutcome::Cancelled).await;
                        return;
                    }
                    // A janela em foco tem de ser a mesma da captura. Entre o hotkey e aqui pode
                    // ter passado uma chamada de dezenas de segundos mais dez de preview: colar
                    // as cegas metia o texto de uma app dentro de outra. O resultado ja esta
                    // guardado, por isso nao colar nao custa nada a ninguem.
                    if !crate::foreground::same_target(target_hwnd) {
                        log::warn!(
                            "[run {run_id}] paste cancelado: a janela em foco mudou desde a captura"
                        );
                        let s = saved.clone();
                        let _ = tauri::async_runtime::spawn_blocking(move || {
                            blocking_restore(s, image, terminal)
                        })
                        .await;
                        finish(&app, run_id, FlowOutcome::ForegroundChanged).await;
                        return;
                    }
                    let expected_selection = selected.clone();
                    let settle_ms = timing.settle_ms;
                    log::info!(
                        "[run {run_id}] paste: starting (terminal={} preview={} len={} reuse={:?})",
                        terminal,
                        preview,
                        refined.chars().count(),
                        from_cache
                    );
                    let target = ApplyTarget {
                        window: target_hwnd,
                        selection: selection_guard,
                        lease: lease.clone(),
                    };
                    let pasted = tauri::async_runtime::spawn_blocking(move || {
                        blocking_replace(refined, terminal, settle_ms, target, expected_selection)
                    })
                    .await;
                    log::info!("[run {run_id}] paste: done (armed={pasted:?})");
                    match pasted {
                        Ok(Ok(true)) => {
                            let outcome = if matches!(from_cache, Reuse::Fresh) {
                                FlowOutcome::Success { provider }
                            } else {
                                FlowOutcome::ReusedFromCache
                            };
                            finish(&app, run_id, outcome).await;
                        }
                        _ => {
                            // O refinado nao chegou a ser armado no clipboard (ocupado). A
                            // seleccao ficou intacta: nao reportar "Refined" falso. O refinado
                            // esta guardado, portanto o atalho seguinte nao volta a pagar.
                            finish(&app, run_id, FlowOutcome::PasteFailed).await;
                        }
                    }
                }
                crate::preview_hook::Decision::Reject => {
                    // Restaura o clipboard e mantem o original. O refinado fica guardado: mudar
                    // de ideias a seguir custa zero.
                    let s = saved.clone();
                    let _ = tauri::async_runtime::spawn_blocking(move || {
                        blocking_restore(s, image, terminal)
                    })
                    .await;
                    finish(&app, run_id, FlowOutcome::PreviewRejected).await;
                }
            }
        }
        ember_core::EngineResult::Degrade(reason) => {
            log::warn!(
                "[run {run_id}] engine degraded ({reason:?}); clipboard restored, nothing pasted"
            );
            let s = saved.clone();
            let _ =
                tauri::async_runtime::spawn_blocking(move || blocking_restore(s, image, terminal))
                    .await;
            // A traducao tem mensagem propria: a accao util e ir ver o perfil, e nao
            // "tenta outra vez", que e o que um erro generico sugere.
            let outcome = match reason {
                ember_core::DegradeReason::LanguageFlipped => FlowOutcome::RefineTranslated,
                _ => FlowOutcome::RefineUnclean,
            };
            finish(&app, run_id, outcome).await;
        }
    }
}

/// De onde veio o refinado que se vai colar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reuse {
    /// Chamada nova ao modelo, paga agora.
    Fresh,
    /// Mesmo texto ja refinado antes: zero custo.
    Exact,
}

/// O que a fase de refinamento produziu.
enum Obtained {
    Ready {
        engine: ember_core::EngineResult,
        provider: String,
        from_cache: Reuse,
    },
    /// O utilizador dispensou a espera. A chamada segue e guarda o resultado.
    Dismissed,
    Failed(ember_core::CoreError),
}

/// O que a tarefa de refinamento devolve a quem estiver a espera (se ainda estiver alguem).
enum RefineDone {
    Ready {
        engine: ember_core::EngineResult,
        provider: String,
    },
    Failed(ember_core::CoreError),
}

/// Arranja o refinado da seleccao, pela ordem mais barata: cache, chamada em curso, chamada nova.
///
/// As tres hipoteses existem por causa da mesma coisa: **nao pagar duas vezes o mesmo texto**.
/// O log das 17:08 de 2026-09-02 mostrou o caso a serio: a mesma seleccao de 385 caracteres
/// capturada tres vezes em sete segundos, com o utilizador a carregar no atalho porque nada
/// parecia estar a acontecer. Hoje isso e uma chamada so, e o segundo e o terceiro atalho
/// juntam-se a ela.
async fn obtain_refined(
    app: &AppHandle,
    run_id: u64,
    selected: &str,
    project_title: Option<&str>,
    mode: RefineMode,
    preview: bool,
) -> Obtained {
    let state = app.state::<AppState>();
    let prep =
        match commands::prepare_refine(app, state.inner(), selected, project_title, mode).await {
            Ok(p) => p,
            Err(e) => return Obtained::Failed(e),
        };
    let key = prep.key.clone();

    // Subscrever ANTES de consultar: se a tarefa em curso guardar o resultado entre a consulta e
    // a espera, o `changed()` dispara na mesma. Ao contrario, perdia-se o sinal e ficava-se a
    // espera de algo que ja tinha acontecido.
    let mut gen = state.store_gen.subscribe();

    if let Some((entry, reuse)) = lookup_cache(app, &key, preview) {
        log::info!(
            "[run {run_id}] cache {reuse:?}: {} chars de {} ({}), sem chamada ao modelo",
            entry.refined.chars().count(),
            entry.provider,
            entry.model
        );
        return ready_from(entry, reuse);
    }

    // Ha uma chamada IGUAL a decorrer (o ciclo anterior, que o utilizador dispensou ou que ainda
    // espera): junta-te a ela em vez de fazer a mesma pergunta ao modelo outra vez.
    let mut joined_log = false;
    let joined_at = std::time::Instant::now();
    while let Some(f) = state.inflight_with(&key) {
        if !joined_log {
            log::info!("[run {run_id}] a juntar-se a chamada do ciclo {}", f.run_id);
            joined_log = true;
        }
        // Teto de espera. A chamada a que nos juntamos ja tem os seus proprios limites (stall de
        // 60s por tentativa, retry, fallback), mas esperar por ela NAO pode ser eterno: se algo
        // do outro lado ficar preso, mais vale pagar uma chamada do que deixar o utilizador com
        // uma brasa acesa para sempre.
        if joined_at.elapsed() > JOIN_TIMEOUT {
            log::warn!("[run {run_id}] existing request is still pending; no duplicate submitted");
            return Obtained::Failed(ember_core::CoreError::AllProvidersFailed);
        }
        tokio::select! {
            _ = gen.changed() => {}
            _ = state.cancel_notify.notified() => {}
            // O `notified()` e recriado a cada volta, portanto um aviso que chegue ENTRE voltas
            // perde-se. Esta batida fecha essa janela: o dispensar e reconfirmado pelo estado,
            // que nao se perde, em vez de depender so do aviso.
            _ = tokio::time::sleep(DISMISS_POLL) => {}
        }
        if state.dismissed(run_id) {
            return Obtained::Dismissed;
        }
        if let Some((entry, reuse)) = lookup_cache(app, &key, preview) {
            log::info!("[run {run_id}] resultado da chamada a que se juntou: reaproveitado");
            return ready_from(entry, reuse);
        }
    }

    if joined_log {
        // An earlier request finished without a recoverable result. Its billing outcome is
        // unknown, so joining it must not silently become a second paid request.
        return Obtained::Failed(ember_core::CoreError::Uncertain);
    }
    // No result and no prior operation: this is a new explicit request.
    if state
        .inflight
        .lock()
        .map(|requests| requests.len() >= 4)
        .unwrap_or(true)
    {
        return Obtained::Failed(ember_core::CoreError::AllProvidersFailed);
    }
    let rx = spawn_refine(app.clone(), run_id, prep);
    tokio::pin!(rx);
    let done = loop {
        tokio::select! {
            r = &mut rx => break r.ok(),
            _ = state.cancel_notify.notified() => {}
            // Ver a nota na juncao: a batida cobre o aviso que chegue entre voltas do `select!`.
            _ = tokio::time::sleep(DISMISS_POLL) => {}
        }
        if state.dismissed(run_id) {
            // Larga-se o RECETOR, nao a tarefa: ela segue ate ao fim e guarda o refinado. Antes
            // largava-se o future da chamada HTTP, o provider cobrava na mesma e o resultado ia
            // para o lixo.
            break None;
        }
    };
    match done {
        Some(RefineDone::Ready { engine, provider }) => Obtained::Ready {
            engine,
            provider,
            from_cache: Reuse::Fresh,
        },
        Some(RefineDone::Failed(e)) => Obtained::Failed(e),
        None => Obtained::Dismissed,
    }
}

/// Consulta a cache. O acerto PARECIDO so se procura com o preview ligado, onde ha quem veja e
/// aprove antes de colar.
fn lookup_cache(
    app: &AppHandle,
    key: &ember_core::CacheKey,
    _preview: bool,
) -> Option<(ember_core::CacheEntry, Reuse)> {
    let state = app.state::<AppState>();
    let hit = state
        .store
        .lock()
        .ok()?
        .lookup(key, now_ms())
        .map(|e| (e, Reuse::Exact));
    hit
}

fn ready_from(entry: ember_core::CacheEntry, reuse: Reuse) -> Obtained {
    Obtained::Ready {
        engine: ember_core::EngineResult::Paste(entry.refined),
        provider: entry.provider,
        from_cache: reuse,
    }
}

/// Lanca a chamada ao modelo numa tarefa PROPRIA, dona do resultado.
///
/// A tarefa faz a chamada, o pos-processamento e a gravacao na cache ANTES de responder a quem
/// esta a espera. Por isso e que dispensar a espera deixou de custar dinheiro: quem espera pode
/// desistir (Esc, segunda tecla, fechar a pilula, fechar a app) que a tarefa segue e guarda.
fn spawn_refine(
    app: AppHandle,
    run_id: u64,
    prep: commands::PreparedRefine,
) -> tokio::sync::oneshot::Receiver<RefineDone> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    // O registo e SINCRONO, antes de a tarefa arrancar: se ficasse la dentro, um ciclo novo
    // podia consultar o registo antes de a tarefa lhe chegar e pagar a mesma chamada.
    app.state::<AppState>()
        .inflight_add(crate::state::InFlight {
            key: prep.key.clone(),
            run_id,
        });
    tauri::async_runtime::spawn(async move {
        // A saida do registo corre no Drop e nao no fim do corpo, e isso e a diferenca entre um
        // erro e um bloqueio: se a tarefa morresse a meio (um panico), o registo ficava la para
        // sempre e todos os ciclos seguintes com aquele texto esperavam por um sinal que nunca
        // vinha. Com a guarda, o desenrolar da pilha limpa e acorda quem esperava.
        let _guard = InFlightGuard {
            app: app.clone(),
            run_id,
        };
        let started = std::time::Instant::now();
        // Feedback de progresso honesto: torna visivel o retry e o fallback (nao a cauda do
        // texto a ser gerado, que sao tokens internos e nao o que sera colado).
        let app_cb = app.clone();
        let on_attempt = move |provider: Provider, idx: usize, attempt: u32| {
            // Se o ciclo ja foi dispensado (ou ja ha outro no ecra), calar: emitir "refining"
            // aqui ressuscitava a orb depois de o utilizador a ter mandado embora.
            let st = app_cb.state::<AppState>();
            if st.dismissed(run_id) || st.hide_gen.load(Ordering::SeqCst) != run_id {
                return;
            }
            let msg = if idx == 0 && attempt == 0 {
                None // primeira tentativa do provider primario: o "refining" ja esta a mostra
            } else if attempt > 0 {
                Some(format!("Retrying {}...", provider.display_name()))
            } else {
                Some(format!("Trying {}...", provider.display_name()))
            };
            if let Some(m) = msg {
                emit(&app_cb, run_id, "refining", Some(m), None);
            }
        };

        let state = app.state::<AppState>();
        let result = commands::execute_refine(&app, state.inner(), &prep, &on_attempt).await;
        let ms = started.elapsed().as_millis();

        let done = match result {
            Ok((raw, provider, model)) => {
                // Motor Ember, fase 2: limpa/desmascara/valida o texto CRU do modelo. Um Degrade
                // (output vazio, ou um span de codigo/URL perdido) NAO se guarda: nao ha nada de
                // util para reaproveitar, e guardar impedia uma segunda tentativa de correr.
                let engine = ember_core::postprocess(&raw, &prep.prepared);
                if let ember_core::EngineResult::Paste(refined) = &engine {
                    let now = now_ms();
                    let entry = ember_core::CacheEntry {
                        refined: refined.clone(),
                        provider: provider.clone(),
                        model: model.clone(),
                        ts_ms: now,
                    };
                    state.remember(prep.key.clone(), entry.clone(), now);
                    if prep.keep_results {
                        crate::refine_store::save(
                            &app,
                            &prep.key,
                            &entry,
                            prep.retention_generation,
                        );
                    }
                    log::info!(
                        "[run {run_id}] guardado: {} chars de {provider} ({model}) em {ms}ms",
                        refined.chars().count()
                    );
                }
                RefineDone::Ready { engine, provider }
            }
            Err(e) => RefineDone::Failed(e),
        };

        // O recetor pode ja nao existir (o utilizador dispensou): o resultado ja esta guardado,
        // portanto perder o envio nao perde nada. A saida do registo fica para o `_guard`, que
        // corre logo a seguir e acorda quem esperava.
        let _ = tx.send(done);
    });
    rx
}

/// Tira a chamada do registo de "a decorrer" e acorda quem esperava por ela, aconteca o que
/// acontecer a tarefa. E uma guarda e nao uma linha no fim do corpo porque o fim do corpo nao
/// corre num panico, e ai quem se juntou a chamada ficava pendurado.
struct InFlightGuard {
    app: AppHandle,
    run_id: u64,
}

impl Drop for InFlightGuard {
    fn drop(&mut self) {
        self.app.state::<AppState>().inflight_done(self.run_id);
    }
}

/// Cola outra vez o ultimo refinado guardado, na janela que estiver em foco. E a saida para tudo
/// o que interrompeu um refine: dispensado, recusado no preview, clipboard ocupado, janela
/// trocada. Sem isto, guardar o resultado nao servia de nada.
pub async fn reapply_last(app: AppHandle) {
    let run_id = match app.state::<AppState>().begin_run() {
        Ok(id) => id,
        Err(_) => return,
    };
    let lease = RunLease::new(app.clone(), run_id);
    let entry = {
        let state = app.state::<AppState>();
        let entry = state.store.lock().ok().and_then(|mut c| {
            c.evict_expired(crate::now_ms());
            c.last().cloned()
        });
        entry
    };
    crate::show_orb_at_cursor(&app);
    let Some(entry) = entry else {
        finish(&app, run_id, FlowOutcome::NothingToReapply).await;
        return;
    };
    let cfg = crate::config::load(&app);
    let terminal = cfg.terminal_handling && crate::foreground::is_terminal_foreground();
    let settle_ms = cfg.paste_settle_ms;
    let refined = entry.refined.clone();
    log::info!(
        "[run {run_id}] reapply: {} chars de {} (terminal={terminal})",
        refined.chars().count(),
        entry.provider
    );
    let target = crate::foreground::foreground_target();
    let capture_lease = lease.clone();
    let capture = tauri::async_runtime::spawn_blocking(move || {
        let _capture_lease = capture_lease;
        blocking_capture(
            terminal,
            CaptureTiming {
                polls: 10,
                step_ms: 10,
                settle_ms,
            },
            false,
            target,
        )
    })
    .await;
    let (selected, selection_guard) = match capture {
        Ok(Ok(output)) => (output.captured.text, output.selection_guard),
        Ok(Err(CaptureFailure::Unverifiable)) => {
            finish(&app, run_id, FlowOutcome::TargetUnverifiable).await;
            return;
        }
        _ => (None, None),
    };
    let Some(selected) = selected else {
        finish(&app, run_id, FlowOutcome::NoSelectionFound).await;
        return;
    };
    if !app
        .state::<AppState>()
        .advance_run(run_id, RunPhase::Reviewing)
    {
        finish(&app, run_id, FlowOutcome::Cancelled).await;
        return;
    }
    emit_preview(&app, run_id, &selected, &refined);
    if crate::preview_hook::gate(app.clone(), run_id).await != crate::preview_hook::Decision::Accept
    {
        finish(&app, run_id, FlowOutcome::Cancelled).await;
        return;
    }
    if !app
        .state::<AppState>()
        .advance_run(run_id, RunPhase::Applying)
    {
        finish(&app, run_id, FlowOutcome::Cancelled).await;
        return;
    }
    let target = ApplyTarget {
        window: target,
        selection: selection_guard,
        lease,
    };
    let pasted = tauri::async_runtime::spawn_blocking(move || {
        blocking_replace(refined, terminal, settle_ms, target, selected)
    })
    .await;
    match pasted {
        Ok(Ok(true)) => finish(&app, run_id, FlowOutcome::ReusedFromCache).await,
        _ => finish(&app, run_id, FlowOutcome::PasteFailed).await,
    }
}

async fn hide_after(app: &AppHandle, run_id: u64, ms: u64) {
    tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
    // Um ciclo novo comecou entretanto: a orb que esta no ecra e dele, nao a nossa pilula.
    // Esconde-la aqui apagava o feedback do ciclo em curso a meio.
    let current = app.state::<AppState>().hide_gen.load(Ordering::SeqCst);
    if !ember_core::may_hide(current, run_id) {
        return;
    }
    hide_orb(app);
    // Repoe o overlay em "hidden" para o DOM esvaziar: sem isto, a pilula do ciclo
    // anterior fica montada e, como o orb partilha `layoutId` com ela, o hotkey seguinte
    // faz o orb MORPHAR da pilula velha (desliza, sem fade) em vez de montar de novo e
    // aparecer com fade no sitio certo.
    emit(app, run_id, "hidden", None, None);
}

fn emit_preview(app: &AppHandle, run_id: u64, original: &str, result: &str) {
    emit_payload(
        app,
        run_id,
        "preview",
        None,
        None,
        Some(serde_json::json!({
            "original": ember_core::preview::pages(original), "result": ember_core::preview::pages(result), "page": 0,
        })),
    );
}

#[cfg(windows)]
pub(crate) fn move_preview_page(app: &AppHandle, run_id: u64, delta: i32) {
    let state = app.state::<AppState>();
    if state.hide_gen.load(Ordering::SeqCst) != run_id {
        return;
    }
    let Ok(mut slot) = state.last_state.lock() else {
        return;
    };
    let Some(payload) = slot.as_mut() else {
        return;
    };
    let Some(preview) = payload.get_mut("preview").filter(|v| v.is_object()) else {
        return;
    };
    let pages = ["original", "result"]
        .iter()
        .filter_map(|key| preview[*key].as_array())
        .map(Vec::len)
        .max()
        .unwrap_or(1)
        .max(1) as i64;
    let page = (preview["page"].as_i64().unwrap_or(0) + delta as i64).clamp(0, pages - 1);
    preview["page"] = page.into();
    payload["sequence"] = (state.event_seq.fetch_add(1, Ordering::SeqCst) + 1).into();
    let _ = app.emit_to("overlay", STATE_EVENT, payload.clone());
}

#[tauri::command]
pub fn overlay_snapshot(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, AppState>,
) -> Result<Option<serde_json::Value>, String> {
    if window.label() != "overlay" {
        return Err("Overlay only".into());
    }
    Ok(state
        .last_state
        .lock()
        .map_err(|_| "Overlay state unavailable")?
        .clone())
}
