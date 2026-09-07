//! Window titles can select a registered project, never authorize file reads.
use ember_core::{project, projects::Project};
use std::path::Path;

pub fn resolve<'a>(
    title: &str,
    home: Option<&Path>,
    projects: &'a [Project],
) -> Option<&'a Project> {
    let candidate = project::extract_path(title, home)?;
    let candidate = candidate.canonicalize().ok()?;
    projects
        .iter()
        .filter_map(|p| {
            let root = Path::new(p.folder.as_deref()?).canonicalize().ok()?;
            candidate
                .starts_with(&root)
                .then_some((root.components().count(), p))
        })
        .max_by_key(|(depth, _)| *depth)
        .map(|(_, p)| p)
}

pub struct Signal<'a> {
    pub run_id: u64,
    pub title: Option<&'a str>,
    pub application: Option<&'a str>,
}
