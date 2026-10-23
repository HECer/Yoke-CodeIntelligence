//! Host-selected, flat YAML discovery with a replayable ordinary pinned selection.
use super::{FileSelection, Loaded, Selection};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fs::{self, File},
    path::{Path, PathBuf},
};
use yoke::{
    capture::Deadline,
    session_config::{self, Source, Target},
};

const MAX_DIRECTORIES: usize = 32;
const MAX_ENTRIES: usize = 1024;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DiscoverySelection {
    host_profile: yoke::project_session::HostProfile,
    #[serde(default)]
    template_context: Option<super::templates::TemplateContext>,
    #[serde(default)]
    system_prompt: Option<String>,
    #[serde(default)]
    memory_embeddings: Option<Vec<String>>,
    #[serde(default)]
    files: Vec<Candidate>,
    #[serde(default)]
    catalogs: Vec<CatalogCandidate>,
    #[serde(default)]
    directories: Vec<DirectorySelection>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Candidate {
    path: PathBuf,
    target: Target,
    #[serde(default)]
    sha256: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CatalogCandidate {
    path: PathBuf,
    #[serde(default)]
    sha256: Option<String>,
}
fn admit_catalog(candidates: &mut Vec<CatalogCandidate>, value: CatalogCandidate) -> Result<()> {
    ensure!(
        candidates.len() < session_config::MAX_CATALOG_FILES,
        "session_config_catalog_file_capacity"
    );
    if let Some(pin) = &value.sha256 {
        ensure!(
            yoke::model::valid_id(pin),
            "session_config_catalog_source_changed"
        );
    }
    candidates.push(value);
    Ok(())
}
#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum Kind {
    Contexts,
    Modes,
    PromptTemplates,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DirectorySelection {
    path: PathBuf,
    kind: Kind,
}
struct DirectoryBinding {
    path: PathBuf,
    names: Vec<String>,
    _held: Vec<File>,
}
pub(super) struct Prepared {
    pub(super) value: Value,
    loaded: Vec<Loaded>,
    directories: Vec<DirectoryBinding>,
}
impl Prepared {
    pub(super) fn recheck(&self, deadline: &Deadline) -> Result<()> {
        for directory in &self.directories {
            let (names, _, _held) = scan(&directory.path, deadline)?;
            ensure!(
                names == directory.names,
                "session_config_discovery_directory_changed"
            );
        }
        super::recheck(&self.loaded, deadline)
    }
}
fn scan(path: &Path, deadline: &Deadline) -> Result<(Vec<String>, usize, Vec<File>)> {
    let held = super::super::meaning_archive_cli::directory_guards(path, deadline)
        .context("session_config_discovery_directory_refused")?;
    let entries = fs::read_dir(path).context("session_config_discovery_directory_refused")?;
    let mut names = Vec::new();
    let mut count = 0usize;
    for entry in entries {
        deadline.check()?;
        count += 1;
        ensure!(
            count <= MAX_ENTRIES,
            "session_config_discovery_entry_capacity"
        );
        let entry = entry.context("session_config_discovery_directory_refused")?;
        let name = entry.file_name();
        let yaml = Path::new(&name)
            .extension()
            .and_then(|s| s.to_str())
            .is_some_and(|s| s.eq_ignore_ascii_case("yml") || s.eq_ignore_ascii_case("yaml"));
        if yaml {
            ensure!(
                names.len() < session_config::MAX_FILES,
                "session_config_file_capacity_exceeded"
            );
            names.push(name.into_string().map_err(|_| {
                anyhow::anyhow!("session_config_discovery_filename_encoding_unavailable")
            })?);
        }
    }
    names.sort();
    Ok((names, count, held))
}
fn admit(
    candidates: &mut Vec<Candidate>,
    seen: &mut BTreeSet<String>,
    value: Candidate,
) -> Result<()> {
    ensure!(
        candidates.len() < session_config::MAX_FILES,
        "session_config_file_capacity_exceeded"
    );
    ensure!(
        seen.insert(serde_json::to_string(&value.target)?),
        "session_config_discovery_target_ambiguous"
    );
    if let Some(pin) = &value.sha256 {
        ensure!(yoke::model::valid_id(pin), "session_config_source_changed");
    }
    candidates.push(value);
    Ok(())
}
#[cfg(test)]
pub(super) fn prepare(path: &Path, deadline: &Deadline) -> Result<Prepared> {
    prepare_with_template_permission(path, deadline, false)
}
#[cfg(test)]
pub(super) fn prepare_with_template_permission(
    path: &Path,
    deadline: &Deadline,
    allow_templates: bool,
) -> Result<Prepared> {
    prepare_with_template_mode(path, deadline, allow_templates, false)
}
pub(super) fn prepare_with_template_mode(
    path: &Path,
    deadline: &Deadline,
    allow_templates: bool,
    defer_templates: bool,
) -> Result<Prepared> {
    let selected = super::read(path, 128000, deadline)?;
    let mut input: DiscoverySelection = yoke::json::parse(&selected.bytes, 128000)
        .and_then(yoke::json::decode)
        .map_err(|_| anyhow::anyhow!("session_config_discovery_selection_invalid"))?;
    ensure!(
        !input.files.is_empty()
            || !input.directories.is_empty()
            || !input.catalogs.is_empty()
            || input
                .template_context
                .as_ref()
                .is_some_and(|c| !c.catalogs.is_empty()),
        "session_config_discovery_selection_required"
    );
    ensure!(
        input.directories.len() <= MAX_DIRECTORIES,
        "session_config_discovery_directory_capacity"
    );
    ensure!(
        input.files.len() <= session_config::MAX_FILES,
        "session_config_file_capacity_exceeded"
    );
    let catalog_selectors = !input.catalogs.is_empty()
        || input
            .directories
            .iter()
            .any(|v| matches!(v.kind, Kind::PromptTemplates));
    ensure!(
        !catalog_selectors || allow_templates,
        "session_config_template_permission_required"
    );
    if catalog_selectors && input.template_context.is_none() {
        input.template_context = Some(super::templates::TemplateContext::default());
    }
    super::templates::admit(input.template_context.as_ref(), allow_templates, deadline)?;
    let deferred = super::templates::deferred_context(
        input.template_context.as_ref(),
        input.system_prompt.as_deref(),
        input.memory_embeddings.as_ref(),
        defer_templates,
        allow_templates,
        deadline,
    )?;
    let mut catalog_candidates = Vec::new();
    if let Some(context) = &input.template_context {
        for file in &context.catalogs {
            admit_catalog(
                &mut catalog_candidates,
                CatalogCandidate {
                    path: file.path.clone(),
                    sha256: Some(file.sha256.clone()),
                },
            )?;
        }
    }
    for file in input.catalogs {
        admit_catalog(&mut catalog_candidates, file)?;
    }

    let mut candidates = Vec::new();
    let mut seen = BTreeSet::new();
    for candidate in input.files {
        admit(&mut candidates, &mut seen, candidate)?;
    }
    let mut directories = Vec::new();
    let mut receipts = Vec::new();
    for directory in input.directories {
        let (names, count, held) = scan(&directory.path, deadline)?;
        for name in &names {
            if matches!(directory.kind, Kind::PromptTemplates) {
                admit_catalog(
                    &mut catalog_candidates,
                    CatalogCandidate {
                        path: directory.path.join(name),
                        sha256: None,
                    },
                )?;
                continue;
            }
            let stem = Path::new(name)
                .file_stem()
                .and_then(|s| s.to_str())
                .filter(|s| !s.is_empty())
                .ok_or_else(|| anyhow::anyhow!("session_config_discovery_filename_invalid"))?;
            let target = match directory.kind {
                Kind::Contexts => Target::Context { name: stem.into() },
                Kind::Modes => Target::Mode { name: stem.into() },
                Kind::PromptTemplates => unreachable!("catalogs admitted separately"),
            };
            admit(
                &mut candidates,
                &mut seen,
                Candidate {
                    path: directory.path.join(name),
                    target,
                    sha256: None,
                },
            )?;
        }
        receipts.push(json!({"path":directory.path,"kind":directory.kind,"yamlFiles":names,"inspectedEntries":count,"membershipSha256":yoke::model::hash(serde_json::to_vec(&names)?),"recursive":false}));
        directories.push(DirectoryBinding {
            path: directory.path,
            names,
            _held: held,
        });
    }
    // No selected YAML payload is read until every directory, candidate count
    // and target uniqueness check passes. Non-YAML entries spend scan budget.
    let mut loaded = Vec::new();
    let mut files = Vec::new();
    let mut total = 0usize;
    for candidate in candidates {
        let value = super::read(&candidate.path, session_config::MAX_FILE_BYTES, deadline)
            .context("session_config_discovery_source_refused")?;
        total = total
            .checked_add(value.bytes.len())
            .context("session_config_source_capacity_exceeded")?;
        ensure!(
            total <= session_config::MAX_TOTAL_BYTES,
            "session_config_source_capacity_exceeded"
        );
        let actual_hash = yoke::model::hash(&value.bytes);
        if let Some(pin) = candidate.sha256 {
            ensure!(pin == actual_hash, "session_config_source_changed");
        }
        files.push(FileSelection {
            path: candidate.path,
            target: candidate.target,
            sha256: actual_hash,
        });
        loaded.push(value);
    }
    let mut catalog_loaded = Vec::new();
    if let Some(context) = &mut input.template_context {
        context.catalogs.clear();
        let mut combined = total;
        for candidate in catalog_candidates {
            let value = super::read(&candidate.path, session_config::MAX_FILE_BYTES, deadline)
                .context("session_config_discovery_source_refused")?;
            combined = combined
                .checked_add(value.bytes.len())
                .context("session_config_source_capacity_exceeded")?;
            ensure!(
                combined <= session_config::MAX_TOTAL_BYTES,
                "session_config_source_capacity_exceeded"
            );
            let hash = yoke::model::hash(&value.bytes);
            if let Some(pin) = candidate.sha256 {
                ensure!(pin == hash, "session_config_catalog_source_changed");
            }
            context.catalogs.push(super::templates::CatalogFile {
                path: candidate.path,
                sha256: hash,
            });
            catalog_loaded.push(value);
        }
    }
    let selection = Selection {
        host_profile: input.host_profile,
        files,
        template_context: input.template_context,
        system_prompt: input.system_prompt,
        memory_embeddings: input.memory_embeddings,
    };
    if catalog_selectors
        || selection
            .template_context
            .as_ref()
            .is_some_and(|c| !c.catalogs.is_empty())
    {
        // A discovery receipt promises replay through the ordinary 128k selector.
        yoke::json::encode_bounded(&selection, 128000, deadline)?;
    }
    let sources: Vec<_> = selection
        .files
        .iter()
        .zip(&loaded)
        .map(|(file, data)| Source {
            target: &file.target,
            expected_sha256: &file.sha256,
            bytes: &data.bytes,
        })
        .collect();
    let context = selection
        .template_context
        .as_ref()
        .map(|c| super::templates::prepare_loaded_context(c, total, deadline, Some(catalog_loaded)))
        .transpose()?;
    let deferred = deferred.map(|mut config| {
        config.context = selection
            .template_context
            .as_ref()
            .expect("Admitted template context")
            .clone();
        config
    });
    let projection = super::templates::project(
        &selection.host_profile,
        &sources,
        context.as_ref(),
        deferred.as_ref(),
        deadline,
    )?;
    let value = json!({"schema":"native_session_config_discovery_v1","selection":selection,"projection":super::templates::plan(&projection,context.as_ref()),"directories":receipts,"discoveryComplete":true,"sourceBytesRewritten":false,"templatesEvaluated":projection.plan()["templatesEvaluated"],"sourceExecuted":false,"storeOpened":false,"networkRequests":0,"modelCalls":0,"complete":false});
    if let Some(context) = context {
        loaded.extend(context.into_loaded());
    }
    loaded.push(selected);
    Ok(Prepared {
        value,
        loaded,
        directories,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let root = tempfile::tempdir().unwrap();
        let modes = root.path().join("modes");
        fs::create_dir(&modes).unwrap();
        fs::write(modes.join("owned.yml"), b"prompt: owned\n").unwrap();
        let selection = root.path().join("discovery.json");
        fs::write(&selection,serde_json::to_vec(&json!({"hostProfile":{"context":"desktop-app","steering":{}},"directories":[{"path":modes,"kind":"modes"}]})).unwrap()).unwrap();
        (root, selection, modes)
    }
    #[test]
    fn a_new_yaml_member_after_preparation_refuses_before_publication() {
        let (_root, selection, modes) = fixture();
        let deadline = Deadline::new(30000);
        let prepared = prepare(&selection, &deadline).unwrap();
        fs::write(modes.join("new.yml"), b"prompt: later\n").unwrap();
        let error = prepared.recheck(&deadline).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("session_config_discovery_directory_changed")
        );
    }
    #[test]
    fn changed_source_is_either_write_locked_or_refused_before_publication() {
        let (_root, selection, modes) = fixture();
        let deadline = Deadline::new(30000);
        let prepared = prepare(&selection, &deadline).unwrap();
        match fs::write(modes.join("owned.yml"), b"prompt: changed\n") {
            Ok(()) => assert!(prepared.recheck(&deadline).is_err()),
            Err(error) => {
                #[cfg(windows)]
                assert_eq!(error.raw_os_error(), Some(32), "{error:?}");
                #[cfg(not(windows))]
                assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
                assert_eq!(
                    fs::read(modes.join("owned.yml")).unwrap(),
                    b"prompt: owned\n"
                );
                prepared.recheck(&deadline).unwrap();
            }
        }
    }
}

#[cfg(test)]
mod catalog_tests {
    use super::*;
    #[test]
    fn selected_catalog_directory_membership_is_rechecked_before_delivery() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("catalogs");
        fs::create_dir(&dir).unwrap();
        fs::write(dir.join("owned.yml"), b"prompts: {entry: owned}\n").unwrap();
        let selection = root.path().join("selection.json");
        fs::write(&selection,serde_json::to_vec(&json!({"hostProfile":{"context":"desktop-app","steering":{}},"directories":[{"path":dir,"kind":"prompt_templates"}]})).unwrap()).unwrap();
        let d = Deadline::new(30000);
        let prepared = prepare_with_template_permission(&selection, &d, true).unwrap();
        fs::write(dir.join("later.yml"), b"prompts: {}\n").unwrap();
        assert!(
            prepared
                .recheck(&d)
                .unwrap_err()
                .to_string()
                .contains("session_config_discovery_directory_changed")
        );
    }
    #[test]
    fn selected_catalog_payload_stays_guarded_through_publication() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("owned.yml");
        fs::write(&source, b"prompts: {entry: owned}\n").unwrap();
        let selection = root.path().join("selection.json");
        fs::write(&selection,serde_json::to_vec(&json!({"hostProfile":{"context":"desktop-app","steering":{}},"catalogs":[{"path":source}]})).unwrap()).unwrap();
        let d = Deadline::new(30000);
        let prepared = prepare_with_template_permission(&selection, &d, true).unwrap();
        match fs::write(&source, b"prompts: {entry: changed}\n") {
            Ok(()) => assert!(prepared.recheck(&d).is_err()),
            Err(error) => {
                #[cfg(windows)]
                assert_eq!(error.raw_os_error(), Some(32));
                #[cfg(not(windows))]
                assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
                prepared.recheck(&d).unwrap();
            }
        }
    }
}
