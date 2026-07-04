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
    /// O clipboard tem conteudo que a app nao sabe preservar (ficheiros, RTF, ...).
    UnpreservableClipboard,
    /// O sentinela nao armou: outra app tinha o clipboard ocupado no momento da captura.
    ClipboardBusy,
    /// Nao havia seleccao (o poll esgotou sem o clipboard mudar).
    NoSelectionFound,
    /// Uma segunda tecla cancelou o ciclo em curso.
    Cancelled,
    /// O texto refinado nao chegou a ser armado no clipboard antes do paste.
    PasteFailed,
    /// Refinamento e paste bem sucedidos.
    Success { provider: String },
    /// O refinamento falhou; `message` ja vem amigavel (de `friendly_error`).
    RefineFailed { message: String },
    /// O motor recusou colar (output vazio, ou perdeu/mutou um span de codigo/URL): a
    /// seleccao do utilizador ficou intacta, em vez de colar por cima algo partido.
    RefineUnclean,
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
            message: None,
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
    }
}

#[cfg(test)]
mod tests {
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
    fn success_carries_the_provider_and_no_message() {
        let fb = feedback_for(FlowOutcome::Success {
            provider: "Claude".into(),
        });
        assert_eq!(fb.phase, "success");
        assert_eq!(fb.message, None);
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
}
