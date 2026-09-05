//! De quem e a overlay agora, e o que a tecla de atalho faz.
//!
//! Parece pouco para um modulo. Vive aqui porque a versao anterior destas regras estava
//! espalhada pelo shell, sem testes, e produziu um bug que quebrava uma regra inviolavel do
//! projeto (um hook LL de cada vez): a guarda de reentrancia era libertada por um ciclo que ja
//! nao era o dono, e um segundo refine arrancava por cima de um que ainda decorria, com dois
//! hooks de teclado vivos e dois a mexer no clipboard.
//!
//! Os ciclos SOBREPOEM-SE de proposito: a guarda liberta quando o trabalho acaba, nao quando a
//! pilula desaparece, senao um atalho carregado durante os ~2s dela era engolido. E por isso que
//! e preciso perguntar de quem e a overlay, em vez de assumir que so ha um ciclo.

/// O ciclo `run_id` ainda e o dono da overlay?
///
/// `current` e o numero do ciclo mais recente. Como os numeros so crescem, "dono" e "o mais
/// recente" sao a mesma coisa, e um ciclo antigo a acabar sabe que ja nao manda.
pub fn owns_overlay(current: u64, run_id: u64) -> bool {
    current == run_id
}

/// Este ciclo pode esconder o que esta no ecra?
///
/// Nao, se ja houver um ciclo mais novo: a pilula que ia esconder ja nao e a dele, e escondia-a
/// a meio do trabalho do outro.
pub fn may_hide(current: u64, run_id: u64) -> bool {
    owns_overlay(current, run_id)
}

/// Este ciclo pode libertar a guarda de reentrancia?
///
/// Mesma regra, e e o coracao do bug: a libertacao no fim do `run` era incondicional, mas o
/// `run` so termina depois da pilula, e a essa altura pode ja haver outro ciclo a decorrer. A
/// libertacao punha a `false` uma guarda que era do OUTRO, e a tecla seguinte arrancava um
/// terceiro ciclo em paralelo com ele.
pub fn may_release_guard(current: u64, run_id: u64) -> bool {
    owns_overlay(current, run_id)
}

/// O que a tecla de atalho faz, dado o estado.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyAction {
    /// Nada a decorrer: comeca um ciclo.
    Start,
    /// Ja ha trabalho: tira a espera da frente do utilizador. NAO mata a chamada ao modelo, que
    /// segue e guarda o resultado.
    Dismiss,
}

pub fn hotkey_action(busy: bool) -> HotkeyAction {
    if busy {
        HotkeyAction::Dismiss
    } else {
        HotkeyAction::Start
    }
}

/// The native interaction has one owner. Background provider requests have their own lifetime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunPhase {
    Capturing,
    Requesting,
    Reviewing,
    Applying,
    Cancelled,
}

#[derive(Debug, Default)]
pub struct ExecutionCoordinator {
    next_id: u64,
    active: Option<(u64, RunPhase)>,
}

impl ExecutionCoordinator {
    /// An occupied coordinator returns its owner so a second hotkey can dismiss that run.
    pub fn begin(&mut self) -> Result<u64, u64> {
        if let Some((id, _)) = self.active {
            return Err(id);
        }
        self.next_id = self
            .next_id
            .checked_add(1)
            .expect("run identifiers exhausted");
        self.active = Some((self.next_id, RunPhase::Capturing));
        Ok(self.next_id)
    }

    pub fn is_busy(&self) -> bool {
        self.active.is_some()
    }

    pub fn is_phase(&self, id: u64, phase: RunPhase) -> bool {
        self.active == Some((id, phase))
    }

    pub fn cancelled(&self, id: u64) -> bool {
        !matches!(self.active, Some((owner, phase)) if owner == id && phase != RunPhase::Cancelled)
    }

    pub fn cancel(&mut self, id: u64) -> bool {
        match self.active.as_mut() {
            Some((owner, phase)) if *owner == id => {
                *phase = RunPhase::Cancelled;
                true
            }
            _ => false,
        }
    }

