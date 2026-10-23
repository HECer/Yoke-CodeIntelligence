//! Host control state only. Snapshot/repository data never supplies execution trust.
use crate::{
    capture::Deadline,
    model::{identity, valid_id},
    store::Store,
};
use anyhow::{Context, Result, ensure};
use ring::rand::{SecureRandom, SystemRandom};
use rusqlite::{OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const KEY: &str = "host_git_repository_selection_v1";
const MAX_BYTES: usize = 16_384;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Choice {
    pub follow_submodules: bool,
    pub follow_nested_repositories: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Selection {
    pub schema_version: u8,
    pub workspace_id: String,
    pub root: String,
    pub revision: u64,
    pub incarnation: String,
    pub choice: Choice,
    pub id: String,
}
impl Selection {
    fn calculated_id(&self) -> String {
        identity(&(
            KEY,
            self.schema_version,
            &self.workspace_id,
            &self.root,
            self.revision,
            &self.incarnation,
            &self.choice,
        ))
    }
    fn checked(&self, store: &Store, root: &Path) -> Result<()> {
        ensure!(
            self.schema_version == 1
                && self.workspace_id == store.workspace_id
                && self.root == root.to_str().context("repository_selection_root_invalid")?
                && self.revision > 0
                && valid_id(&self.incarnation)
                && valid_id(&self.id)
                && self.id == self.calculated_id(),
            "repository_selection_binding_invalid"
        );
        Ok(())
    }
}
#[derive(Clone, Debug)]
pub enum Expected {
    Absent,
    Id(String),
}
impl Expected {
    pub fn parse(value: &str) -> Result<Self> {
        if value == "absent" {
            return Ok(Self::Absent);
        }
        ensure!(valid_id(value), "repository_selection_expected_invalid");
        Ok(Self::Id(value.into()))
    }
    fn matches(&self, current: Option<&Selection>) -> bool {
        match (self, current) {
            (Self::Absent, None) => true,
            (Self::Id(expected), Some(current)) => expected == &current.id,
            _ => false,
        }
    }
}
fn root_for_store(store: &Store, d: &Deadline) -> Result<PathBuf> {
    d.check()?;
    let root = store
        .dir
        .parent()
        .context("repository_selection_root_invalid")?;
    super::repository_scope::root_guard(root, d)
}
fn raw(store: &Store, d: &Deadline) -> Result<Option<String>> {
    d.check()?;
    let limit = rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH;
    let old = store.db.limit(limit)?;
    store
        .db
        .set_limit(limit, old.min((MAX_BYTES + 1024) as i32))?;
    let result = (|| -> Result<Option<String>> {
        let info: Option<(usize, String)> = store
            .db
            .query_row(
                "SELECT length(CAST(value AS BLOB)),typeof(value) FROM meta WHERE key=?1",
                [KEY],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let Some((length, kind)) = info else {
            return Ok(None);
        };
        ensure!(
            length <= MAX_BYTES && kind == "text",
            "repository_selection_capacity_or_type_invalid"
        );
        let value: String =
            store
                .db
                .query_row("SELECT value FROM meta WHERE key=?1", [KEY], |r| r.get(0))?;
        d.check()?;
        Ok(Some(value))
    })();
    store.db.set_limit(limit, old)?;
    result
}
fn decode(value: &str, store: &Store, root: &Path) -> Result<Selection> {
    let selection: Selection =
        crate::json::decode(crate::json::parse(value.as_bytes(), MAX_BYTES)?)?;
    selection.checked(store, root)?;
    Ok(selection)
}
pub fn get(store: &Store, d: &Deadline) -> Result<Option<Selection>> {
    store.set_deadline(d)?;
    let root = root_for_store(store, d)?;
    raw(store, d)?
        .map(|value| decode(&value, store, &root))
        .transpose()
}
/// An existing-only host lookup. It never creates a Store or resolves a stored root.
pub fn show(root: &Path, d: &Deadline) -> Result<Option<Selection>> {
    let root = super::repository_scope::root_guard(root, d)?;
    for path in [root.join(".yoke"), root.join(".yoke/index.sqlite")] {
        match std::fs::symlink_metadata(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
            Ok(m) => ensure!(
                !crate::paths::is_link(&m),
                "repository_selection_link_refused"
            ),
        }
    }
    let store = Store::open_existing_read_only(&root, d)?;
    get(&store, d)
}
fn write(
    store: &Store,
    expected: &Expected,
    choice: Option<Choice>,
    allowed: bool,
    d: &Deadline,
) -> Result<Option<Selection>> {
    ensure!(
        allowed,
        "repository_selection_host_write_permission_required"
    );
    d.check()?;
    store.set_deadline(d)?;
    if let Expected::Id(id) = expected {
        ensure!(valid_id(id), "repository_selection_expected_invalid");
    }
    ensure!(
        store.db.is_autocommit(),
        "repository_selection_requires_owned_transaction"
    );
    let root = root_for_store(store, d)?;
    let tx = Transaction::new_unchecked(&store.db, TransactionBehavior::Immediate)?;
    let original = raw(store, d)?;
    let current = original
        .as_ref()
        .map(|v| decode(v, store, &root))
        .transpose()?;
    ensure!(
        expected.matches(current.as_ref()),
        "repository_selection_stale"
    );
    let selected = choice
        .map(|choice| -> Result<Selection> {
            let revision = current.as_ref().map_or(Ok(1), |s| {
                s.revision
                    .checked_add(1)
                    .context("repository_selection_revision_exhausted")
            })?;
            let incarnation = if let Some(current) = &current {
                current.incarnation.clone()
            } else {
                let mut random = [0u8; 32];
                SystemRandom::new()
                    .fill(&mut random)
                    .map_err(|_| anyhow::anyhow!("repository_selection_randomness_unavailable"))?;
                hex::encode(random)
            };
            let mut selected = Selection {
                schema_version: 1,
                workspace_id: store.workspace_id.clone(),
                root: root
                    .to_str()
                    .context("repository_selection_root_invalid")?
                    .into(),
                revision,
                incarnation,
                choice,
                id: String::new(),
            };
            selected.id = selected.calculated_id();
            selected.checked(store, &root)?;
            Ok(selected)
        })
        .transpose()?;
    let changed = match (&selected, &original) {
        (Some(selected), None) => {
            let value = serde_json::to_string(selected)?;
            ensure!(
                value.len() <= MAX_BYTES,
                "repository_selection_capacity_or_type_invalid"
            );
            tx.execute(
                "INSERT INTO meta(key,value) VALUES(?1,?2) ON CONFLICT(key) DO NOTHING",
                params![KEY, value],
            )?
        }
        (Some(selected), Some(original)) => {
            let value = serde_json::to_string(selected)?;
            ensure!(
                value.len() <= MAX_BYTES,
                "repository_selection_capacity_or_type_invalid"
            );
            tx.execute(
                "UPDATE meta SET value=?1 WHERE key=?2 AND value=?3",
                params![value, KEY, original],
            )?
        }
        (None, Some(original)) => tx.execute(
            "DELETE FROM meta WHERE key=?1 AND value=?2",
            params![KEY, original],
        )?,
        (None, None) => 0,
    };
    ensure!(
        changed == usize::from(selected.is_some() || original.is_some()),
        "repository_selection_stale"
    );
    d.check()?;
    tx.commit()?;
    Ok(selected)
}
pub fn set(
    store: &Store,
    expected: &Expected,
    choice: Choice,
    allow_host_writes: bool,
    d: &Deadline,
) -> Result<Selection> {
    write(store, expected, Some(choice), allow_host_writes, d)?
        .context("repository_selection_write_failed")
}
pub fn clear(
    store: &Store,
    expected: &Expected,
    allow_host_writes: bool,
    d: &Deadline,
) -> Result<()> {
    write(store, expected, None, allow_host_writes, d)?;
    Ok(())
}

/// Invocation-owned read binding. It carries no program or execution permission.
#[derive(Clone)]
pub struct Resolved {
    root: PathBuf,
    expected: Option<Selection>,
    pub choice: Choice,
}
impl Resolved {
    pub(super) fn check_current(&self, d: &Deadline) -> Result<()> {
        ensure!(
            show(&self.root, d)? == self.expected,
            "repository_selection_stale"
        );
        Ok(())
    }
    pub(super) fn root(&self) -> &Path {
        &self.root
    }
    pub fn current(
        root: &Path,
        submodules: Option<bool>,
        nested: Option<bool>,
        d: &Deadline,
    ) -> Result<Self> {
        let root = super::repository_scope::root_guard(root, d)?;
        let expected = show(&root, d)?;
        let mut choice = expected
            .as_ref()
            .map(|s| s.choice.clone())
            .unwrap_or_default();
        if let Some(value) = submodules {
            choice.follow_submodules = value;
        }
        if let Some(value) = nested {
            choice.follow_nested_repositories = value;
        }
        Ok(Self {
            root,
            expected,
            choice,
        })
    }
    pub fn selection(&self) -> Option<&Selection> {
        self.expected.as_ref()
    }
    pub(crate) fn check(&self, store: &Store, d: &Deadline) -> Result<()> {
        ensure!(
            root_for_store(store, d)? == self.root,
            "repository_selection_root_changed"
        );
        ensure!(
            get(store, d)? == self.expected,
            "repository_selection_stale"
        );
        Ok(())
    }
}
