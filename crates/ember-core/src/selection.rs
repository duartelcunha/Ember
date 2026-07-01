//! Sequenciamento puro de captura/substituicao de seleccao (clipboard-sentinel).
//! Sem SO nem rede: o I/O real (enigo/arboard) vive no shell src-tauri.

/// Abstrai o I/O necessario para capturar/substituir a seleccao.
pub trait SelectionIo {
    fn clip_get(&mut self) -> Option<String>;
    fn clip_set(&mut self, s: &str);
    /// Liberta modificadores fisicos do hotkey (Ctrl/Shift/Alt) antes de simular.
    fn release_modifiers(&mut self);
    fn send_copy(&mut self);
    fn send_paste(&mut self);
    fn sleep_ms(&mut self, ms: u64);
}

/// Resultado da captura: `text` = seleccao (None se nada selecionado);
/// `saved` = clipboard original a restaurar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Captured {
    pub text: Option<String>,
    pub saved: Option<String>,
}

/// Captura a seleccao sem destruir o clipboard: guarda o original, escreve um
/// sentinela, simula Ctrl+C e faz poll. Se o clipboard continuar = sentinela,
/// nada foi selecionado (`text == None`).
pub fn capture(io: &mut impl SelectionIo, sentinel: &str, polls: u32, step_ms: u64) -> Captured {
    let saved = io.clip_get();
    io.release_modifiers();
    io.sleep_ms(step_ms);
    io.clip_set(sentinel);
    io.send_copy();
    let mut text = None;
    for _ in 0..polls {
        io.sleep_ms(step_ms);
        match io.clip_get() {
            Some(t) if t != sentinel => {
                text = Some(t);
                break;
            }
            _ => {}
        }
    }
    Captured { text, saved }
}

/// Substitui a seleccao: poe o refinado no clipboard, simula Ctrl+V, espera o
/// paste assentar e restaura o clipboard original.
pub fn replace(io: &mut impl SelectionIo, refined: &str, saved: &Option<String>, settle_ms: u64) {
    io.clip_set(refined);
    io.send_paste();
    io.sleep_ms(settle_ms);
    restore(io, saved);
}

/// Restaura o clipboard original (best-effort: so texto).
pub fn restore(io: &mut impl SelectionIo, saved: &Option<String>) {
    if let Some(s) = saved {
        io.clip_set(s);
    }
}

/// Clampa a posicao (x,y) de uma janela wxh a uma area de trabalho, para o orb
/// nunca sair do ecra.
pub fn clamp_pos(
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    area_x: i32,
    area_y: i32,
    area_w: i32,
    area_h: i32,
) -> (i32, i32) {
    let max_x = area_x + (area_w - w).max(0);
    let max_y = area_y + (area_h - h).max(0);
    (x.clamp(area_x, max_x), y.clamp(area_y, max_y))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct FakeIo {
        clipboard: Option<String>,
        /// O que o "SO" copiaria com Ctrl+C (None = nada selecionado).
        selection: Option<String>,
        pasted: Option<String>,
    }

    impl SelectionIo for FakeIo {
        fn clip_get(&mut self) -> Option<String> {
            self.clipboard.clone()
        }
        fn clip_set(&mut self, s: &str) {
            self.clipboard = Some(s.to_string());
        }
        fn release_modifiers(&mut self) {}
        fn send_copy(&mut self) {
            if let Some(sel) = &self.selection {
                self.clipboard = Some(sel.clone());
            }
        }
        fn send_paste(&mut self) {
            self.pasted = self.clipboard.clone();
        }
        fn sleep_ms(&mut self, _ms: u64) {}
    }

    const SENT: &str = "__ember_sentinel__";

    #[test]
    fn captures_selected_text() {
        let mut io = FakeIo {
            clipboard: Some("old".into()),
            selection: Some("hello world".into()),
            ..Default::default()
        };
        let c = capture(&mut io, SENT, 5, 1);
        assert_eq!(c.text, Some("hello world".into()));
        assert_eq!(c.saved, Some("old".into()));
    }

    #[test]
    fn empty_when_nothing_selected() {
        let mut io = FakeIo {
            clipboard: Some("old".into()),
            selection: None,
            ..Default::default()
        };
        let c = capture(&mut io, SENT, 5, 1);
        assert_eq!(c.text, None);
        assert_eq!(c.saved, Some("old".into()));
    }

    #[test]
    fn replace_pastes_refined_and_restores_clipboard() {
        let mut io = FakeIo {
            clipboard: Some("old".into()),
            selection: Some("hi".into()),
            ..Default::default()
        };
        let c = capture(&mut io, SENT, 5, 1);
        replace(&mut io, "REFINED", &c.saved, 1);
        assert_eq!(io.pasted, Some("REFINED".into()));
        assert_eq!(io.clipboard, Some("old".into()));
    }

    #[test]
    fn restore_with_none_saved_does_not_panic() {
        let mut io = FakeIo {
            clipboard: Some(SENT.into()),
            ..Default::default()
        };
        restore(&mut io, &None);
        // clipboard inalterado (best-effort): nao rebenta.
        assert_eq!(io.clipboard, Some(SENT.into()));
    }

    #[test]
    fn clamp_keeps_window_on_screen() {
        // cursor perto do canto inferior-direito: a janela e empurrada para dentro.
        assert_eq!(clamp_pos(1910, 1070, 260, 100, 0, 0, 1920, 1080), (1660, 980));
        // dentro: inalterado.
        assert_eq!(clamp_pos(100, 100, 260, 100, 0, 0, 1920, 1080), (100, 100));
    }
}
