//! Feedback pontual do overlay (mensagem + duracao) para cada resultado terminal do fluxo
//! de refinamento. Pura e testavel: antes, cada `emit`/`hide_after` em `flow.rs` embutia a
//! sua propria string e o seu proprio numero magico, alguns duplicados por varios sitios
//! (o atraso de erro "1600" aparecia em tres chamadas diferentes). Aqui fica um so lugar
//! para o QUE mostrar e por QUANTO TEMPO, dado o resultado, testavel sem Tauri.

/// Um resultado terminal do fluxo de refinamento (o que aconteceu ao ciclo hotkey -> paste).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlowOutcome {
    /// A captura em si falhou (spawn_blocking ou `RealIo::new` deu erro).
    CaptureFailed,
    /// The focused field cannot prove its editable element and selection identity.
    TargetUnverifiable,
    /// O clipboard tem conteudo que a app nao sabe preservar (ficheiros, RTF, ...).
    UnpreservableClipboard,
    /// O sentinela nao armou: outra app tinha o clipboard ocupado no momento da captura.
    ClipboardBusy,
    /// Nao havia seleccao (o poll esgotou sem o clipboard mudar).
    NoSelectionFound,
    /// A seleccao nao tem nada que se refine (curta de mais e sem estrutura). NAO houve chamada
    /// ao modelo: e um desfecho de POUPANCA, nao um erro.
    NothingToRefine,
    /// O fallback de select-all trouxe texto a mais para ser um campo: o foco nao estava numa
    /// caixa de texto e o Ctrl+A agarrou o documento todo. Nada foi colado.
    SelectAllTooBig,
    /// Uma segunda tecla cancelou o ciclo em curso.
    Cancelled,
    /// O texto refinado nao chegou a ser armado no clipboard antes do paste.
    PasteFailed,
    /// A replacement shortcut was sent. This does not prove the destination accepted it.
    Success { provider: String },
    /// O refinamento falhou; `message` ja vem amigavel (de `friendly_error`).
    RefineFailed { message: String },
    /// O motor recusou colar (output vazio, ou perdeu/mutou um span de codigo/URL): a
    /// seleccao do utilizador ficou intacta, em vez de colar por cima algo partido.
    RefineUnclean,
    /// O utilizador (ou o timeout) recusou aplicar o refinado no gate de preview. A seleccao
    /// original foi restaurada; nada foi colado.
    PreviewRejected,
    /// O modelo traduziu o texto em vez de o refinar. Caso proprio, e nao mais um
    /// `RefineUnclean`, porque a accao util e diferente e concreta: quase sempre e o perfil a
    /// pedir uma lingua. Um "couldn't refine cleanly" generico nao levava ninguem la.
    RefineTranslated,
    /// O utilizador dispensou a espera (Esc ou segunda tecla) DEPOIS de a chamada ao modelo ter
    /// arrancado. Distingue-se de `Cancelled`, que e o aborto antes de haver chamada: aqui ja ha
    /// dinheiro gasto, a chamada segue ate ao fim em segundo plano e o resultado fica guardado.
    Dismissed,
    /// A janela em foco mudou entre a captura e o paste. Nao se cola as cegas noutra app; o
    /// resultado fica guardado e o atalho seguinte sobre o mesmo texto reutiliza-o sem pagar.
    ForegroundChanged,
    /// O refinado veio da cache: mesma seleccao ja refinada antes, sem nova chamada ao modelo.
    ReusedFromCache,
    /// Another writer took the clipboard between arming the result and the paste (a bridge, a
    /// sync tool, any app). Nothing was sent: Ctrl+V would have inserted that content into the
    /// document. The result is cached, so the retry costs nothing.
    ClipboardChanged,
    /// Terminal: the flattened result was left on the clipboard and no keys were sent. Terminal
    /// line editing is shell-specific; until a terminal has a tested adapter the user pastes it.
    TerminalHandoff,
}

/// A overlay segue o cursor nesta fase?
///
/// Regra unica: tudo o que esta visivel segue. Antes so o orb e o preview seguiam, e as pilulas
/// de resultado ficavam onde tinham nascido; quem mexia o rato durante o refine encontrava a
/// resposta a metros de onde estava a olhar, e o efeito lido era "a pilula nao segue o rato".
pub fn follows_cursor(phase: &str) -> bool {
    phase != "hidden"
}

/// O chao de tempo de qualquer nota, seja qual for a preferencia.
///
/// As duracoes por tipo de mensagem ja foram medidas uma a uma (um erro longo fica mais tempo do
/// que uma confirmacao), e nenhuma preferencia por rapidez pode tornar ilegivel a unica frase que
/// explica porque e que um refine falhou. Preferir depressa encurta o que da para encurtar.
pub const MIN_NOTICE_MS: u64 = 600;

