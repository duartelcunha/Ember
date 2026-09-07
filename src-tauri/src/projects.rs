//! Ler a pasta de um projeto e destilar um brief a partir do ficheiro que la tiver mais
//! convencoes. So I/O e orquestracao: quem escolhe o ficheiro, quem valida o brief e quem monta o
//! prompt vive tudo em `ember_core` e testa-se sem disco e sem rede.

use std::path::{Path, PathBuf};

use ember_core::project::{self, Found};
use ember_core::projects as core;
use tauri::AppHandle;

use crate::state::AppState;

/// Teto por ficheiro lido. O mesmo do seletor de perfil (`read_profile_file`): acima disto nao e
/// um ficheiro de convencoes, e nao ha razao para o carregar todo para memoria.
const MAX_SOURCE_BYTES: u64 = 512 * 1024;

/// O que se encontrou numa pasta, para a UI poder MOSTRAR antes de enviar seja o que for.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Scan {
    /// O ficheiro escolhido. `None` = a pasta nao tem nenhum com conteudo a serio, e isso e legal.
    pub source_path: Option<String>,
    /// Nome curto do ficheiro (`AGENTS.md`), para a UI nao ter de o extrair do caminho.
    pub file_name: Option<String>,
    /// Linhas do ficheiro escolhido, para a pessoa perceber a dimensao do que vai enviar.
    pub lines: usize,
    pub source_fingerprint: String,
    pub source_paths: Vec<String>,
    pub warnings: Vec<String>,
    /// Todos os candidatos que existem na pasta, com o peso de cada um. E o que torna a escolha
    /// explicavel: da para ver que o `CLAUDE.md` de uma linha perdeu para o `AGENTS.md`.
    pub candidates: Vec<Candidate>,
    /// Subpastas que TEM ficheiro de convencoes, quando esta nao tem.
    ///
    /// Existe por um erro que acontece de verdade e e natural: apontar a pasta-mae em vez do repo.
    /// O `~/deleg8lab/E2O` tem quatro repos la dentro e nenhum ficheiro proprio; dizer so "nao ha
    /// nada aqui" e verdade e nao ajuda nada. Um nivel, e nao uma varredura: mais do que isso
    /// seria andar a ler a arvore de alguem sem ele pedir.
    pub subfolders: Vec<Subfolder>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Subfolder {
    pub name: String,
    pub path: String,
    pub file_name: String,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub file_name: String,
    pub score: usize,
    pub chosen: bool,
}

fn read_capped(path: &Path, root: &Path) -> Option<String> {
    let root = root.canonicalize().ok()?;
    let path = path.canonicalize().ok()?;
    if !path.starts_with(&root) {
        return None;
    }
    crate::profile::read_bounded(&path, MAX_SOURCE_BYTES).ok()
}

struct Sources {
    root: PathBuf,
    visited: std::collections::HashSet<PathBuf>,
    active: std::collections::HashSet<PathBuf>,
    values: Vec<(Found, String)>,
    warnings: Vec<String>,
    bytes: usize,
    oversized: bool,
}

impl Sources {
    fn visit(&mut self, found: Found, depth: usize) {
        let Ok(path) = found.path.canonicalize() else {
            self.warnings
                .push("A source could not be resolved and was excluded.".into());
            return;
        };
        if !path.starts_with(&self.root) || !path.is_file() {
            self.warnings.push(
                "A source outside the selected directory or not a regular file was excluded."
                    .into(),
            );
            return;
        }
        if self.active.contains(&path) {
            self.warnings
                .push("A cyclic local import was excluded.".into());
            return;
        }
        if self.visited.contains(&path) {
            return;
        }
        if depth > 8 || self.visited.len() >= 32 {
            self.warnings.push(
                "Local import limit reached (8 levels, 32 files). Some sources were excluded."
                    .into(),
            );
            return;
        }
        self.visited.insert(path.clone());
        let Ok(text) = crate::profile::read_bounded(&path, MAX_SOURCE_BYTES) else {
            self.warnings
                .push("A source was excluded: unreadable, too large or not UTF-8.".into());
            return;
        };
        self.bytes += text.len();
        if self.bytes > MAX_SOURCE_BYTES as usize {
            self.oversized = true;
            return;
        }
        let imports: Vec<_> = text
            .lines()
            .filter_map(|line| {
                let value = line.trim().strip_prefix('@')?.trim().trim_matches('"');
                Some(value.to_owned())
            })
            .collect();
        self.values.push((
            Found {
                path: path.clone(),
                kind: found.kind,
            },
            text,
        ));
        self.active.insert(path.clone());
        for import in imports {
            let relative = Path::new(&import);
            // Only explicit local Markdown imports are followed. No URLs, globs or home paths.
            if relative.is_absolute()
                || import.contains(':')
                || import.starts_with('~')
                || relative
                    .extension()
                    .is_none_or(|e| e.to_str().is_none_or(|e| !e.eq_ignore_ascii_case("md")))
            {
                self.warnings.push("An unsupported import was excluded. Use a relative Markdown path on its own @ line.".into());
                continue;
            }
            self.visit(
                Found {
                    path: path.parent().unwrap_or(&self.root).join(relative),
                    kind: found.kind,
                },
                depth + 1,
            );
        }
        self.active.remove(&path);
    }
}

