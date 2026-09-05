//! Single background reader for explicitly authorized context sources.
use ember_core::context::{Delivery, Source};
use ember_core::projects::Project;
use sha2::{Digest, Sha256};
use std::{collections::HashMap, path::Path};
use tauri::{AppHandle, Manager};

#[derive(Clone)]
pub struct Cached {
    pub key: String,
    pub sources: Vec<Source>,
    pub status: String,
    metadata: Vec<(u64, Option<std::time::SystemTime>)>,
    checked: std::time::Instant,
}
pub fn key(project: &Project) -> String {
    format!(
        "{:x}",
        Sha256::digest(
            serde_json::to_vec(&(project.folder.as_deref(), &project.context)).unwrap_or_default()
        )
    )
}
fn read_sources(root: &Path, sources: &[Source]) -> Result<Vec<Source>, String> {
    let root = root
        .canonicalize()
        .map_err(|_| "Project folder unavailable")?;
    if sources.len() > 32 {
        return Err("At most 32 authorized sources are allowed".into());
    }
    let mut bytes = 0usize;
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for source in sources {
        let path = Path::new(&source.path)
            .canonicalize()
            .map_err(|_| "Authorized source unavailable")?;
        if !path.starts_with(&root)
            || !seen.insert(path.clone())
            || !path.is_file()
            || !matches!(
                path.extension().and_then(|s| s.to_str()),
                Some("md" | "markdown" | "txt")
            )
        {
            return Err("Sources must be distinct text files inside the project folder".into());
        }
        // Reject retargeted symlinks rather than silently granting a new authorization.
        if path.to_string_lossy() != source.path {
            return Err("Source path changed; authorize it again".into());
        }
        let text = crate::profile::read_bounded(&path, 512 * 1024)
            .map_err(|_| "Authorized source unreadable or too large")?;
        bytes += text.len();
        if bytes > 512 * 1024 {
            return Err("Combined authorized sources exceed 512 KiB".into());
        }
        let extracted = ember_core::profile_import::extract(&text);
        let excluded_lines = extracted.excluded_lines;
        let filtered = ember_core::profile_import::compose(&[extracted]);
        if filtered.trim().is_empty() && !source.text.trim().is_empty() {
            return Err("No usable context extracted".into());
        }
        if filtered.len() > 32 * 1024 {
            return Err("Extracted context is too long; shorten the source".into());
        }
        out.push(Source {
            path: source.path.clone(),
            fingerprint: format!("{:x}", Sha256::digest(text.as_bytes())),
            text: filtered,
            excluded_lines,
        });
    }
    out.sort_by(|a, b| {
        Path::new(&a.path)
            .components()
            .count()
            .cmp(&Path::new(&b.path).components().count())
            .then_with(|| a.path.cmp(&b.path))
    });
    Ok(out)
}
/// Called by the settings command only. Paths in file contents never reach this function.
pub fn authorize(project: &mut Project) -> Result<(), String> {
    if project.context.version > 1 {
        return Err("Unsupported context version".into());
    }
    if project.context.applications.len() > 16 {
        return Err("At most 16 application associations are allowed".into());
    }
    for app in &mut project.context.applications {
        let path = Path::new(app)
            .canonicalize()
            .map_err(|_| "Choose an existing application file")?;
        if !path.is_file() {
            return Err("Choose an application file".into());
        }
        *app = path.to_string_lossy().to_string();
    }
    project.context.applications.sort();
    project.context.applications.dedup();
    if !project.context.sources.is_empty() {
        let root = Path::new(
            project
                .folder
                .as_deref()
                .ok_or("Choose a project folder first")?,
        )
        .canonicalize()
        .map_err(|_| "Project folder unavailable")?;
        project.folder = Some(root.to_string_lossy().to_string());
        for source in &mut project.context.sources {
            source.path = Path::new(&source.path)
                .canonicalize()
                .map_err(|_| "Source unavailable")?
                .to_string_lossy()
                .to_string();
        }
        project.context.sources = read_sources(&root, &project.context.sources)?;
    }
    project.context.version = 1;
    Ok(())
}

