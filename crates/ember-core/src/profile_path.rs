//! Resolucao pura do caminho do perfil (CLAUDE.md).
//!
//! O adapter passa os diretorios base e um predicado de existencia; a logica de qual
//! candidato escolher fica pura e testavel sem tocar no filesystem.

use std::path::{Path, PathBuf};

/// Constroi a lista de candidatos a partir do diretorio home: so o CLAUDE.md GLOBAL do
/// utilizador. Ember e uma app de tray/autostart, nao uma CLI de projeto: o cwd do
/// processo depende de como o SO a lancou (autostart, atalho, Explorer...) e nao
/// corresponde de forma fiavel a "o projeto em que o utilizador esta a trabalhar", por
/// isso nao ha um candidato baseado em cwd (havia um antes; removido por ser
/// imprevisivel, nao por preguica).
pub fn profile_candidates(home: Option<&Path>) -> Vec<PathBuf> {
    home.map(|h| vec![h.join(".claude").join("CLAUDE.md")])
        .unwrap_or_default()
}

/// Devolve o primeiro candidato que existe, segundo o predicado injetado.
pub fn pick_existing(candidates: &[PathBuf], exists: &dyn Fn(&Path) -> bool) -> Option<PathBuf> {
    candidates.iter().find(|p| exists(p)).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_the_global_candidate_from_home() {
        let home = PathBuf::from("/home/u");
        let c = profile_candidates(Some(&home));
        assert_eq!(c, vec![PathBuf::from("/home/u/.claude/CLAUDE.md")]);
    }

    #[test]
    fn empty_without_home() {
        assert_eq!(profile_candidates(None), Vec::<PathBuf>::new());
    }

    #[test]
    fn picks_first_existing() {
        let candidates = vec![
            PathBuf::from("/home/u/.claude/CLAUDE.md"),
            PathBuf::from("/proj/CLAUDE.md"),
        ];
        // So o segundo existe.
        let exists = |p: &Path| p == Path::new("/proj/CLAUDE.md");
        assert_eq!(
            pick_existing(&candidates, &exists),
            Some(PathBuf::from("/proj/CLAUDE.md"))
        );
    }

    #[test]
    fn none_when_nothing_exists() {
        let candidates = vec![PathBuf::from("/a/CLAUDE.md")];
        let exists = |_: &Path| false;
        assert_eq!(pick_existing(&candidates, &exists), None);
    }
}