/// A duracao de uma nota, ajustada a preferencia e nunca abaixo do chao.
pub fn notice_ms(base_ms: u64, speed: crate::model::NoticeSpeed) -> u64 {
    use crate::model::NoticeSpeed;
    let scaled = match speed {
        NoticeSpeed::Quick => base_ms * 7 / 10,
        NoticeSpeed::Normal => base_ms,
        NoticeSpeed::Relaxed => base_ms * 3 / 2,
    };
    scaled.max(MIN_NOTICE_MS)
}

/// O que mostrar no overlay e por quanto tempo, dado um `FlowOutcome`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverlayFeedback {
    pub phase: &'static str,
    pub message: Option<String>,
    pub provider: Option<String>,
    pub hide_after_ms: u64,
}

/// Mapeia um resultado terminal para a mensagem/fase/duracao a mostrar. Os atrasos nao sao
/// todos iguais de proposito: uma mensagem mais longa (`UnpreservableClipboard`) fica mais
/// tempo visivel, e um cancelamento (feedback so confirmativo) desaparece mais depressa.
pub fn feedback_for(outcome: FlowOutcome) -> OverlayFeedback {
    match outcome {
        FlowOutcome::TargetUnverifiable => OverlayFeedback {
            phase: "hint",
            message: Some("Field unavailable. Select text in another editor.".into()),
            provider: None,
            hide_after_ms: 3500,
        },
        FlowOutcome::CaptureFailed => OverlayFeedback {
            phase: "error",
            message: Some("Couldn't read the selection.".into()),
            provider: None,
            hide_after_ms: 1400,
        },
        FlowOutcome::UnpreservableClipboard => OverlayFeedback {
            phase: "error",
            message: Some(
                "Clipboard holds files Ember can't preserve. Copy your text first.".into(),
            ),
            provider: None,
            hide_after_ms: 1800,
        },
        FlowOutcome::ClipboardBusy => OverlayFeedback {
            phase: "error",
            message: Some("Clipboard was busy. Try again.".into()),
            provider: None,
            hide_after_ms: 1600,
        },
        FlowOutcome::NoSelectionFound => OverlayFeedback {
            phase: "hint",
            message: Some("Select text first".into()),
            provider: None,
            hide_after_ms: 1400,
        },
        // "hint" e nao "error": nada falhou, e o utilizador que precisa de clicar na caixa ou
        // selecionar o trecho. Fica mais tempo visivel porque a mensagem e mais longa.
        FlowOutcome::NothingToRefine => OverlayFeedback {
            phase: "hint",
            message: Some("Nothing to refine there".into()),
            provider: None,
            // Curto: nao ha nada para ler alem do facto de nao ter havido trabalho.
            hide_after_ms: 1200,
        },
        FlowOutcome::SelectAllTooBig => OverlayFeedback {
            phase: "hint",
            message: Some("Click into a text field, or select the text you want".into()),
            provider: None,
            hide_after_ms: 2200,
        },
        FlowOutcome::Cancelled => OverlayFeedback {
            phase: "hint",
            message: Some("Cancelled".into()),
            provider: None,
            hide_after_ms: 800,
        },
        FlowOutcome::PasteFailed => OverlayFeedback {
            phase: "error",
            message: Some("Couldn't paste the result. Try again.".into()),
            provider: None,
            hide_after_ms: 1600,
        },
        FlowOutcome::Success { provider } => OverlayFeedback {
            phase: "success",
            message: Some("Paste sent. Check your text.".into()),
            provider: Some(provider),
            hide_after_ms: 2000,
        },
        FlowOutcome::RefineFailed { message } => OverlayFeedback {
            phase: "error",
            message: Some(message),
            provider: None,
            hide_after_ms: 1600,
        },
        FlowOutcome::RefineUnclean => OverlayFeedback {
            phase: "error",
            message: Some("Couldn't refine cleanly. Nothing changed.".into()),
            provider: None,
            hide_after_ms: 1600,
        },
        FlowOutcome::RefineTranslated => OverlayFeedback {
            phase: "error",
            message: Some("The model translated it. Kept your original.".into()),
            provider: None,
            hide_after_ms: 2000,
        },
        FlowOutcome::Dismissed => OverlayFeedback {
            phase: "hint",
            // "still running" e nao "saved": no instante em que isto aparece a chamada ainda esta
            // a decorrer e pode falhar. Prometer que ficou guardado seria mentir num terco dos
            // casos, e uma promessa dessas so se percebe que era falsa quando ja custou trabalho.
            message: Some("Dismissed \u{00b7} still running".into()),
            provider: None,
            hide_after_ms: 1400,
        },
        FlowOutcome::ForegroundChanged => OverlayFeedback {
            phase: "hint",
            // "run the shortcut again" and not "reapply from the tray": there is no tray entry
            // any more. The saved result is served by the cache on the next shortcut over the
            // same text, which is the only way back that does not need a second surface.
            message: Some("Window changed \u{00b7} result saved, run the shortcut again".into()),
            provider: None,
            hide_after_ms: 2200,
        },
        FlowOutcome::ReusedFromCache => OverlayFeedback {
            phase: "success",
            message: Some("Cached result sent. Check your text.".into()),
            provider: None,
            hide_after_ms: 1600,
        },
        FlowOutcome::PreviewRejected => OverlayFeedback {
            phase: "hint",
            message: Some("Kept your original".into()),
            provider: None,
            hide_after_ms: 900,
        },
        FlowOutcome::ClipboardChanged => OverlayFeedback {
            phase: "error",
            message: Some("Another app changed the clipboard. Nothing pasted. Try again.".into()),
            provider: None,
            // Long message; the retry is free, so the time to read it is the only cost.
            hide_after_ms: 2200,
        },
        FlowOutcome::TerminalHandoff => OverlayFeedback {
            phase: "hint",
            message: Some("Terminal: the result is on your clipboard. Paste it yourself.".into()),
            provider: None,
            // The longest hint: it says where the result went and what to do next.
            hide_after_ms: 3500,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{notice_ms, MIN_NOTICE_MS};
    use crate::model::NoticeSpeed;

    #[test]
    fn the_notice_speed_stretches_and_shrinks_around_the_measured_duration() {
        // 3500ms is the longest measured notice (the one that tells you the field is unusable).
        let base = 3500;
        assert_eq!(notice_ms(base, NoticeSpeed::Normal), base);
        assert!(notice_ms(base, NoticeSpeed::Quick) < base);
        assert!(notice_ms(base, NoticeSpeed::Relaxed) > base);
    }

    #[test]
    fn no_preference_can_make_a_message_too_short_to_read() {
        // The shortest measured notice is 800ms; seven tenths of it is 560, under the floor.
        assert_eq!(notice_ms(800, NoticeSpeed::Quick), MIN_NOTICE_MS);
        assert_eq!(notice_ms(0, NoticeSpeed::Quick), MIN_NOTICE_MS);
        for outcome_ms in [800, 1200, 1400, 1600, 1800, 2000, 2200, 3500] {
            for speed in [NoticeSpeed::Quick, NoticeSpeed::Normal, NoticeSpeed::Relaxed] {
                assert!(notice_ms(outcome_ms, speed) >= MIN_NOTICE_MS, "{outcome_ms} {speed:?}");
            }
        }
    }

    #[test]
    fn the_orb_size_is_always_a_whole_pixel_multiple() {
        use crate::model::OrbSize;
        // Pixel art blurs on a fractional multiple, so the sizes are three fixed steps and the
        // type makes any other value unrepresentable.
        for size in [OrbSize::Small, OrbSize::Normal, OrbSize::Large] {
            assert!(size.px() >= 2 && size.px() <= 4);
        }
        assert_eq!(OrbSize::default().px(), 3, "the default must stay the size everyone has today");
    }

    #[test]
    fn nothing_to_refine_is_a_hint_and_names_no_provider() {
        // E poupanca, nao erro: hint (neutro), sem provider, e sai do ecra depressa.
        let f = super::feedback_for(super::FlowOutcome::NothingToRefine);
        assert_eq!(f.phase, "hint");
        assert!(f.provider.is_none());
        assert!(f.hide_after_ms <= 1400);
    }

    use super::*;

    #[test]
    fn capture_failed_is_a_short_error() {
        let fb = feedback_for(FlowOutcome::CaptureFailed);
        assert_eq!(fb.phase, "error");
        assert_eq!(fb.hide_after_ms, 1400);
        assert!(fb.message.unwrap().contains("read the selection"));
    }

    #[test]
    fn unpreservable_clipboard_gets_the_longest_delay() {
        // Mensagem mais longa: precisa de mais tempo para ser lida antes de desaparecer.
        let fb = feedback_for(FlowOutcome::UnpreservableClipboard);
        assert_eq!(fb.phase, "error");
        assert_eq!(fb.hide_after_ms, 1800);
    }

    #[test]
    fn clipboard_busy_and_paste_failed_share_the_standard_error_delay_but_not_the_message() {
        let busy = feedback_for(FlowOutcome::ClipboardBusy);
        let paste = feedback_for(FlowOutcome::PasteFailed);
        assert_eq!(busy.hide_after_ms, 1600);
        assert_eq!(paste.hide_after_ms, 1600);
        assert_ne!(busy.message, paste.message);
    }

    #[test]
    fn a_clipboard_taken_over_before_the_paste_is_its_own_error() {
        // "Try again" is honest here: the result is cached, so the retry costs nothing and the
        // foreign writer is usually gone. It must not read like the generic paste failure.
        let fb = feedback_for(FlowOutcome::ClipboardChanged);
        assert_eq!(fb.phase, "error");
        let message = fb.message.as_deref().unwrap();
        assert!(message.contains("changed the clipboard"));
        assert_ne!(fb.message, feedback_for(FlowOutcome::PasteFailed).message);
    }

    #[test]
    fn a_terminal_handoff_is_a_hint_that_says_where_the_result_went() {
        // Nothing failed: the result is on the clipboard and the user pastes it. Long message,
        // so it stays as long as the longest hint.
        let fb = feedback_for(FlowOutcome::TerminalHandoff);
        assert_eq!(fb.phase, "hint");
        assert!(fb.message.as_deref().unwrap().contains("clipboard"));
        assert_eq!(fb.hide_after_ms, 3500);
    }

    #[test]
    fn no_selection_is_a_hint_not_an_error() {
        let fb = feedback_for(FlowOutcome::NoSelectionFound);
        assert_eq!(fb.phase, "hint");
        assert_eq!(fb.message.as_deref(), Some("Select text first"));
    }

    #[test]
    fn cancelled_hides_faster_than_other_hints() {
        let cancelled = feedback_for(FlowOutcome::Cancelled);
        let no_selection = feedback_for(FlowOutcome::NoSelectionFound);
        assert_eq!(cancelled.phase, "hint");
        assert!(cancelled.hide_after_ms < no_selection.hide_after_ms);
    }

    #[test]
    fn sending_input_does_not_claim_verified_replacement() {
        let fb = feedback_for(FlowOutcome::Success {
            provider: "Claude".into(),
        });
        assert_eq!(fb.phase, "success");
        assert_eq!(fb.message.as_deref(), Some("Paste sent. Check your text."));
        assert_eq!(fb.provider.as_deref(), Some("Claude"));
    }

    #[test]
    fn refine_failed_carries_through_the_friendly_message() {
        let fb = feedback_for(FlowOutcome::RefineFailed {
            message: "Invalid API key.".into(),
        });
        assert_eq!(fb.phase, "error");
        assert_eq!(fb.message.as_deref(), Some("Invalid API key."));
    }

    #[test]
    fn refine_unclean_is_an_error_that_changed_nothing() {
        let fb = feedback_for(FlowOutcome::RefineUnclean);
        assert_eq!(fb.phase, "error");
        assert!(fb.message.unwrap().contains("Nothing changed"));
    }

    #[test]
    fn preview_rejected_is_a_fast_hint_that_keeps_the_original() {
        let fb = feedback_for(FlowOutcome::PreviewRejected);
        assert_eq!(fb.phase, "hint");
        assert!(fb.message.unwrap().contains("Kept your original"));
        assert!(fb.hide_after_ms <= 1000);
    }

    #[test]
    fn every_visible_phase_follows_the_cursor_and_hidden_does_not() {
        for phase in ["refining", "preview", "success", "error", "hint"] {
            assert!(follows_cursor(phase), "{phase} devia seguir o cursor");
        }
        assert!(!follows_cursor("hidden"));
    }

    #[test]
    fn dismiss_and_foreground_change_say_the_result_was_kept() {
        // O ponto destes dois desfechos e exatamente esse: dizer que o dinheiro gasto nao se
        // perdeu. Se a mensagem deixar de o dizer, o utilizador volta a carregar no atalho.
        let d = feedback_for(FlowOutcome::Dismissed);
        assert_eq!(d.phase, "hint");
        // Dispensar nao promete que ficou guardado: nesse instante a chamada ainda decorre.
        let msg = d.message.unwrap().to_lowercase();
        assert!(msg.contains("running") && !msg.contains("saved"));
        let f = feedback_for(FlowOutcome::ForegroundChanged);
        assert_eq!(f.phase, "hint");
        let msg = f.message.unwrap().to_lowercase();
        // The way back is the shortcut itself, and the message has to say so: a saved result
        // nobody knows how to reach is money spent for nothing.
        assert!(msg.contains("saved") && msg.contains("shortcut"));
    }

    #[test]
    fn a_cache_reuse_reads_as_success_not_as_a_warning() {
        let fb = feedback_for(FlowOutcome::ReusedFromCache);
        assert_eq!(fb.phase, "success");
    }
}
