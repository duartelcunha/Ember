//! Resolve reviewed profile snapshots. Ambient files are never read during refinement.
use ember_core::model::{Profile, ProfileSource};
use ember_core::profile_import::Source;
use std::io::Read;
use std::path::Path;

// English defaults avoid biasing multilingual input toward the profile's language.
// Spaces before continuation escapes keep adjacent words separated in the prompt.
pub const DEFAULT_PROFILE: &str = "\
Write with clarity and precision. Professional but direct tone. Short sentences. Avoid \
unnecessary jargon and filler. When context is missing, keep the request generic or use \
placeholders instead of inventing details or asking the user for clarification.";

pub struct Resolved {
    pub profile: Profile,
    pub path: Option<String>,
}

/// Only explicit, reviewed text is eligible. Source paths are provenance, not live imports.
pub fn resolve(override_text: Option<&str>, sources: &[Source]) -> Resolved {
    if let Some(text) = override_text.filter(|text| !text.trim().is_empty()) {
        return Resolved {
            profile: Profile {
                text: text.to_owned(),
                source: ProfileSource::UserEdited,
            },
            path: sources.first().map(|source| source.path.clone()),
        };
    }
    Resolved {
        profile: Profile {
            text: DEFAULT_PROFILE.to_owned(),
            source: ProfileSource::Default,
        },
        path: None,
    }
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Import {
    pub text: String,
    pub sources: Vec<Source>,
    pub warnings: Vec<String>,
}

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, LazyLock,
};
static IMPORT_BUSY: LazyLock<Arc<AtomicBool>> = LazyLock::new(|| Arc::new(AtomicBool::new(false)));
struct ImportLease(Arc<AtomicBool>);
impl Drop for ImportLease {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

async fn run_import<F>(
    gate: Arc<AtomicBool>,
    timeout: std::time::Duration,
    work: F,
) -> Result<Import, String>
where
    F: FnOnce() -> Result<Import, String> + Send + 'static,
{
    if gate
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err("A previous profile read is still finishing. Try again shortly.".into());
    }
    let lease = ImportLease(gate);
    let worker = tokio::task::spawn_blocking(move || {
        let _lease = lease;
        work()
    });
    // A timed-out filesystem call retains the lease until it actually returns. Repeated
    // clicks cannot create an unbounded pool of blocked filesystem workers.
    match tokio::time::timeout(timeout, worker).await {
        Ok(Ok(result)) => result,
        Ok(Err(_)) => Err("Profile import could not be completed.".into()),
        Err(_) => Err(
            "Profile read timed out. The local read may still be finishing; no profile was saved."
                .into(),
        ),
    }
}

pub async fn import_selected(paths: Vec<String>) -> Result<Import, String> {
    run_import(
        Arc::clone(&IMPORT_BUSY),
        std::time::Duration::from_secs(5),
        move || import_files(&paths),
    )
    .await
}

