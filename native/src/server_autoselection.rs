//! Creation-time classification of captured filenames; registrations remain host-owned.
use crate::{capture::Deadline, model};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
pub const VERSION: &str = "host_creation_language_selection_v1";
pub const MAX_FILES: usize = 100_000;
const MAX_PATH_BYTES: usize = 16_000_000;

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub priorities: BTreeMap<String, i32>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Row {
    id: String,
    priority: i32,
    case_sensitive: bool,
    suffixes: Vec<String>,
}
fn rows() -> &'static [Row] {
    static ROWS: std::sync::LazyLock<Vec<Row>> = std::sync::LazyLock::new(|| {
        #[derive(Deserialize)]
        struct Contract {
            backends: Vec<Row>,
        }
        let mut data: Contract =
            serde_json::from_str(include_str!("server_autoselection_contract.json"))
                .expect("Compiled language selection contract");
        for row in &mut data.backends {
            if !row.case_sensitive {
                row.suffixes = row
                    .suffixes
                    .iter()
                    .map(|s| s.chars().flat_map(char::to_lowercase).collect())
                    .collect();
                row.suffixes.sort();
                row.suffixes.dedup();
            }
        }
        data.backends
    });
    &ROWS
}
pub(crate) fn contract_binding() -> &'static str {
    static BINDING: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
        model::hash(include_bytes!("server_autoselection_contract.json"))
    });
    &BINDING
}
impl Row {
    fn matches(&self, path: &str) -> bool {
        if self.case_sensitive {
            self.suffixes.iter().any(|s| path.ends_with(s.as_str()))
        } else {
            let folded: String = path.chars().flat_map(char::to_lowercase).collect();
            self.suffixes.iter().any(|s| folded.ends_with(s.as_str()))
        }
    }
}
impl Config {
    pub(crate) fn validate(&self) -> Result<()> {
        ensure!(
            self.priorities.len() <= 80 && serde_json::to_vec(self)?.len() <= 8000,
            "language_auto_policy_capacity"
        );
        ensure!(
            self.priorities
                .keys()
                .all(|id| rows().iter().any(|r| r.id == *id)),
            "language_auto_unknown_backend"
        );
        Ok(())
    }
    pub(crate) fn from_yaml(value: &Value) -> Result<Option<Self>> {
        if value.is_null() {
            return Ok(None);
        }
        let priorities = serde_json::from_value(value.clone())
            .context("session_config_ls_priorities_invalid")?;
        let value = Self { priorities };
        value.validate()?;
        Ok(Some(value))
    }
    pub(crate) fn priority(&self, id: &str) -> Option<i32> {
        self.priorities
            .get(id)
            .copied()
            .or_else(|| rows().iter().find(|r| r.id == id).map(|r| r.priority))
    }
}
pub(crate) fn matches_backend(id: &str, path: &str) -> bool {
    rows()
        .iter()
        .find(|r| r.id == id)
        .is_some_and(|r| r.matches(path))
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Decision {
    pub backend_id: Option<String>,
    pub matched_files: usize,
    pub captured_files: usize,
    pub omitted_paths: usize,
    pub filename_set_hash: String,
}
impl Decision {
    pub(crate) fn validate(&self) -> Result<()> {
        ensure!(
            self.captured_files <= MAX_FILES
                && self.omitted_paths <= MAX_FILES
                && self.matched_files <= self.captured_files
                && model::valid_id(&self.filename_set_hash),
            "language_auto_decision_invalid"
        );
        match &self.backend_id {
            Some(id) => ensure!(
                self.matched_files > 0 && rows().iter().any(|r| r.id == *id),
                "language_auto_decision_backend_invalid"
            ),
            None => ensure!(
                self.matched_files == 0,
                "language_auto_empty_decision_invalid"
            ),
        }
        Ok(())
    }
    pub(crate) fn apply(&self, engine: &mut crate::Engine) -> Result<()> {
        self.validate()?;
        if let Some(id) = &self.backend_id {
            ensure!(
                engine.backend_servers.contains_key(id),
                "language_server_not_host_registered"
            );
        }
        engine.automatic_backend_selection = true;
        engine.selected_backends = Some(self.backend_id.iter().cloned().collect());
        Ok(())
    }
}
pub(crate) fn select<'a>(
    config: &Config,
    paths: impl IntoIterator<Item = &'a str>,
    omitted_paths: usize,
    deadline: &Deadline,
) -> Result<Decision> {
    deadline.check()?;
    config.validate()?;
    ensure!(
        omitted_paths <= MAX_FILES,
        "language_auto_omission_capacity"
    );
    let mut input = BTreeSet::new();
    let mut bytes = 0usize;
    for path in paths {
        deadline.check()?;
        bytes = bytes
            .checked_add(path.len())
            .context("language_auto_path_capacity")?;
        ensure!(
            path.len() <= 4096 && bytes <= MAX_PATH_BYTES && input.len() < MAX_FILES,
            "language_auto_path_capacity"
        );
        ensure!(
            crate::paths::normalized_syntax(path)? == path && input.insert(path),
            "language_auto_path_invalid_or_duplicate"
        );
    }
    let mut winner: Option<(&Row, usize, i32)> = None;
    for row in rows() {
        deadline.check()?;
        let priority = config.priority(&row.id).expect("Compiled backend priority");
        if priority <= 0 {
            continue;
        }
        let mut count = 0;
        for path in &input {
            deadline.check()?;
            if row.matches(path) {
                count += 1;
            }
        }
        if count > 0
            && winner.is_none_or(|(_, best, p)| count > best || (count == best && priority > p))
        {
            winner = Some((row, count, priority));
        }
    }
    let value = Decision {
        backend_id: winner.map(|(row, _, _)| row.id.clone()),
        matched_files: winner.map_or(0, |(_, n, _)| n),
        captured_files: input.len(),
        omitted_paths,
        filename_set_hash: model::identity(&(VERSION, &input)),
    };
    value.validate()?;
    deadline.check()?;
    Ok(value)
}
pub(crate) fn detect(
    engine: &mut crate::Engine,
    config: &Config,
    deadline: &Deadline,
) -> Result<Decision> {
    let (snapshot, _, _, _) = engine.index(deadline)?;
    let scope = crate::capture::source_scope_policy(&engine.store, &snapshot)?;
    for file in &snapshot.files {
        deadline.check()?;
        scope
            .file(&file.path)
            .context("language_auto_source_membership_missing")?;
    }
    let decision = select(
        config,
        snapshot.files.iter().map(|f| f.path.as_str()),
        snapshot.omissions.len(),
        deadline,
    )?;
    if let Some(id) = &decision.backend_id {
        ensure!(
            engine.backend_servers.contains_key(id),
            "language_server_not_host_registered"
        );
    }
    Ok(decision)
}
pub(crate) fn validate_retained(choices: &BTreeMap<String, Decision>) -> Result<()> {
    ensure!(
        choices.len() <= 101 && serde_json::to_vec(choices)?.len() <= 32000,
        "language_auto_decision_capacity"
    );
    for (name, value) in choices {
        ensure!(
            !name.is_empty()
                && name.len() <= 100
                && name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_-.".contains(&b)),
            "language_auto_project_invalid"
        );
        value.validate()?;
    }
    Ok(())
}
#[cfg(test)]
#[path = "server_autoselection_tests.rs"]
mod tests;
