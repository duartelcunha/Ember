//! Resolucao pura do caminho do perfil (CLAUDE.md).
//!
//! O adapter passa os diretorios base e um predicado de existencia; a logica de qual
//! candidato escolher fica pura e testavel sem tocar no filesystem.

use std::path::{Path, PathBuf};

/// Constroi a lista ordenada de candidatos a partir dos diretorios base.
/// Ordem: o CLAUDE.md global (perfil pessoal) primeiro, depois o do projeto/cwd.
pub fn profile_candidates(home: Option<&Path>, cwd: Option<&Path>) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(h) = home {
        out.push(h.join(".claude").join("CLAUDE.md"));
    }
    if let Some(c) = cwd {
        out.push(c.join("CLAUDE.md"));
    }
    out
}

/// Devolve o primeiro candidato que existe, segundo o predicado injetado.
pub fn pick_existing(candidates: &[PathBuf], exists: &dyn Fn(&Path) -> bool) -> Option<PathBuf> {
    candidates.iter().find(|p| exists(p)).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_candidates_in_priority_order() {
        let home = PathBuf::from("/home/u");
        let cwd = PathBuf::from("/proj");
        let c = profile_candidates(Some(&home), Some(&cwd));
        assert_eq!(c[0], PathBuf::from("/home/u/.claude/CLAUDE.md"));
        assert_eq!(c[1], PathBuf::from("/proj/CLAUDE.md"));
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