/// Selecting a file permits a bounded local read, not automatic prompt inclusion.
pub fn import_files(paths: &[String]) -> Result<Import, String> {
    use sha2::{Digest, Sha256};
    if paths.is_empty() || paths.len() > 8 {
        return Err("Choose between one and eight profile files".into());
    }
    let mut sources = Vec::new();
    let mut extracted = Vec::new();
    let mut warnings = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut total = 0;
    for path in paths {
        let extension = Path::new(path)
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if path.len() > 4096
            || path.contains('\0')
            || !matches!(extension.as_str(), "md" | "markdown" | "txt")
        {
            return Err("Choose a Markdown or plain-text profile source.".into());
        }
        let path = Path::new(path)
            .canonicalize()
            .map_err(|_| "A selected source could not be resolved")?;
        if !seen.insert(path.clone()) {
            continue;
        }
        let text = read_bounded(&path, 64 * 1024)?;
        total += text.len();
        if total > 256 * 1024 {
            return Err("Selected profiles exceed the combined read limit (256 KiB)".into());
        }
        let value = ember_core::profile_import::extract(&text);
        if value.excluded_lines > 0 {
            warnings.push(format!("{} nonempty lines were excluded from a selected source because they were outside recognized preference/fact sections or contained operational content.", value.excluded_lines));
        }
        if value.secrets_removed {
            warnings.push("Secret-like content was removed from a selected source before preparing this local draft.".into());
        }
        sources.push(Source {
            path: path.to_string_lossy().into_owned(),
            fingerprint: format!("{:x}", Sha256::digest(text.as_bytes())),
            bytes: text.len(),
        });
        extracted.push(value);
    }
    let text = ember_core::profile_import::compose(&extracted);
    if text.is_empty() {
        warnings.push("No recognized writing preferences or technical facts were found. No raw source content will be used. Write the preferences you want explicitly.".into());
    }
    warnings.push("Review this local extraction before saving. It is conservative and can omit useful rules. Source order is preserved; resolve contradictions in the draft. Files are snapshots and are not reloaded automatically.".into());
    Ok(Import {
        text,
        sources,
        warnings,
    })
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn absent_override_uses_defaults_without_reading_an_ambient_file() {
        let source = Source {
            path: "/nonexistent/AGENTS.md".into(),
            fingerprint: "a".repeat(64),
            bytes: 10,
        };
        assert_eq!(
            resolve(None, &[source]).profile.source,
            ProfileSource::Default
        );
        assert_eq!(
            resolve(Some("  "), &[]).profile.source,
            ProfileSource::Default
        );
    }
    #[test]
    fn reviewed_snapshot_is_resolved_without_reopening_its_source() {
        let source = Source {
            path: "/nonexistent/AGENTS.md".into(),
            fingerprint: "a".repeat(64),
            bytes: 10,
        };
        let resolved = resolve(Some("Tone: direct"), &[source]);
        assert_eq!(resolved.profile.text, "Tone: direct");
        assert_eq!(resolved.path.as_deref(), Some("/nonexistent/AGENTS.md"));
    }
    #[test]
    fn invalid_import_count_and_missing_files_fail_without_raw_fallback() {
        assert!(import_files(&[]).is_err());
        assert!(import_files(&vec!["missing".into(); 9]).is_err());
        assert!(import_files(&["nonexistent-profile-fixture-ember.md".into()]).is_err());
    }
    #[tokio::test]
    async fn timed_out_read_retains_its_lease_until_the_worker_finishes() {
        let gate = Arc::new(AtomicBool::new(false));
        let (release, waiting) = std::sync::mpsc::channel();
        let result = run_import(
            gate.clone(),
            std::time::Duration::from_millis(20),
            move || {
                let _ = waiting.recv_timeout(std::time::Duration::from_secs(2));
                Ok(Import {
                    text: String::new(),
                    sources: vec![],
                    warnings: vec![],
                })
            },
        )
        .await;
        assert!(result.is_err_and(|error| error.contains("timed out")));
        assert!(gate.load(Ordering::SeqCst));
        let second = run_import(gate.clone(), std::time::Duration::from_millis(20), || {
            panic!("A second worker must not be spawned")
        })
        .await;
        assert!(second.is_err_and(|error| error.contains("previous profile read")));
        release.send(()).unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while gate.load(Ordering::SeqCst) {
                tokio::time::sleep(std::time::Duration::from_millis(1)).await;
            }
        })
        .await
        .unwrap();
    }

    #[test]
    fn explicit_import_deduplicates_sources_and_fingerprints_the_original_snapshot() {
        use sha2::{Digest, Sha256};
        let path = std::env::temp_dir().join(format!(
            "ember-import-{}-{}.md",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let original = "# Writing style\nClear prose.\n# Workflow\nRun tests.\n";
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        file.write_all(original.as_bytes()).unwrap();
        drop(file);
        let paths = vec![path.to_string_lossy().into_owned(); 2];
        let imported = import_files(&paths).unwrap();
        std::fs::remove_file(path).unwrap();
        assert_eq!(imported.text, "Writing preferences:\nClear prose.");
        assert_eq!(imported.sources.len(), 1);
        assert_eq!(
            imported.sources[0].fingerprint,
            format!("{:x}", Sha256::digest(original))
        );
        assert_eq!(imported.sources[0].bytes, original.len());
        assert!(imported
            .warnings
            .iter()
            .any(|warning| warning.contains("excluded")));
    }

    #[test]
    fn source_reads_reject_oversized_and_invalid_utf8_content() {
        use std::io::Write;
        let name = format!(
            "ember-profile-{}-{}.md",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let path = std::env::temp_dir().join(name);
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        file.write_all(&[b'a', b'b', b'c', b'd', 255]).unwrap();
        drop(file);
        assert!(read_bounded(&path, 4).is_err_and(|error| error.contains("read limit")));
        assert!(read_bounded(&path, 10).is_err_and(|error| error.contains("UTF-8")));
        std::fs::remove_file(path).unwrap();
    }
}
