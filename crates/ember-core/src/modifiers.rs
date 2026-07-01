//! Politica pura de neutralizacao de modificadores presos.
//!
//! Quando a hotkey dispara, o utilizador pode ainda estar fisicamente a segurar
//! Ctrl/Shift/Alt/Win. Injetar Ctrl+C/Ctrl+V por cima corrompe o atalho. Esta funcao
//! decide (sem I/O) se esperar mais, forcar key-ups, ou avancar. O adapter le o estado
//! real (GetAsyncKeyState) e injeta os key-ups; a decisao e testavel aqui.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Modifier {
    Ctrl,
    Shift,
    Alt,
    Win,
}

/// Que modificadores estao fisicamente premidos neste instante.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ModifierState {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub win: bool,
}

impl ModifierState {
    pub fn any_held(&self) -> bool {
        self.ctrl || self.shift || self.alt || self.win
    }

    pub fn held(&self) -> Vec<Modifier> {
        let mut v = Vec::new();
        if self.ctrl {
            v.push(Modifier::Ctrl);
        }
        if self.shift {
            v.push(Modifier::Shift);
        }
        if self.alt {
            v.push(Modifier::Alt);
        }
        if self.win {
            v.push(Modifier::Win);
        }
        v
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NeutralizeDecision {
    /// Nada premido: seguro injetar ja.
    Ready,
    /// Ainda dentro do timeout: esperar pela libertacao natural.
    WaitMore,
    /// Timeout atingido: forcar key-ups destes modificadores antes de injetar.
    ForceRelease(Vec<Modifier>),
}

/// Decide o que fazer dado o estado fisico, o tempo ja esperado e o timeout.
pub fn decide_neutralize(
    state: &ModifierState,
    elapsed_ms: u64,
    timeout_ms: u64,
) -> NeutralizeDecision {
    if !state.any_held() {
        NeutralizeDecision::Ready
    } else if elapsed_ms >= timeout_ms {
        NeutralizeDecision::ForceRelease(state.held())
    } else {
        NeutralizeDecision::WaitMore
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ready_when_nothing_held() {
        assert_eq!(
            decide_neutralize(&ModifierState::default(), 0, 200),
            NeutralizeDecision::Ready
        );
    }

    #[test]
    fn waits_inside_timeout() {
        let s = ModifierState { ctrl: true, ..Default::default() };
        assert_eq!(decide_neutralize(&s, 50, 200), NeutralizeDecision::WaitMore);
    }

    #[test]
    fn force_releases_on_timeout() {
        let s = ModifierState { ctrl: true, alt: true, ..Default::default() };
        assert_eq!(
            decide_neutralize(&s, 200, 200),
            NeutralizeDecision::ForceRelease(vec![Modifier::Ctrl, Modifier::Alt])
        );
    }
}