fn collect(folder: &Path) -> (Vec<(Found, String)>, Vec<String>) {
    let Ok(root) = folder.canonicalize() else {
        return (
            Vec::new(),
            vec!["The selected directory is unavailable.".into()],
        );
    };
    let mut sources = Sources {
        root,
        visited: Default::default(),
        active: Default::default(),
        values: Vec::new(),
        warnings: Vec::new(),
        bytes: 0,
        oversized: false,
    };
    for found in project::candidates_in(folder, &|p| p.exists()) {
        sources.visit(found, 0);
    }
    if sources.oversized {
        sources.values.clear();
        sources
            .warnings
            .push("Combined sources exceed 512 KiB. Extraction is disabled.".into());
    }
    (sources.values, sources.warnings)
}

fn fingerprint(sources: &[(Found, String)]) -> String {
    use sha2::{Digest, Sha256};
    let mut hash = Sha256::new();
    for (source, text) in sources {
        hash.update(source.path.to_string_lossy().as_bytes());
        hash.update([0]);
        hash.update(text.as_bytes());
        hash.update([0]);
    }
    format!("{:x}", hash.finalize())
}

fn short_name(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| p.to_string_lossy().into_owned())
}

/// Le os candidatos da pasta e diz qual serve. NAO envia nada para lado nenhum: existe
/// precisamente para a pessoa ver o que seria enviado antes de decidir.
pub fn scan(folder: &Path) -> Scan {
    let (with_text, warnings) = collect(folder);
    let chosen = core::pick_source(&with_text).map(|f| f.path.clone());
    let candidates = with_text
        .iter()
        .map(|(f, t)| Candidate {
            file_name: short_name(&f.path),
            score: core::content_score(t),
            chosen: Some(&f.path) == chosen.as_ref(),
        })
        .collect();
    let lines = chosen
        .as_ref()
        .and_then(|p| with_text.iter().find(|(f, _)| &f.path == p))
        .map(|(_, t)| t.lines().count())
        .unwrap_or(0);

    // So se procura um nivel abaixo quando ESTA pasta nao tem nada. Com ficheiro proprio, a
    // resposta ja esta dada e ir espreitar subpastas seria ler o que ninguem pediu.
    let subfolders = if chosen.is_none() {
        subfolders_with_context(folder)
    } else {
        Vec::new()
    };

    Scan {
        source_fingerprint: fingerprint(&with_text),
        source_paths: with_text
            .iter()
            .map(|(f, _)| f.path.to_string_lossy().into_owned())
            .collect(),
        warnings,
        file_name: chosen.as_ref().map(|p| short_name(p)),
        source_path: chosen.map(|p| p.to_string_lossy().into_owned()),
        lines,
        candidates,
        subfolders,
    }
}

/// Subpastas diretas que tem um ficheiro de convencoes com conteudo a serio.
///
/// Um nivel so, e no maximo doze, ordenadas por nome. Nao le o conteudo de nada que nao seja um
/// dos ficheiros conhecidos, e nao entra em pastas escondidas nem em `node_modules` e afins: isto
/// e para ajudar a acertar na pasta, nao para varrer o disco.
fn subfolders_with_context(root: &Path) -> Vec<Subfolder> {
    const IGNORAR: [&str; 5] = ["node_modules", "target", "dist", "build", ".git"];
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut dirs: Vec<PathBuf> = entries
        .take(256)
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .filter(|p| {
            let n = short_name(p);
            !n.starts_with('.') && !IGNORAR.contains(&n.as_str())
        })
        .collect();
    dirs.sort();

    dirs.iter()
        .filter_map(|dir| {
            let cands: Vec<(Found, String)> = project::candidates_in(dir, &|p| p.exists())
                .into_iter()
                .filter_map(|f| read_capped(&f.path, root).map(|t| (f, t)))
                .collect();
            let escolhido = core::pick_source(&cands)?;
            Some(Subfolder {
                name: short_name(dir),
                path: dir.to_string_lossy().into_owned(),
                file_name: short_name(&escolhido.path),
            })
        })
        .take(12)
        .collect()
}

/// Porque e que a destilacao nao deu um brief. Todas terminam no mesmo sitio na UI (projeto criado
/// com brief vazio e um botao de tentar outra vez), mas a mensagem tem de dizer a verdade.
pub enum DistillFail {
    SourceChanged,
    NoSource,
    Provider(ember_core::CoreError),
    NothingUseful,
    Rejected,
}

