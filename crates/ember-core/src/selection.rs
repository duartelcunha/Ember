//! Sequenciamento puro de captura/substituicao de seleccao (clipboard-sentinel).
//! Sem SO nem rede: o I/O real (enigo/arboard) vive no shell src-tauri.

use crate::modifiers::{decide_neutralize, ModifierState, NeutralizeDecision};

/// Abstrai o I/O necessario para capturar/substituir a seleccao.
pub trait SelectionIo {
    fn clip_get(&mut self) -> Option<String>;
    fn clip_set(&mut self, s: &str);
    /// Que modificadores (Ctrl/Shift/Alt/Win) estao fisicamente premidos agora.
    fn modifiers_held(&mut self) -> ModifierState;
    /// Liberta modificadores fisicos do hotkey (Ctrl/Shift/Alt) antes de simular.
    fn release_modifiers(&mut self);
    fn send_copy(&mut self);
    fn send_paste(&mut self);
    fn sleep_ms(&mut self, ms: u64);
}

/// Resultado da captura: `text` = seleccao (None se nada selecionado);
/// `saved` = clipboard original a restaurar; `armed` = o sentinela chegou mesmo ao
/// clipboard (se `false`, o clipboard estava ocupado e nada deve ser colado).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Captured {
    pub text: Option<String>,
    pub saved: Option<String>,
    pub armed: bool,
}

/// Neutraliza os modificadores do hotkey (Ctrl/Shift/Alt/Win) ainda premidos antes de
/// injetar o Ctrl+C. Espera pela libertacao natural ate ao timeout e so entao forca os
/// key-ups: assim evita keyups sinteticos desnecessarios (que teriam efeitos como o Win a
/// abrir o menu Iniciar) quando o utilizador ja largou as teclas. Politica pura em
/// `modifiers::decide_neutralize`, aqui apenas conduzida contra o I/O real.
fn neutralize_modifiers(io: &mut impl SelectionIo, step_ms: u64, timeout_ms: u64) {
    let mut elapsed = 0;
    loop {
        match decide_neutralize(&io.modifiers_held(), elapsed, timeout_ms) {
            NeutralizeDecision::Ready => break,
            NeutralizeDecision::WaitMore => {
                io.sleep_ms(step_ms);
                elapsed = elapsed.saturating_add(step_ms);
            }
            NeutralizeDecision::ForceRelease(_) => {
                io.release_modifiers();
                break;
            }
        }
    }
}

/// Captura a seleccao sem destruir o clipboard: guarda o original, escreve um
/// sentinela, simula Ctrl+C e faz poll. Se o clipboard continuar = sentinela,
/// nada foi selecionado (`text == None`).
///
/// `terminal` ativa o fallback de copy-on-select: muitos terminais (ex. Windows Terminal com
/// "copy on select") poem a seleccao no clipboard AL SELECIONAR, sem precisar de Ctrl+Shift+C.
/// Nesse caso o copy sintetico nao muda o clipboard (a seleccao ja foi consumida), mas o texto
/// que estava no clipboard A ENTRADA (`saved`) E a seleccao. So em terminais, para nao arriscar
/// tratar um clipboard antigo como seleccao fora deles.
pub fn capture(
    io: &mut impl SelectionIo,
    sentinel: &str,
    polls: u32,
    step_ms: u64,
    neutralize_timeout_ms: u64,
    terminal: bool,
) -> Captured {
    let saved = io.clip_get();
    // Neutraliza os modificadores do hotkey. O `GetAsyncKeyState` mente com o hotkey global
    // registado (reporta Ctrl+Shift em baixo mesmo depois de largar), por isso alem da espera
    // natural curta, FORCAMOS SEMPRE os key-ups e damos um settle generoso: e a unica forma de o
    // estado do teclado ficar limpo antes de injetar o Ctrl+Shift+C, sem confiar no sinal que
    // mente. Sem isto, o copy sintetico colide com o Ctrl+Shift ainda "em baixo" e o terminal
    // nao copia (causa raiz confirmada por logs: clipboard fica no sentinela todos os polls).
    neutralize_modifiers(io, step_ms, neutralize_timeout_ms);
    io.release_modifiers();
    io.sleep_ms(step_ms.max(1) * 8); // settle generoso para o estado do teclado assentar
    io.clip_set(sentinel);
    // Confirma que o sentinela ficou mesmo no clipboard. Sem esta guarda, se o `clip_set`
    // falhar em silencio (clipboard bloqueado por outra app), o valor ANTIGO do clipboard
    // (que e != sentinela) seria lido no primeiro poll e tratado como a seleccao, acabando
    // colado por cima do texto real do utilizador. Melhor abortar sem arriscar.
    if io.clip_get().as_deref() != Some(sentinel) {
        return Captured {
            text: None,
            saved,
            armed: false,
        };
    }
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
    // Fallback copy-on-select (so terminais): o Ctrl+Shift+C nao mudou o clipboard, mas se havia
    // texto NAO-vazio a entrada, foi a seleccao a ser copiada ao selecionar. Usa-o. O
    // preview-before-paste (recomendado em terminais) deixa o utilizador confirmar, cobrindo o
    // raro caso de nao ter selecionado nada e haver clipboard antigo.
    if text.is_none() && terminal {
        if let Some(s) = saved.as_deref() {
            if !s.trim().is_empty() && s != sentinel {
                text = Some(s.to_string());
            }
        }
    }
    Captured {
        text,
        saved,
        armed: true,
    }
}

