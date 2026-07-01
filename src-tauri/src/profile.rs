//! Resolucao do perfil de personalizacao: override do utilizador, ou CLAUDE.md
//! auto-detetado, ou o perfil de qualidade por defeito.

use ember_core::model::{Profile, ProfileSource};
use ember_core::profile_path::{pick_existing, profile_candidates};
use std::fs;
use std::path::Path;
use tauri::{AppHandle, Manager};

pub const DEFAULT_PROFILE: &str = "\
Escreve com clareza e precisao. Tom profissional mas direto. Frases curtas. Evita jargao\
 desnecessario e enchimento. Quando faltar contexto, pede o que e essencial em vez de assumir.";

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
        let cwd = std::env::current_dir().ok();
        let candidates = profile_candidates(home.as_deref(), cwd.as_deref());
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