impl DistillFail {
    pub fn message(&self) -> String {
        match self {
            DistillFail::SourceChanged => "Sources changed since the folder was scanned. Select the folder again before extracting.".into(),
            DistillFail::NoSource => {
                "No conventions file in that folder. Write the brief yourself below.".into()
            }
            DistillFail::Provider(e) => format!(
                "Couldn't reach the model to read it ({}). Try again.",
                crate::commands::friendly_error(e)
            ),
            DistillFail::NothingUseful => {
                "That file has plenty in it, but no useful writing preferences or technical facts. Write the brief \
                 yourself below."
                    .into()
            }
            DistillFail::Rejected => {
                "The summary came back unusable and was discarded. Try again, or write it yourself."
                    .into()
            }
        }
    }
}

/// Le o ficheiro escolhido e devolve um brief pronto a rever.
///
/// Tres coisas nao sao negociaveis neste caminho, e todas ja mordera alguem noutro sitio:
/// 1. o ficheiro e redigido ANTES de sair da maquina (`redact_secrets`);
/// 2. o resultado e redigido OUTRA VEZ, porque o modelo pode ter copiado uma chave para o resumo;
/// 3. uma falha nunca cai para o conteudo cru do ficheiro. Um repo de cliente por rever dentro de
///    todos os refines e exatamente o que o `project_context: false` existe para impedir.
pub async fn distill(
    app: &AppHandle,
    state: &AppState,
    folder: &Path,
    expected_fingerprint: Option<&str>,
) -> Result<(String, PathBuf), DistillFail> {
    let (sources, _) = collect(folder);
    if sources.is_empty() {
        return Err(DistillFail::NoSource);
    }
    if expected_fingerprint.is_some_and(|expected| expected != fingerprint(&sources)) {
        return Err(DistillFail::SourceChanged);
    }
    let path = sources[0].0.path.clone();
    // All known root documents participate. Paths label data; they never grant authority.
    let fonte = sources
        .iter()
        .map(|(found, text)| {
            format!(
                "Source: {}\n{}",
                found
                    .path
                    .strip_prefix(
                        folder
                            .canonicalize()
                            .unwrap_or_else(|_| folder.to_path_buf())
                    )
                    .unwrap_or(&found.path)
                    .display(),
                project::redact_secrets(text)
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    let cfg = crate::config::load(app);
    let chain = crate::commands::build_chain(app, state, &cfg)
        .await
        .map_err(DistillFail::Provider)?;
    // O modelo aqui e so um placeholder: o `refine` substitui-o pelo do passo que estiver a
    // correr. Passar o do primeiro passo mantem o pedido honesto se alguem o inspecionar.
    let modelo = chain.first().map(|s| s.model.clone()).unwrap_or_default();
    let req = ember_core::prompt::build_distill_request(&fonte, &modelo);

    let rcfg = ember_core::retry::RetryConfig {
        step_count: chain.len(),
        step_providers: chain.iter().map(|s| s.provider).collect(),
        ..Default::default()
    };
    let pctx = crate::providers::ProviderCtx {
        on_response: None,
        openai_base_url: &cfg.openai_base_url,
    };
    let resp = crate::providers::refine(
        &state.http,
        &rcfg,
        &chain,
        &req,
        &pctx,
        &|_, _, _| {},
        &|_| {},
    )
    .await
    .map_err(DistillFail::Provider)?;

    match core::validate_brief(&resp.text, &project::redact_secrets) {
        Ok(brief) => Ok((brief, path)),
        Err(core::BriefError::NothingUseful) => Err(DistillFail::NothingUseful),
        Err(e) => {
            log::warn!("destilacao rejeitada: {e:?}");
            Err(DistillFail::Rejected)
        }
    }
}

#[cfg(test)]
mod source_tests {
    use super::*;
    #[test]
    fn imports_are_bounded_scoped_and_part_of_the_source_revision() {
        let root = std::env::temp_dir().join(format!(
            "ember-source-test-{}-{}",
            std::process::id(),
            crate::now_ms()
        ));
        std::fs::create_dir(&root).unwrap();
        std::fs::write(
            root.join("CLAUDE.md"),
            "Writing: concise.\n@facts.md\n@../outside.md\n",
        )
        .unwrap();
        std::fs::write(
            root.join("facts.md"),
            "Architecture: Rust core.\n@CLAUDE.md\n",
        )
        .unwrap();
        let (sources, warnings) = collect(&root);
        assert_eq!(sources.len(), 2);
        assert!(warnings.iter().any(|w| w.contains("cyclic")));
        assert!(warnings.len() >= 2);
        let before = fingerprint(&sources);
        std::fs::write(root.join("facts.md"), "Architecture: revised.\n").unwrap();
        assert_ne!(fingerprint(&collect(&root).0), before);
        std::fs::remove_file(root.join("CLAUDE.md")).unwrap();
        std::fs::remove_file(root.join("facts.md")).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
}
