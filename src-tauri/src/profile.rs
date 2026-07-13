//! Resolucao do perfil de personalizacao: override do utilizador, ou CLAUDE.md
//! auto-detetado, ou o perfil de qualidade por defeito.

use ember_core::model::{Profile, ProfileSource};
use ember_core::profile_path::{pick_existing, profile_candidates};
use std::fs;
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
            if let Ok(text) = fs::read_to_string(&p) {
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
