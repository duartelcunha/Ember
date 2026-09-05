//! Resolucao do perfil de personalizacao: override do utilizador, ou CLAUDE.md
//! auto-detetado, ou o perfil de qualidade por defeito.

use ember_core::model::{Profile, ProfileSource};
use ember_core::profile_path::{pick_existing, profile_candidates};
use std::io::Read;
use std::path::Path;
use tauri::{AppHandle, Manager};

// Em ingles: e a lingua da app e dos modelos, e um profile portugues empurrava o output de
// utilizadores nao-portugueses para a lingua errada. A lingua do OUTPUT continua a ser a do
// input (regra dura no prompt); o profile so define estilo.
// Nota: o espaco que une as palavras fica ANTES do `\` (a continuacao do Rust come o
// whitespace do inicio da linha seguinte; com o espaco depois, as palavras fundem-se).
pub const DEFAULT_PROFILE: &str = "\
Write with clarity and precision. Professional but direct tone. Short sentences. Avoid \
unnecessary jargon and filler. When context is missing, keep the request generic or use \
placeholders instead of inventing details or asking the user for clarification.";

pub struct Resolved {
    pub profile: Profile,
    pub path: Option<String>,
}

/// Resolve o perfil a usar. Prioridade: override -> (a menos que ignore) CLAUDE.md -> default.
pub fn resolve(app: &AppHandle, override_text: Option<&str>, ignore_claude_md: bool) -> Resolved {
    if let Some(t) = override_text {
        if !t.trim().is_empty() {
            return Resolved {
                profile: Profile {
                    text: t.to_string(),
                    source: ProfileSource::UserEdited,
                },
                path: None,
            };
        }
    }

    if !ignore_claude_md {
        let home = app.path().home_dir().ok();
        let candidates = profile_candidates(home.as_deref());
        let exists = |p: &Path| p.exists();
        if let Some(p) = pick_existing(&candidates, &exists) {
            if let Ok(text) = read_bounded(&p, 64 * 1024) {
                return Resolved {
                    profile: Profile {
                        text,
                        source: ProfileSource::ClaudeMd,
                    },
                    path: Some(p.display().to_string()),
                };
            }
        }
    }

    Resolved {
        profile: Profile {
            text: DEFAULT_PROFILE.to_string(),
            source: ProfileSource::Default,
        },
        path: None,
    }
}

/// Read a bounded UTF-8 snapshot, including when a file grows after metadata inspection.
pub(crate) fn read_bounded(path: &Path, limit: u64) -> Result<String, String> {
    let file = std::fs::File::open(path).map_err(|_| "Source could not be opened")?;
    if !file
        .metadata()
        .map_err(|_| "Source metadata unavailable")?
        .is_file()
    {
        return Err("Source must be a regular file".into());
    }
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Source could not be read")?;
    if bytes.len() as u64 > limit {
        return Err("Source exceeds the read limit".into());
    }
    String::from_utf8(bytes).map_err(|_| "Source must be UTF-8 text".into())
}
