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
}