    pub fn advance(&mut self, id: u64, next: RunPhase) -> bool {
        use RunPhase::*;
        let Some((owner, phase)) = self.active.as_mut() else {
            return false;
        };
        if *owner != id
            || !matches!(
                (*phase, next),
                (Capturing, Requesting | Reviewing)
                    | (Requesting, Reviewing | Applying)
                    | (Reviewing, Applying)
            )
        {
            return false;
        }
        *phase = next;
        true
    }

    /// Completion is idempotent and cannot release a newer run's ownership.
    pub fn complete(&mut self, id: u64) -> bool {
        if self.active.is_some_and(|(owner, _)| owner == id) {
            self.active = None;
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_newest_cycle_owns_the_overlay() {
        assert!(owns_overlay(7, 7));
        assert!(!owns_overlay(8, 7));
    }

    #[test]
    fn an_old_cycle_may_neither_hide_nor_release_the_guard() {
        // O cenario exato do bug: o ciclo 1 mostra a pilula e liberta a guarda; o utilizador
        // carrega no atalho e nasce o ciclo 2; dois segundos depois o `run` do ciclo 1 acaba.
        // Se ele libertasse a guarda ai, a tecla seguinte arrancava um terceiro ciclo EM
        // PARALELO com o segundo: dois hooks LL de teclado vivos e dois a armar o clipboard.
        let current = 2;
        assert!(!may_release_guard(current, 1));
        assert!(!may_hide(current, 1));
        assert!(may_release_guard(current, 2));
        assert!(may_hide(current, 2));
    }

    #[test]
    fn the_hotkey_starts_when_idle_and_dismisses_when_busy() {
        assert_eq!(hotkey_action(false), HotkeyAction::Start);
        assert_eq!(hotkey_action(true), HotkeyAction::Dismiss);
    }

    #[test]
    fn cancellation_cannot_be_reversed_by_a_late_provider_or_preview() {
        use RunPhase::*;
        for phase in [Capturing, Requesting, Reviewing, Applying] {
            let mut runs = ExecutionCoordinator::default();
            let id = runs.begin().unwrap();
            if phase != Capturing {
                assert!(runs.advance(id, Requesting));
            }
            if matches!(phase, Reviewing | Applying) {
                assert!(runs.advance(id, Reviewing));
            }
            if phase == Applying {
                assert!(runs.advance(id, Applying));
            }
            assert!(runs.cancel(id));
            assert!(!runs.advance(id, Applying));
            assert!(!runs.advance(id, Reviewing));
            assert!(!runs.is_phase(id, Applying));
            assert_eq!(runs.begin(), Err(id));
            assert!(runs.complete(id));
        }
    }

    #[test]
    fn late_cleanup_and_cancellation_do_not_touch_the_next_run() {
        let mut runs = ExecutionCoordinator::default();
        let first = runs.begin().unwrap();
        assert!(runs.complete(first));
        let second = runs.begin().unwrap();
        assert!(!runs.complete(first));
        assert!(!runs.cancel(first));
        assert!(!runs.advance(first, RunPhase::Requesting));
        assert!(runs.is_phase(second, RunPhase::Capturing));
        assert!(runs.cancelled(first));
        assert!(!runs.cancelled(second));
    }

    #[test]
    fn application_requires_a_completed_capture_path() {
        use RunPhase::*;
        let mut runs = ExecutionCoordinator::default();
        let id = runs.begin().unwrap();
        assert!(!runs.advance(id, Applying));
        assert!(runs.advance(id, Reviewing)); // Reapply still requires capture and review.
        assert!(runs.advance(id, Applying));
        assert!(!runs.advance(id, Requesting));
        assert!(!runs.advance(id, Applying));
        assert!(runs.complete(id));
        assert!(!runs.is_busy());
        assert!(!runs.advance(id, Applying));
    }
}
