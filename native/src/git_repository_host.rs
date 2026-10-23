//! Invocation-owned Git capture authority; never decoded from project or MCP data.
use super::{
    HostGit,
    repository_preferences::Resolved,
    repository_scope::{self, Options},
};
use crate::{
    Engine,
    capture::Deadline,
    model::{Graph, Snapshot},
    store::Store,
};
use anyhow::{Context, Result, ensure};
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub struct Host {
    root: PathBuf,
    git: HostGit,
    options: Options,
    preference: Resolved,
}
impl Host {
    /// This is trusted Rust host input, not a serialized permission grant.
    pub fn new(
        root: &Path,
        git: HostGit,
        mut options: Options,
        preference: Resolved,
        d: &Deadline,
    ) -> Result<Self> {
        ensure!(
            options.allow_index_read,
            "repository_scope_index_permission_required"
        );
        options.limits.validate()?;
        let root = repository_scope::root_guard(root, d)?;
        ensure!(preference.root() == root, "repository_host_root_changed");
        options.follow_submodules = preference.choice.follow_submodules;
        options.follow_nested_repositories = preference.choice.follow_nested_repositories;
        Ok(Self {
            root,
            git,
            options,
            preference,
        })
    }
    /// Metadata/physical preflight precedes creation of cold application state.
    pub fn open(self, d: &Deadline) -> Result<Engine> {
        let (snapshot, _, _, _) = repository_scope::capture_root_resolved(
            &self.root,
            &self.git,
            &self.options,
            &self.preference,
            d,
        )?;
        let mut engine = Engine::open_with_timeout(&self.root, d.remaining())?;
        engine.capture_options = crate::capture::options_for_snapshot(&engine.store, &snapshot)?;
        engine.git_repository_host = Some(self);
        engine.sync_source_exclusions(d)?;
        Ok(engine)
    }
    pub(crate) fn check(&self, store: &Store, d: &Deadline) -> Result<()> {
        ensure!(
            store.dir.parent() == Some(self.root.as_path()),
            "repository_host_root_changed"
        );
        self.preference.check(store, d)
    }
    /// Stable host policy only; observed source membership is separate evidence.
    pub(crate) fn binding(&self) -> Result<String> {
        let program = self
            .git
            .executable
            .to_str()
            .context("repository_host_program_encoding_invalid")?;
        ensure!(
            program.len() <= 16_384,
            "repository_host_program_capacity_exceeded"
        );
        Ok(crate::model::identity(&(
            "host_git_repository_capture_v1",
            &self.root,
            program,
            self.options.allow_index_read,
            self.options.follow_submodules,
            self.options.follow_nested_repositories,
            &self.options.limits,
            self.preference.selection().map(|s| &s.id),
        )))
    }
    pub(crate) fn index(
        &self,
        engine: &mut Engine,
        d: &Deadline,
        bounded_saved_graph: bool,
    ) -> Result<(Snapshot, Graph, usize, usize)> {
        ensure!(engine.root == self.root, "repository_host_root_changed");
        self.check(&engine.store, d)?;
        repository_scope::capture_host(
            engine,
            &self.git,
            &self.options,
            &self.preference,
            d,
            bounded_saved_graph,
        )
    }
}
impl Engine {
    /// Initial host-selected capture must succeed before the persistent driver is installed.
    pub fn host_git_repository_capture(&mut self, host: Host, d: &Deadline) -> Result<()> {
        ensure!(
            self.git_repository_host.is_none(),
            "repository_host_already_installed"
        );
        host.index(self, d, false)?;
        self.git_repository_host = Some(host);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hosted_retrieval_index_keeps_typed_saved_graph_capacity_refusal() {
        let root = tempfile::tempdir().unwrap();
        let git = HostGit::discover().unwrap();
        let output = std::process::Command::new(&git.executable)
            .arg("-C")
            .arg(root.path())
            .args(["init", "--quiet", "--template="])
            .output()
            .unwrap();
        assert!(output.status.success());
        std::fs::write(root.path().join("main.rs"), b"pub fn owned(){}\r\n").unwrap();
        let d = Deadline::new(60000);
        let resolved = Resolved::current(root.path(), None, None, &d).unwrap();
        let mut engine = Host::new(
            root.path(),
            git,
            Options {
                allow_index_read: true,
                ..Default::default()
            },
            resolved,
            &d,
        )
        .unwrap()
        .open(&d)
        .unwrap();
        let saved = engine.index_retrieval_at(&d).unwrap().0;
        assert_eq!(
            engine
                .store
                .db
                .execute(
                    "UPDATE graphs SET value=?1 WHERE id=?2",
                    rusqlite::params![" ".repeat(64_000_001), saved.id]
                )
                .unwrap(),
            1
        );
        let error = engine.index_retrieval_at(&d).unwrap_err();
        assert!(
            matches!(
                error.downcast_ref::<crate::retrieval_scopes::RetrievalHardError>(),
                Some(crate::retrieval_scopes::RetrievalHardError::Capacity(_))
            ),
            "{error}"
        );
        assert_eq!(
            engine
                .store
                .db
                .query_row(
                    "SELECT length(CAST(value AS BLOB)) FROM graphs WHERE id=?1",
                    [saved.id],
                    |r| r.get::<_, usize>(0)
                )
                .unwrap(),
            64_000_001
        );
    }
}