pub fn start(app: AppHandle) {
    std::thread::spawn(move || loop {
        let cfg = crate::config::load(&app);
        let state = app.state::<crate::state::AppState>();
        let mut next = HashMap::new();
        for project in &cfg.projects {
            if project.context.sources.is_empty() {
                continue;
            }
            let identity = key(project);
            let previous = state.context_sources.lock().ok().and_then(|cache| {
                cache
                    .get(&project.id)
                    .filter(|entry| entry.key == identity)
                    .cloned()
            });
            let metadata: Vec<_> = project
                .context
                .sources
                .iter()
                .map(|source| {
                    std::fs::metadata(&source.path)
                        .map(|m| (m.len(), m.modified().ok()))
                        .unwrap_or((0, None))
                })
                .collect();
            // Metadata avoids full rereads at idle. A periodic hash also catches tools that
            // preserve timestamps; permission and symlink boundaries are rechecked on every read.
            if let Some(previous) = previous.as_ref().filter(|previous| {
                previous.metadata == metadata
                    && previous.checked.elapsed() < std::time::Duration::from_secs(30)
            }) {
                next.insert(project.id.clone(), previous.clone());
                continue;
            }
            let fallback = previous
                .as_ref()
                .map(|c| c.sources.clone())
                .unwrap_or_else(|| project.context.sources.clone());
            let (sources, status) = match project
                .folder
                .as_deref()
                .ok_or_else(|| "Project folder unavailable".to_string())
                .and_then(|root| read_sources(Path::new(root), &fallback))
            {
                Ok(sources) => {
                    let status = if sources.iter().any(|s| s.excluded_lines > 0) {
                        "Updated; unrelated or operational content excluded"
                    } else {
                        "Up to date"
                    };
                    (sources, status.to_string())
                }
                Err(error) => (fallback, format!("{}; using last approved context", error)),
            };
            next.insert(
                project.id.clone(),
                Cached {
                    key: identity,
                    sources,
                    status,
                    metadata,
                    checked: std::time::Instant::now(),
                },
            );
        }
        if let Ok(mut cache) = state.context_sources.lock() {
            *cache = next;
        }
        std::thread::sleep(std::time::Duration::from_secs(2));
    });
}

pub fn resolved_sources(
    state: &crate::state::AppState,
    project: &Project,
    scope: Option<&Path>,
) -> (Vec<Source>, String) {
    let cached = state.context_sources.lock().ok().and_then(|cache| {
        cache
            .get(&project.id)
            .filter(|entry| entry.key == key(project))
            .cloned()
    });
    let (sources, status) = cached.map(|c| (c.sources, c.status)).unwrap_or_else(|| {
        (
            project.context.sources.clone(),
            if project.context.sources.is_empty() {
                "Saved preferences"
            } else {
                "Update pending"
            }
            .into(),
        )
    });
    let root = project.folder.as_deref().map(Path::new);
    let sources = sources
        .into_iter()
        .filter(|source| {
            let parent = Path::new(&source.path).parent();
            parent == root
                || scope.is_some_and(|scope| parent.is_some_and(|parent| scope.starts_with(parent)))
        })
        .collect();
    (sources, status)
}
pub fn delivery(state: &crate::state::AppState, run_id: u64, delivery: Delivery) {
    if let Ok(mut slot) = state.resolved_context.lock() {
        if let Some(snapshot) = slot.as_mut() {
            snapshot.update_delivery(run_id, delivery);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn authorized_read_never_follows_new_imports_and_preserves_exclusions() {
        let root = std::env::temp_dir().join(format!(
            "ember-context-{}-{}",
            std::process::id(),
            crate::now_ms()
        ));
        std::fs::create_dir(&root).unwrap();
        let file = root.join("AGENTS.md");
        std::fs::write(
            &file,
            "# Technical context\nStack: Rust\n@new.md\nRun commands",
        )
        .unwrap();
        let canonical = file.canonicalize().unwrap().to_string_lossy().to_string();
        let approved = vec![Source {
            path: canonical,
            ..Default::default()
        }];
        let first = read_sources(&root, &approved).unwrap();
        assert!(first[0].text.contains("Rust"));
        assert!(!first[0].text.contains("Run"));
        std::fs::write(root.join("new.md"), "# Technical context\nStack: hidden").unwrap();
        assert_eq!(read_sources(&root, &approved).unwrap(), first);
        std::fs::write(&file, "# Technical context\nStack: Tauri").unwrap();
        assert_ne!(
            read_sources(&root, &approved).unwrap()[0].fingerprint,
            first[0].fingerprint
        );
        std::fs::remove_file(&file).unwrap();
        assert!(read_sources(&root, &approved).is_err());
        std::fs::remove_file(root.join("new.md")).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
    #[test]
    fn scope_uses_root_then_active_ancestors_only() {
        let state = crate::state::AppState::new();
        let project: Project = serde_json::from_str(r#"{"id":"a","name":"A","folder":"/root","context":{"sources":[{"path":"/root/AGENTS.md","text":"Tone: direct"},{"path":"/root/lib/CLAUDE.md","text":"Stack: Rust"},{"path":"/root/other/AGENTS.md","text":"Stack: excluded"}]}}"#).unwrap();
        let (root, _) = resolved_sources(&state, &project, None);
        assert_eq!(root.len(), 1);
        let (nested, _) = resolved_sources(&state, &project, Some(Path::new("/root/lib/file.rs")));
        assert_eq!(nested.len(), 2);
        assert!(!nested.iter().any(|s| s.text.contains("excluded")));
    }
    #[test]
    fn failed_extraction_cannot_replace_a_useful_snapshot() {
        let root = std::env::temp_dir().join(format!(
            "ember-context-failure-{}-{}",
            std::process::id(),
            crate::now_ms()
        ));
        std::fs::create_dir(&root).unwrap();
        let file = root.join("AGENTS.md");
        std::fs::write(&file, "# Operations\nRun commands").unwrap();
        let source = Source {
            path: file.canonicalize().unwrap().to_string_lossy().into(),
            text: "Tone: approved".into(),
            ..Default::default()
        };
        assert!(read_sources(&root, &[source]).is_err());
        std::fs::remove_file(file).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
}
