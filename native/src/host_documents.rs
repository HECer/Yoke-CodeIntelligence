//! Exact host-selected configuration documents. Plans are inert data, not write authority.
use super::io;
use crate::{
    capture::Deadline,
    model::{hash, identity, valid_id},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

const MAX_DOCUMENTS: usize = 16;
const MAX_BYTES: usize = 64_000;
const MAX_PLAN: usize = 512_000;
#[path = "host_documents_journal.rs"]
mod journal;
pub use journal::{
    Approval, BindingMigrationApproval, BindingMigrationAuthority, BindingMigrationPlan,
    BindingMigrationReceipt, BindingMigrationReview, HistoryArchiveAccess, HistoryArchiveApproval,
    HistoryArchivePlan, HistoryArchivePurgeAccess, HistoryArchivePurgeApproval,
    HistoryArchivePurgePermission, HistoryArchivePurgePlan, HistoryArchivePurgeReview,
    HistoryArchiveRecord, HistoryArchiveReview, HistoryPurgeFileWitness,
    HistoryPurgeRetainedWitness, JournalView, Receipt, RecoveryApproval, RecoveryDirection,
    WriteAuthority, apply_binding_migration, apply_history_archive, apply_history_archive_purge,
    inspect_binding_migration, inspect_history_archive, inspect_history_archive_purge,
    plan_binding_migration, plan_history_archive, plan_history_archive_purge,
    read_history_archive_record, recover_binding_migration, recover_history_archive,
    recover_history_archive_purge,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum Scope {
    Global {},
    Project { project: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DocumentSpec {
    pub id: String,
    pub label: String,
    pub path: PathBuf,
    pub scope: Scope,
    #[serde(default)]
    pub writable: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Config {
    pub documents: Vec<DocumentSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub journal_root: Option<PathBuf>,
    #[serde(default = "default_max_bytes")]
    pub max_document_bytes: usize,
}
fn default_max_bytes() -> usize {
    MAX_BYTES
}

/// Already registered analyzed roots, supplied by the owning host, never by a request.
pub struct ProjectRoot {
    pub name: String,
    pub path: PathBuf,
}
struct BoundDocument {
    spec: DocumentSpec,
    parent_path: PathBuf,
    parent_identity: String,
    source_project: Option<String>,
}
pub struct Access {
    documents: BTreeMap<String, BoundDocument>,
    max_bytes: usize,
    binding: String,
    journal: Option<(PathBuf, String)>,
    project_roots: Vec<(String, PathBuf, String)>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub document_id: String,
    pub label: String,
    pub path: String,
    pub scope: Scope,
    pub revision: String,
    pub sha256: String,
    pub security_sha256: String,
    pub byte_length: usize,
    pub content: String,
    pub host_write_selected: bool,
    pub source_project: Option<String>,
}
/// Carries a proposal and preconditions only. A caller cannot obtain filesystem
/// write authority by deserializing this type, changing it, or hashing it again.
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Plan {
    pub schema_version: u32,
    pub id: String,
    pub access_binding: String,
    pub document_id: String,
    pub before_revision: String,
    pub before_identity: String,
    pub before_hash: String,
    pub before_security_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_generation: Option<String>,
    pub after_hash: String,
    pub content: String,
    pub nonce: String,
}
fn path_key(path: &Path) -> Result<String> {
    let value = path
        .to_str()
        .context("Host document path must be Unicode")?;
    Ok(if cfg!(windows) {
        value.to_ascii_lowercase()
    } else {
        value.to_owned()
    })
}
fn inside(path: &Path, parent: &Path) -> Result<bool> {
    // Both values come from the same guarded Windows canonical-path mechanism.
    let path = PathBuf::from(path_key(path)?);
    let parent = PathBuf::from(path_key(parent)?);
    Ok(path.starts_with(parent))
}
impl Config {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.documents.is_empty() && self.documents.len() <= MAX_DOCUMENTS,
            "dashboard_document_selection_capacity"
        );
        ensure!(
            (1..=MAX_BYTES).contains(&self.max_document_bytes),
            "dashboard_document_byte_capacity"
        );
        let mut ids = BTreeSet::new();
        for spec in &self.documents {
            crate::project_session::label(&spec.id)?;
            ensure!(
                ids.insert(&spec.id) && !spec.label.is_empty() && spec.label.len() <= 200,
                "dashboard_document_identity_or_label_invalid"
            );
            ensure!(
                spec.path.is_absolute() && spec.path.file_name().is_some(),
                "dashboard_document_absolute_file_required"
            );
            if let Scope::Project { project } = &spec.scope {
                crate::project_session::label(project)?;
            }
        }
        if let Some(root) = &self.journal_root {
            ensure!(
                root.is_absolute(),
                "dashboard_document_absolute_journal_root_required"
            );
        }
        ensure!(
            serde_json::to_vec(self)?.len() <= 64_000,
            "dashboard_document_selection_byte_capacity"
        );
        Ok(())
    }
}
impl Access {
    /// Selects exact document paths. Missing targets remain unavailable to reads
    /// and planning, but retaining their parent binding permits later recovery.
    /// The journal root must already exist outside every analyzed root.
    pub fn open(config: &Config, roots: &[ProjectRoot], deadline: &Deadline) -> Result<Self> {
        deadline.check()?;
        config.validate()?;
        ensure!(
            cfg!(windows),
            "dashboard_guarded_host_documents_windows_only"
        );
        ensure!(
            !roots.is_empty() && roots.len() <= 101,
            "dashboard_document_project_capacity"
        );
        let mut root_names = BTreeSet::new();
        let mut root_paths = Vec::new();
        let mut root_guards = Vec::new();
        for root in roots {
            crate::project_session::label(&root.name)?;
            ensure!(
                root_names.insert(&root.name),
                "dashboard_document_duplicate_project"
            );
            let held = io::guarded_directory(&root.path, deadline)?;
            root_paths.push((root.name.clone(), held.path.clone(), held.identity.clone()));
            root_guards.push(held);
        }
        let journal = config
            .journal_root
            .as_ref()
            .map(|path| -> Result<_> {
                let held = io::guarded_directory(path, deadline)?;
                for (_, root, _) in &root_paths {
                    ensure!(
                        !inside(&held.path, root)?,
                        "dashboard_document_journal_inside_analyzed_root"
                    );
                }
                Ok((held.path.clone(), held.identity.clone()))
            })
            .transpose()?;
        let mut documents = BTreeMap::new();
        let mut targets = BTreeSet::new();
        let mut target_paths = BTreeSet::new();
        for spec in &config.documents {
            deadline.check()?;
            let requested_parent = spec
                .path
                .parent()
                .context("Host document parent required")?;
            let parent = io::guarded_directory(requested_parent, deadline)?;
            let parent_path = parent.path.clone();
            let file = io::read_file(&spec.path, config.max_document_bytes, false, deadline)?;
            let target_path = parent_path.join(
                spec.path
                    .file_name()
                    .context("Host document leaf required")?,
            );
            if let Some(file) = &file {
                std::str::from_utf8(&file.bytes).context("Selected host document is not UTF-8")?;
                ensure!(
                    file.path == target_path,
                    "Host document physical path changed"
                );
                ensure!(
                    targets.insert(file.identity.clone()),
                    "dashboard_document_duplicate_physical_file"
                );
            }
            ensure!(
                target_paths.insert(path_key(&target_path)?),
                "dashboard_document_duplicate_target_path"
            );
            if let Some((root, _)) = &journal {
                ensure!(
                    !inside(&target_path, root)?,
                    "dashboard_document_inside_recovery_namespace"
                );
            }
            let containing = root_paths
                .iter()
                .filter_map(|(name, root, _)| match inside(&target_path, root) {
                    Ok(true) => Some(Ok(name.as_str())),
                    Ok(false) => None,
                    Err(e) => Some(Err(e)),
                })
                .collect::<Result<Vec<_>>>()?;
            ensure!(
                containing.len() <= 1,
                "dashboard_document_overlapping_project_roots"
            );
            let source_project = containing.first().map(|s| s.to_string());
            match &spec.scope {
                Scope::Global {} => ensure!(
                    source_project.is_none(),
                    "dashboard_global_document_inside_analyzed_root"
                ),
                Scope::Project { project } => ensure!(
                    source_project.as_ref() == Some(project),
                    "dashboard_document_project_scope_mismatch"
                ),
            }
            documents.insert(
                spec.id.clone(),
                BoundDocument {
                    spec: spec.clone(),
                    parent_path,
                    parent_identity: parent.identity.clone(),
                    source_project,
                },
            );
        }
        let binding = identity(&(
            "dashboard_host_documents_v1",
            config,
            documents
                .iter()
                .map(|(id, d)| (id, &d.parent_path, &d.parent_identity, &d.source_project))
                .collect::<Vec<_>>(),
            &journal,
            &root_paths,
        ));
        Ok(Self {
            documents,
            max_bytes: config.max_document_bytes,
            binding,
            journal,
            project_roots: root_paths,
        })
    }
    pub fn binding(&self) -> &str {
        &self.binding
    }
    pub fn selections(&self) -> impl Iterator<Item = &DocumentSpec> {
        self.documents.values().map(|d| &d.spec)
    }
    pub fn selection(&self, id: &str) -> Result<&DocumentSpec> {
        self.documents
            .get(id)
            .map(|d| &d.spec)
            .context("Host document is not selected")
    }
    /// Derive a selected project's source-relative path from trusted, physically
    /// checked roots. Requests never supply a path or choose an Engine root.
    pub(crate) fn project_path(
        &self,
        id: &str,
        project: &str,
        root: &Path,
        deadline: &Deadline,
    ) -> Result<String> {
        let selected = self.selection(id)?;
        ensure!(
            matches!(&selected.scope, Scope::Project { project: name } if name == project),
            "Project document selection differs"
        );
        let (_, bound_path, bound_identity) = self
            .project_roots
            .iter()
            .find(|(name, _, _)| name == project)
            .context("Project document root unavailable")?;
        let held = io::guarded_directory(root, deadline)?;
        ensure!(
            held.path == *bound_path && held.identity == *bound_identity,
            "Project document Engine root changed"
        );
        let _guards = self.hold_bindings(id, deadline)?;
        let selected = &self.documents[id];
        let target = selected.parent_path.join(
            selected
                .spec
                .path
                .file_name()
                .context("Project document leaf required")?,
        );
        let relative = target.strip_prefix(bound_path)?;
        ensure!(
            relative
                .components()
                .all(|c| matches!(c, std::path::Component::Normal(_))),
            "Project document relative path invalid"
        );
        Ok(relative
            .to_str()
            .context("Project document path must be Unicode")?
            .replace('\\', "/"))
    }
    fn check_plan(&self, plan: &Plan, deadline: &Deadline) -> Result<&BoundDocument> {
        deadline.check()?;
        ensure!(
            plan.schema_version == 1
                && plan.access_binding == self.binding
                && [
                    &plan.id,
                    &plan.access_binding,
                    &plan.before_revision,
                    &plan.before_hash,
                    &plan.before_security_hash,
                    &plan.after_hash,
                    &plan.nonce
                ]
                .into_iter()
                .all(|id| valid_id(id))
                && !plan.before_identity.is_empty()
                && plan.before_identity.len() <= 128
                && plan.owner_generation.as_deref().is_none_or(valid_id)
                && plan.content.len() <= self.max_bytes
                && hash(plan.content.as_bytes()) == plan.after_hash,
            "Host document plan binding or content invalid"
        );
        ensure!(
            serde_json::to_vec(plan)?.len() <= MAX_PLAN,
            "Host document plan capacity exceeded"
        );
        let mut copy = plan.clone();
        copy.id.clear();
        ensure!(
            identity(&("dashboard_host_document_plan_v1", &copy)) == plan.id,
            "Host document plan identity invalid"
        );
        let selected = self
            .documents
            .get(&plan.document_id)
            .context("Host document is not selected")?;
        ensure!(
            selected.source_project.is_none() || plan.owner_generation.is_none(),
            "Project document plans cannot carry a global owner generation"
        );
        Ok(selected)
    }
    pub fn journal_binding(&self) -> Option<(&Path, &str)> {
        self.journal
            .as_ref()
            .map(|(path, id)| (path.as_path(), id.as_str()))
    }
    fn current(&self, id: &str, deadline: &Deadline) -> Result<(&BoundDocument, io::HeldFile)> {
        self.hold_current(id, false, deadline)
    }
    fn hold_current(
        &self,
        id: &str,
        delete_access: bool,
        deadline: &Deadline,
    ) -> Result<(&BoundDocument, io::HeldFile)> {
        let _guards = self.hold_bindings(id, deadline)?;
        let selected = &self.documents[id];
        let file = io::read_file(&selected.spec.path, self.max_bytes, delete_access, deadline)?
            .context("Selected host document unavailable")?;
        ensure!(
            file.path.parent() == Some(selected.parent_path.as_path()),
            "Host document parent binding changed"
        );
        Ok((selected, file))
    }
    fn hold_bindings(&self, id: &str, deadline: &Deadline) -> Result<Vec<io::GuardedDirectory>> {
        deadline.check()?;
        let selected = self
            .documents
            .get(id)
            .context("Host document is not selected")?;
        let mut root_guards = Vec::new();
        for (_, path, id) in &self.project_roots {
            let held = io::guarded_directory(path, deadline)?;
            ensure!(
                &held.identity == id,
                "Host document analyzed-root binding changed"
            );
            root_guards.push(held);
        }
        let parent = io::guarded_directory(&selected.parent_path, deadline)?;
        ensure!(
            parent.identity == selected.parent_identity,
            "Host document parent changed"
        );
        root_guards.push(parent);
        Ok(root_guards)
    }
    fn revision(
        &self,
        selected: &BoundDocument,
        file: &io::HeldFile,
        security_hash: &str,
    ) -> String {
        identity(&(
            "dashboard_host_document_revision_v1",
            &self.binding,
            &selected.spec.id,
            &selected.parent_identity,
            &file.identity,
            &file.hash,
            file.byte_length,
            security_hash,
        ))
    }
    pub fn read(&self, id: &str, deadline: &Deadline) -> Result<Snapshot> {
        let (selected, file) = self.current(id, deadline)?;
        let content = std::str::from_utf8(&file.bytes)
            .context("Selected host document is not UTF-8")?
            .to_owned();
        let security_hash = io::security_fingerprint(&file, deadline)?;
        deadline.check()?;
        Ok(Snapshot {
            document_id: id.to_owned(),
            label: selected.spec.label.clone(),
            path: file
                .path
                .to_str()
                .context("Host document path must be Unicode")?
                .to_owned(),
            scope: selected.spec.scope.clone(),
            revision: self.revision(selected, &file, &security_hash),
            sha256: file.hash.clone(),
            security_sha256: security_hash,
            byte_length: file.byte_length,
            content,
            host_write_selected: selected.spec.writable,
            source_project: selected.source_project.clone(),
        })
    }
    pub fn plan(
        &self,
        id: &str,
        expected_revision: &str,
        content: &str,
        deadline: &Deadline,
    ) -> Result<Plan> {
        ensure!(
            valid_id(expected_revision) && content.len() <= self.max_bytes,
            "dashboard_document_plan_revision_or_capacity"
        );
        let (selected, file) = self.current(id, deadline)?;
        let security_hash = io::security_fingerprint(&file, deadline)?;
        ensure!(
            self.revision(selected, &file, &security_hash) == expected_revision,
            "Stale host document revision"
        );
        let mut entropy = [0u8; 32];
        getrandom::fill(&mut entropy)
            .map_err(|_| anyhow::anyhow!("Host document transaction identity unavailable"))?;
        let mut plan = Plan {
            schema_version: 1,
            id: String::new(),
            access_binding: self.binding.clone(),
            document_id: id.to_owned(),
            before_revision: expected_revision.to_owned(),
            before_identity: file.identity.clone(),
            before_hash: file.hash.clone(),
            before_security_hash: security_hash,
            owner_generation: journal::owner_generation(selected, deadline)?,
            after_hash: hash(content.as_bytes()),
            content: content.to_owned(),
            nonce: hash(entropy),
        };
        ensure!(
            serde_json::to_vec(&plan)?.len() <= MAX_PLAN,
            "dashboard_document_plan_byte_capacity"
        );
        plan.id = identity(&("dashboard_host_document_plan_v1", &plan));
        deadline.check()?;
        Ok(plan)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[cfg(windows)]
    fn d() -> Deadline {
        Deadline::new(10_000)
    }
    fn config(path: &Path) -> Config {
        Config {
            documents: vec![DocumentSpec {
                id: "global".into(),
                label: "Owned configuration".into(),
                path: path.into(),
                scope: Scope::Global {},
                writable: true,
            }],
            journal_root: None,
            max_document_bytes: MAX_BYTES,
        }
    }
    #[test]
    fn selection_schema_rejects_request_authority_relative_paths_and_capacity() {
        assert!(
            serde_json::from_value::<Config>(json!({"documents":[],"command":"untrusted"}))
                .is_err()
        );
        assert!(
            serde_json::from_value::<Scope>(json!({"kind":"global","project":"untrusted"}))
                .is_err()
        );
        let mut selected = config(Path::new("relative.yaml"));
        assert!(selected.validate().is_err());
        selected.documents[0].path = if cfg!(windows) {
            PathBuf::from("C:\\owned.yaml")
        } else {
            PathBuf::from("/owned.yaml")
        };
        selected.validate().unwrap();
        selected.documents.push(selected.documents[0].clone());
        assert!(selected.validate().is_err());
        selected.documents.pop();
        selected.max_document_bytes = MAX_BYTES + 1;
        assert!(selected.validate().is_err());
        selected.max_document_bytes = 0;
        assert!(selected.validate().is_err());
        selected.max_document_bytes = MAX_BYTES;
        selected.documents[0].label = "x".repeat(201);
        assert!(selected.validate().is_err());
    }
    #[cfg(windows)]
    #[test]
    fn selected_unicode_bom_crlf_bytes_are_authoritative_and_plans_are_inert() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        std::fs::create_dir(&source).unwrap();
        let path = temp.path().join("host-π.yaml");
        let original = "\u{feff}# e\u{301}\r\nvalue: 🦀\r\n";
        std::fs::write(&path, original).unwrap();
        let roots = [ProjectRoot {
            name: "default".into(),
            path: source,
        }];
        let access = Access::open(&config(&path), &roots, &d()).unwrap();
        let loaded = access.read("global", &d()).unwrap();
        assert_eq!(loaded.content.as_bytes(), original.as_bytes());
        assert_eq!(loaded.sha256, hash(original));
        let plan = access
            .plan(
                "global",
                &loaded.revision,
                "command: <script>inert</script>\r\n",
                &d(),
            )
            .unwrap();
        assert_eq!(plan.before_hash, loaded.sha256);
        assert!(valid_id(&plan.id));
        assert_ne!(plan.after_hash, plan.before_hash);
        assert_eq!(plan.before_security_hash, loaded.security_sha256);
        let another = access
            .plan(
                "global",
                &loaded.revision,
                "command: <script>inert</script>\r\n",
                &d(),
            )
            .unwrap();
        assert_ne!(plan.id, another.id);
        assert_ne!(plan.nonce, another.nonce);
        assert_eq!(std::fs::read(&path).unwrap(), original.as_bytes());
        assert!(access.read("../unregistered.yaml", &d()).is_err());
        assert!(
            access
                .plan("global", &loaded.revision, &"x".repeat(MAX_BYTES + 1), &d())
                .is_err()
        );
        assert!(
            access
                .plan("global", "not-a-revision", "proposal", &d())
                .is_err()
        );
        assert!(!temp.path().join("inert-marker").exists());
    }
    #[cfg(windows)]
    #[test]
    fn project_and_journal_scopes_refuse_relabeling_or_analyzed_recovery_authority() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        let journals = temp.path().join("journals");
        std::fs::create_dir(&source).unwrap();
        std::fs::create_dir(&journals).unwrap();
        let path = source.join("project.yaml");
        std::fs::write(&path, b"value: true\n").unwrap();
        let roots = [ProjectRoot {
            name: "default".into(),
            path: source.clone(),
        }];
        let mut selected = config(&path);
        assert!(Access::open(&selected, &roots, &d()).is_err());
        selected.documents[0].scope = Scope::Project {
            project: "default".into(),
        };
        selected.journal_root = Some(journals.clone());
        let access = Access::open(&selected, &roots, &d()).unwrap();
        assert_eq!(
            access
                .read("global", &d())
                .unwrap()
                .source_project
                .as_deref(),
            Some("default")
        );
        selected.documents[0].scope = Scope::Project {
            project: "foreign".into(),
        };
        assert!(Access::open(&selected, &roots, &d()).is_err());
        selected.documents[0].scope = Scope::Project {
            project: "default".into(),
        };
        selected.journal_root = Some(source.clone());
        assert!(Access::open(&selected, &roots, &d()).is_err());
        let private = journals.join("journal.json");
        std::fs::write(&private, b"{}\n").unwrap();
        selected = config(&private);
        selected.journal_root = Some(journals);
        assert!(Access::open(&selected, &roots, &d()).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"value: true\n");
    }
    #[cfg(windows)]
    #[test]
    fn external_file_changes_and_hardlinks_never_reuse_a_loaded_revision() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        std::fs::create_dir(&source).unwrap();
        let path = temp.path().join("host.yaml");
        std::fs::write(&path, b"old\r\n").unwrap();
        let roots = [ProjectRoot {
            name: "default".into(),
            path: source,
        }];
        let access = Access::open(&config(&path), &roots, &d()).unwrap();
        let old = access.read("global", &d()).unwrap();
        std::fs::write(&path, b"concurrent\r\n").unwrap();
        assert!(
            access
                .plan("global", &old.revision, "lost update", &d())
                .is_err()
        );
        assert_eq!(std::fs::read(&path).unwrap(), b"concurrent\r\n");
        let alias = temp.path().join("alias.yaml");
        std::fs::hard_link(&path, &alias).unwrap();
        assert!(access.read("global", &d()).is_err());
        assert_eq!(std::fs::read(alias).unwrap(), b"concurrent\r\n");
    }
    #[cfg(windows)]
    #[test]
    fn replacing_analyzed_root_or_document_parent_invalidates_pinned_access() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        let host = temp.path().join("host");
        std::fs::create_dir(&source).unwrap();
        std::fs::create_dir(&host).unwrap();
        let path = host.join("host.yaml");
        std::fs::write(&path, b"owned\n").unwrap();
        let roots = [ProjectRoot {
            name: "default".into(),
            path: source.clone(),
        }];
        let access = Access::open(&config(&path), &roots, &d()).unwrap();
        std::fs::rename(&source, temp.path().join("original-source")).unwrap();
        std::fs::create_dir(&source).unwrap();
        assert!(access.read("global", &d()).is_err());
        let access = Access::open(&config(&path), &roots, &d()).unwrap();
        std::fs::rename(&host, temp.path().join("original-host")).unwrap();
        std::fs::create_dir(&host).unwrap();
        std::fs::write(&path, b"foreign\n").unwrap();
        assert!(access.read("global", &d()).is_err());
        assert_eq!(
            std::fs::read(temp.path().join("original-host/host.yaml")).unwrap(),
            b"owned\n"
        );
        assert_eq!(std::fs::read(&path).unwrap(), b"foreign\n");
    }
    #[cfg(windows)]
    #[test]
    fn selected_missing_file_is_unavailable_not_an_empty_complete_document() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        std::fs::create_dir(&source).unwrap();
        let path = temp.path().join("temporarily-absent.yaml");
        let roots = [ProjectRoot {
            name: "default".into(),
            path: source,
        }];
        let access = Access::open(&config(&path), &roots, &d()).unwrap();
        assert!(access.read("global", &d()).is_err());
        assert!(
            access
                .plan("global", &"a".repeat(64), "must not create", &d())
                .is_err()
        );
        assert!(!path.exists());
    }
}
