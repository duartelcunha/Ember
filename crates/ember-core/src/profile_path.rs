//! Resolucao pura do caminho do perfil de personalizacao.
//!
//! O adapter passa os diretorios base e um predicado de existencia; a logica de qual
//! candidato escolher fica pura e testavel sem tocar no filesystem.

use std::path::{Path, PathBuf};

/// Candidatos a perfil GLOBAL, por ordem de preferencia.
///
/// Nao e so o `CLAUDE.md`: o mesmo ficheiro de "como eu escrevo" existe com outros nomes
/// consoante a ferramenta (o `AGENTS.md` e o standard aberto que o Codex, o Cursor e o Copilot
/// leem; o `GEMINI.md` e do Gemini CLI). Detetar so um deles deixava de fora toda a gente que nao
/// usa Claude, apesar de terem exatamente o mesmo ficheiro a dizer exatamente a mesma coisa.
///
/// A ordem e deliberada: as pastas de ferramenta (`~/.claude/`, `~/.codex/`, `~/.gemini/`) vem
/// antes das da home, porque um ficheiro solto na home e mais provavel ser de um projeto que ali
/// calhou do que a configuracao global da pessoa.
///
/// So o perfil GLOBAL, nunca o do projeto: o Ember e uma app de tray/autostart e o cwd do
/// processo depende de como o SO a lancou (autostart, atalho, Explorer), portanto nao diz nada
/// sobre "o projeto em que estou a trabalhar". O contexto do PROJETO em foco e uma feature
/// separada e opt-in (ver `project.rs`), por causa da privacidade.
pub fn profile_candidates(home: Option<&Path>) -> Vec<PathBuf> {
    let Some(h) = home else {
        return Vec::new();
    };
    vec![
        h.join(".claude").join("CLAUDE.md"),
        h.join(".codex").join("AGENTS.md"),
        h.join(".gemini").join("GEMINI.md"),
        h.join("AGENTS.md"),
        h.join("CLAUDE.md"),
    ]
}

/// Devolve o primeiro candidato que existe, segundo o predicado injetado.
pub fn pick_existing(candidates: &[PathBuf], exists: &dyn Fn(&Path) -> bool) -> Option<PathBuf> {
    candidates.iter().find(|p| exists(p)).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_profile_of_every_major_agent_tool_not_just_claude() {
        let home = PathBuf::from("/home/u");
        let c = profile_candidates(Some(&home));
        // O mesmo ficheiro tem nomes diferentes conforme a ferramenta. Detetar so o do Claude
        // deixava de fora quem usa Codex, Cursor, Copilot (AGENTS.md) ou o Gemini CLI.
        assert!(c.contains(&PathBuf::from("/home/u/.claude/CLAUDE.md")));
        assert!(c.contains(&PathBuf::from("/home/u/.codex/AGENTS.md")));
        assert!(c.contains(&PathBuf::from("/home/u/.gemini/GEMINI.md")));
        assert!(c.contains(&PathBuf::from("/home/u/AGENTS.md")));
        // O do Claude ganha quando existem varios (o Ember nasceu nesse ecossistema).
        assert_eq!(c[0], PathBuf::from("/home/u/.claude/CLAUDE.md"));
        // As pastas de ferramenta vem antes dos ficheiros soltos na home: um ficheiro na raiz da
        // home e mais provavel ser de um projeto que ali calhou do que a config global.
        let idx = |s: &str| c.iter().position(|p| p == &PathBuf::from(s)).unwrap();
        assert!(idx("/home/u/.gemini/GEMINI.md") < idx("/home/u/AGENTS.md"));
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