/// Substitui a seleccao: poe o refinado no clipboard, simula Ctrl+V, espera o paste
/// assentar e restaura o clipboard original. Devolve `true` se o texto refinado foi mesmo
/// colocado no clipboard antes do paste (confirmado por leitura). Se `false` (clipboard
/// ocupado), NAO simula o paste (evita colar conteudo errado por cima da seleccao) e
/// restaura o original; o caller deve degradar em vez de reportar sucesso.
#[must_use]
pub fn replace(
    io: &mut impl SelectionIo,
    refined: &str,
    saved: &Option<String>,
    settle_ms: u64,
) -> bool {
    io.clip_set(refined);
    let armed = io.clip_get().as_deref() == Some(refined);
    if armed {
        io.send_paste();
        io.sleep_ms(settle_ms);
    }
    restore(io, saved);
    armed
}

/// Restaura o clipboard original (best-effort: so texto).
pub fn restore(io: &mut impl SelectionIo, saved: &Option<String>) {
    if let Some(s) = saved {
        io.clip_set(s);
    }
}

/// Achata o refinado para uma unica linha, para colar num TERMINAL. Num terminal um `\n` no meio
/// do texto SUBMETE o comando: cada newline executaria uma linha parcial. Colapsa qualquer
/// corrida de espacos/newlines (incl. `\r\n`) num unico espaco e apara as pontas, para o prompt
/// receber uma so linha continua. Pura e testavel; so aplicada quando o alvo e um terminal.
pub fn flatten_for_terminal(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut prev_ws = false;
    for ch in text.chars() {
        if ch.is_whitespace() {
            if !prev_ws {
                out.push(' ');
                prev_ws = true;
            }
        } else {
            out.push(ch);
            prev_ws = false;
        }
    }
    out.trim().to_string()
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

/// Como `clamp_pos`, mas a janela e maior do que o conteudo visivel dentro dela (cxch),
/// cujo canto superior-esquerdo esta em (content_dx, content_dy) relativamente ao canto da
/// janela. Clampa o CONTEUDO ao monitor, nao a janela toda, e devolve o top-left da janela
/// que mantem esse conteudo dentro do monitor.
///
/// Sem isto, uma janela fixa grande (p.ex. 300x140, para caber a pilula de erro) usada so
/// para mostrar um orb pequeno desviava-se muito do cursor perto das bordas do ecra: o
/// clamp continha a janela toda, nao o pontinho visivel la dentro. O `content_dx/dy`
/// generico permite conteudo alinhado a esquerda (pilula) e nao so centrado (orb).
pub fn clamp_window_for_content(
    win_x: i32,
    win_y: i32,
    content_dx: i32,
    content_dy: i32,
    cw: i32,
    ch: i32,
    area_x: i32,
    area_y: i32,
    area_w: i32,
    area_h: i32,
) -> (i32, i32) {
    let content_x = win_x + content_dx;
    let content_y = win_y + content_dy;
    let (cx, cy) = clamp_pos(content_x, content_y, cw, ch, area_x, area_y, area_w, area_h);
    (cx - content_dx, cy - content_dy)
}

/// Encontra o monitor (retangulo x,y,w,h) que contem o ponto (px,py), tipicamente o
/// cursor. Usado para clampar o orb ao ecra ONDE O CURSOR ESTA, nao ao ecra da janela
/// (que fica desatualizado quando o cursor muda de monitor a meio do seguimento).
pub fn monitor_containing(
    px: i32,
    py: i32,
    monitors: &[(i32, i32, i32, i32)],
) -> Option<(i32, i32, i32, i32)> {
    monitors
        .iter()
        .copied()
        .find(|&(x, y, w, h)| px >= x && px < x + w && py >= y && py < y + h)
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
        /// Clipboard bloqueado por outra app: `clip_set` nao tem efeito.
        frozen: bool,
        /// Modificadores fisicamente premidos reportados a politica de neutralizacao.
        held: ModifierState,
        /// `true` depois de `release_modifiers` ser chamado (force-release).
        force_released: bool,
    }

    impl SelectionIo for FakeIo {
        fn clip_get(&mut self) -> Option<String> {
            self.clipboard.clone()
        }
        fn clip_set(&mut self, s: &str) {
            if !self.frozen {
                self.clipboard = Some(s.to_string());
            }
        }
        fn modifiers_held(&mut self) -> ModifierState {
            self.held
        }
        fn release_modifiers(&mut self) {
            self.force_released = true;
        }
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
    const NEUT: u64 = 50;

    #[test]
    fn captures_selected_text() {
        let mut io = FakeIo {
            clipboard: Some("old".into()),
            selection: Some("hello world".into()),
            ..Default::default()
        };
        let c = capture(&mut io, SENT, 5, 1, NEUT, false);
        assert_eq!(c.text, Some("hello world".into()));
        assert_eq!(c.saved, Some("old".into()));
        assert!(c.armed);
    }

    #[test]
    fn empty_when_nothing_selected() {
        let mut io = FakeIo {
            clipboard: Some("old".into()),
            selection: None,
            ..Default::default()
        };
        let c = capture(&mut io, SENT, 5, 1, NEUT, false);
        assert_eq!(c.text, None);
        assert_eq!(c.saved, Some("old".into()));
        assert!(c.armed);
    }

    #[test]
    fn terminal_fallback_uses_copy_on_select_clipboard_when_synthetic_copy_does_nothing() {
        // Copy-on-select (ex. Windows Terminal): a seleccao ja esta no clipboard a entrada, e o
        // Ctrl+Shift+C sintetico nao muda nada (selection=None). Em terminal, usamos o `saved`.
        let mut io = FakeIo {
            clipboard: Some("selected via mouse".into()),
            selection: None, // o copy sintetico nao traz nada
            ..Default::default()
        };
        let c = capture(&mut io, SENT, 5, 1, NEUT, true);
        assert_eq!(c.text, Some("selected via mouse".into()));
        assert!(c.armed);
    }

    #[test]
    fn non_terminal_does_not_fall_back_to_input_clipboard() {
        // Fora de terminal, o mesmo cenario NAO usa o clipboard antigo como seleccao (evita
        // refinar algo que o utilizador nao selecionou).
        let mut io = FakeIo {
            clipboard: Some("old clipboard".into()),
            selection: None,
            ..Default::default()
        };
        let c = capture(&mut io, SENT, 5, 1, NEUT, false);
        assert_eq!(c.text, None);
    }

    #[test]
    fn capture_reports_not_armed_when_clipboard_is_frozen() {
        // Clipboard bloqueado: o sentinela nunca chega la. Sem a guarda de arm, o "old"
        // seria lido como seleccao e colado por cima do texto real (perda de dados).
        let mut io = FakeIo {
            clipboard: Some("old".into()),
            selection: Some("hello".into()),
            frozen: true,
            ..Default::default()
        };
        let c = capture(&mut io, SENT, 5, 1, NEUT, false);
        assert!(!c.armed);
        assert_eq!(c.text, None);
        assert_eq!(c.saved, Some("old".into()));
    }

    #[test]
    fn capture_force_releases_modifiers_still_held_after_timeout() {
        // Ctrl continua premido: espera (WaitMore) ate ao timeout e so entao forca o release.
        let mut io = FakeIo {
            clipboard: Some("old".into()),
            selection: Some("hi".into()),
            held: ModifierState { ctrl: true, ..Default::default() },
            ..Default::default()
        };
        // step 10, timeout 20 -> WaitMore(0), WaitMore(10), ForceRelease(20).
        let c = capture(&mut io, SENT, 5, 10, 20, false);
        assert!(io.force_released);
        assert_eq!(c.text, Some("hi".into()));
    }

    #[test]
    fn capture_always_force_releases_before_copy() {
        // Nova politica: mesmo com nada premido, forcamos os key-ups antes do copy. O sinal
        // fisico (GetAsyncKeyState) nao e de confiar com o hotkey global registado, por isso
        // limpamos sempre o estado do teclado antes de injetar o Ctrl+Shift+C.
        let mut io = FakeIo {
            clipboard: Some("old".into()),
            selection: Some("hi".into()),
            ..Default::default() // held = default: nada premido
        };
        let c = capture(&mut io, SENT, 5, 1, NEUT, false);
        assert!(io.force_released);
        assert_eq!(c.text, Some("hi".into()));
    }

    #[test]
    fn replace_pastes_refined_and_restores_clipboard() {
        let mut io = FakeIo {
            clipboard: Some("old".into()),
            selection: Some("hi".into()),
            ..Default::default()
        };
        let c = capture(&mut io, SENT, 5, 1, NEUT, false);
        let ok = replace(&mut io, "REFINED", &c.saved, 1);
        assert!(ok);
        assert_eq!(io.pasted, Some("REFINED".into()));
        assert_eq!(io.clipboard, Some("old".into()));
    }

    #[test]
    fn replace_reports_failure_and_does_not_paste_when_frozen() {
        // Clipboard bloqueado no momento do replace: nao arma o refinado, por isso NAO cola
        // (evita injetar Ctrl+V sobre conteudo errado) e reporta false.
        let mut io = FakeIo {
            clipboard: Some("old".into()),
            frozen: true,
            ..Default::default()
        };
        let ok = replace(&mut io, "REFINED", &Some("old".into()), 1);
        assert!(!ok);
        assert_eq!(io.pasted, None);
    }

    #[test]
    fn flatten_for_terminal_collapses_newlines_and_runs_of_whitespace() {
        // Newlines viram espaco unico (senao submetiam o comando linha a linha no terminal).
        assert_eq!(
            flatten_for_terminal("linha um\nlinha dois"),
            "linha um linha dois"
        );
        // \r\n, tabs e corridas de espacos colapsam num so espaco.
        assert_eq!(
            flatten_for_terminal("a\r\n\r\n  b\t\tc"),
            "a b c"
        );
        // Pontas aparadas; uma linha simples fica intacta.
        assert_eq!(flatten_for_terminal("  ja e uma linha  "), "ja e uma linha");
        assert_eq!(flatten_for_terminal("sem quebras"), "sem quebras");
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

    #[test]
    fn monitor_containing_finds_point_in_first_monitor() {
        let monitors = [(0, 0, 1920, 1080), (1920, 0, 1920, 1080)];
        assert_eq!(
            monitor_containing(100, 100, &monitors),
            Some((0, 0, 1920, 1080))
        );
    }

    #[test]
    fn monitor_containing_finds_point_in_second_monitor() {
        let monitors = [(0, 0, 1920, 1080), (1920, 0, 1920, 1080)];
        assert_eq!(
            monitor_containing(2500, 500, &monitors),
            Some((1920, 0, 1920, 1080))
        );
    }

    #[test]
    fn monitor_containing_treats_left_edge_as_inside_and_right_edge_as_outside() {
        let monitors = [(0, 0, 1920, 1080), (1920, 0, 1920, 1080)];
        // x=1920 e o primeiro pixel do segundo monitor, nao o ultimo do primeiro.
        assert_eq!(
            monitor_containing(1920, 0, &monitors),
            Some((1920, 0, 1920, 1080))
        );
        // x=1919 e o ultimo pixel do primeiro monitor.
        assert_eq!(
            monitor_containing(1919, 0, &monitors),
            Some((0, 0, 1920, 1080))
        );
    }

    #[test]
    fn monitor_containing_returns_none_outside_all_monitors() {
        let monitors = [(0, 0, 1920, 1080)];
        assert_eq!(monitor_containing(-10, 500, &monitors), None);
        assert_eq!(monitor_containing(500, 2000, &monitors), None);
    }

    #[test]
    fn clamp_window_for_content_left_anchored_content_stays_flush_with_right_edge() {
        // Conteudo 20x20 a 8px do canto esquerdo de uma janela grande, com o cursor
        // perto da borda direita: a caixa visivel tem de parar encostada a borda
        // (2560 - 20 = 2540), e a janela recua o suficiente para isso.
        let content_dx = 8;
        // Posicao da janela que poria o conteudo em x=2555 (fora do ecra sem clamp).
        let win_x = 2555 - content_dx;
        let (wx, _wy) =
            clamp_window_for_content(win_x, 500, content_dx, 60, 20, 20, 0, 0, 2560, 1440);
        assert_eq!(wx + content_dx, 2540);
    }

    #[test]
    fn clamp_window_for_content_left_anchored_content_stays_flush_with_left_edge() {
        let content_dx = 8;
        let win_x = -100 - content_dx; // conteudo pretendido em x=-100 (fora do ecra)
        let (wx, _wy) =
            clamp_window_for_content(win_x, 500, content_dx, 60, 20, 20, 0, 0, 2560, 1440);
        assert_eq!(wx + content_dx, 0);
    }
}
