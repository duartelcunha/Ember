//! Reviewed sources are data snapshots, never permissions inherited from file contents.
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ProjectContext {
    pub version: u32,
    pub applications: Vec<String>,
    pub sources: Vec<Source>,
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Source {
    pub path: String,
    pub fingerprint: String,
    pub text: String,
    pub excluded_lines: usize,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Delivery {
    Prepared,
    Sending,
    Sent,
    Cached,
    Unconfirmed,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub run_id: u64,
    pub selection: String,
    pub project: Option<String>,
    pub project_id: Option<String>,
    pub reason: String,
    pub profile: String,
    pub profile_sources: Vec<crate::profile_import::Source>,
    pub profile_review_needed: bool,
    pub profile_invalid: bool,
    pub project_context: Option<String>,
    pub sources: Vec<Source>,
    pub source_status: String,
    pub fingerprint: String,
    pub config_revision: u64,
    pub delivery: Delivery,
}
impl Snapshot {
    pub fn update_delivery(&mut self, run_id: u64, delivery: Delivery) {
        if self.run_id == run_id
            && !(self.delivery == Delivery::Sent
                && matches!(delivery, Delivery::Sending | Delivery::Unconfirmed))
        {
            self.delivery = delivery;
        }
    }
}
/// An ambiguous app association never wins over the user's actual project path.
pub fn associated<'a>(
    projects: &'a [crate::projects::Project],
    application: &str,
) -> Option<&'a crate::projects::Project> {
    let mut matching = projects.iter().filter(|p| {
        p.context
            .applications
            .iter()
            .any(|app| app.eq_ignore_ascii_case(application))
    });
    let first = matching.next()?;
    matching.next().is_none().then_some(first)
}
/// Later, more specific explicit fields replace earlier fields with the same key.
pub fn compose(texts: &[String], manual: &str) -> String {
    let mut writing = Vec::<String>::new();
    let mut technical = Vec::<String>::new();
    fn merge(into: &mut Vec<String>, lines: Vec<String>) {
        for line in lines {
            if let Some((key, _)) = line.split_once(':') {
                let key = key.trim().trim_start_matches('-').trim().to_lowercase();
                into.retain(|old| {
                    old.split_once(':').is_none_or(|(k, _)| {
                        k.trim().trim_start_matches('-').trim().to_lowercase() != key
                    })
                });
            }
            if !into.contains(&line) {
                into.push(line);
            }
        }
    }
    for text in texts {
        let e = crate::profile_import::extract(text);
        merge(&mut writing, e.writing);
        merge(&mut technical, e.technical);
    }
    let mut out = crate::profile_import::compose(&[crate::profile_import::Extracted {
        writing,
        technical,
        ..Default::default()
    }]);
    let manual = safe_manual(manual);
    if !manual.is_empty() {
        out.push_str("\n\nUser preferences (take precedence over derived context):\n");
        out.push_str(&manual);
    }
    out.trim().to_owned()
}
pub fn safe_manual(text: &str) -> String {
    if needs_review(text) {
        crate::profile_import::compose(&[crate::profile_import::extract(text)])
    } else {
        crate::project::redact_secrets(text)
    }
}
pub fn needs_review(text: &str) -> bool {
    text.lines().any(|line| {
        crate::profile_import::operational(line)
            || line.trim_start().starts_with('@')
            || line.contains("EMBER_")
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn operational_profiles_have_no_raw_fallback() {
        assert_eq!(safe_manual("Run commands and read @memory"), "");
        let safe = safe_manual("# Writing preferences\nTone: concise\nRun sudo command\n# Technical context\nStack: Rust");
        assert!(safe.contains("concise") && safe.contains("Rust") && !safe.contains("sudo"));
    }
    #[test]
    fn specific_fields_replace_global_fields() {
        let combined = compose(
            &[
                "Writing preferences:\nTone: formal".into(),
                "Writing preferences:\nTone: direct".into(),
            ],
            "Language: Portuguese",
        );
        assert!(!combined.contains("formal"));
        assert!(combined.contains("direct"));
        assert!(combined.ends_with("Language: Portuguese"));
    }
    #[test]
    fn ambiguous_application_has_no_match() {
        let a: crate::projects::Project = serde_json::from_str(
            r#"{"id":"a","name":"A","context":{"applications":["/app/editor"]}}"#,
        )
        .unwrap();
        let mut b = a.clone();
        b.id = "b".into();
        assert!(associated(std::slice::from_ref(&a), "/app/editor").is_some());
        assert!(associated(&[a, b], "/app/editor").is_none());
    }
    #[test]
    fn delayed_delivery_does_not_update_a_newer_run() {
        let mut snapshot = Snapshot {
            run_id: 9,
            selection: "none".into(),
            project: None,
            project_id: None,
            reason: String::new(),
            profile: String::new(),
            profile_sources: vec![],
            profile_review_needed: false,
            profile_invalid: false,
            project_context: None,
            sources: vec![],
            source_status: String::new(),
            fingerprint: String::new(),
            config_revision: 0,
            delivery: Delivery::Prepared,
        };
        snapshot.update_delivery(8, Delivery::Sent);
        assert_eq!(snapshot.delivery, Delivery::Prepared);
        snapshot.update_delivery(9, Delivery::Sent);
        snapshot.update_delivery(9, Delivery::Unconfirmed);
        assert_eq!(snapshot.delivery, Delivery::Sent);
    }
}
