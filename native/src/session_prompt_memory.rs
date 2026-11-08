//! Resolved authored-note data only. No store, filesystem or executable handles.
use crate::{capture::Deadline, model, notes::Scope};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub const VERSION: &str = "native_prompt_memory_snapshot_v1";
pub const MAX_NAMES: usize = 32;
pub const MAX_BYTES: usize = 256000;

pub fn selection(names: &[String]) -> Result<()> {
    ensure!(
        names.len() <= MAX_NAMES,
        "session_prompt_memory_selection_capacity"
    );
    let mut seen = BTreeSet::new();
    for name in names {
        crate::notes::prompt_selection(name)?;
        ensure!(
            seen.insert(name),
            "session_prompt_memory_selection_duplicate"
        );
    }
    Ok(())
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Anchor {
    pub path: String,
    pub content_hash: String,
    pub snapshot_id: String,
    pub source_workspace_id: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Revision {
    pub revision_id: String,
    pub scope: Scope,
    pub storage_workspace_id: String,
    pub source_workspace_id: String,
    pub snapshot_id: String,
    pub content_sha256: String,
    pub anchors: Vec<Anchor>,
    pub trust: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum Entry {
    Available { content: String, revision: Revision },
    Unavailable,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub entries: BTreeMap<String, Entry>,
}

impl Snapshot {
    pub fn validate(&self, d: &Deadline) -> Result<()> {
        selection(&self.entries.keys().cloned().collect::<Vec<_>>())?;
        crate::json::encode_bounded(self, MAX_BYTES, d)?;
        for (name, entry) in &self.entries {
            d.check()?;
            let (scope, _) = crate::notes::prompt_selection(name)?;
            if let Entry::Available { content, revision } = entry {
                crate::notes::prompt_content(content)?;
                ensure!(
                    revision.scope == scope
                        && revision.content_sha256 == model::hash(content)
                        && [
                            &revision.revision_id,
                            &revision.storage_workspace_id,
                            &revision.source_workspace_id,
                            &revision.snapshot_id,
                        ]
                        .into_iter()
                        .all(|id| model::valid_id(id))
                        && revision.anchors.len() <= 100
                        && revision.trust.len() <= 200
                        && !revision.trust.chars().any(char::is_control),
                    "session_prompt_memory_revision_invalid"
                );
                for anchor in &revision.anchors {
                    ensure!(
                        crate::paths::normalized(&anchor.path)? == anchor.path
                            && [
                                &anchor.content_hash,
                                &anchor.snapshot_id,
                                &anchor.source_workspace_id
                            ]
                            .into_iter()
                            .all(|id| model::valid_id(id)),
                        "session_prompt_memory_anchor_invalid"
                    );
                }
            }
        }
        crate::json::encode_bounded(&self.receipts(), 65536, d)?;
        Ok(())
    }

    pub fn hash(&self) -> Result<String> {
        Ok(model::hash(serde_json::to_vec(self)?))
    }

    pub fn receipts(&self) -> Value {
        let entries: BTreeMap<_, _> = self
            .entries
            .iter()
            .map(|(name, entry)| {
                let receipt = match entry {
                    Entry::Available { content, revision } => {
                        json!({"status":"available","bytes":content.len(),"revision":revision})
                    }
                    Entry::Unavailable => json!({"status":"unavailable"}),
                };
                (name, receipt)
            })
            .collect();
        json!({"schema":VERSION,"entries":entries,"contentIsAuthoredClaim":true,"sourceBound":false,"anchorCurrentness":"not_revalidated_for_prompt_embedding","complete":false})
    }

    pub fn check_selection(&self, names: &[String], d: &Deadline) -> Result<()> {
        selection(names)?;
        self.validate(d)?;
        ensure!(
            names.iter().collect::<BTreeSet<_>>() == self.entries.keys().collect::<BTreeSet<_>>(),
            "session_prompt_memory_selection_changed"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selected_names_retain_unicode_and_refuse_unsafe_or_duplicate_addresses() {
        let d = Deadline::new(10000);
        let names = vec![
            "guide e\u{301}".to_owned(),
            "global/guide \u{e9}".to_owned(),
        ];
        let snapshot = Snapshot {
            entries: names
                .iter()
                .map(|n| (n.clone(), Entry::Unavailable))
                .collect(),
        };
        snapshot.check_selection(&names, &d).unwrap();
        assert_eq!(snapshot.receipts()["entries"].as_object().unwrap().len(), 2);
        for names in [
            vec!["../outside".into()],
            vec!["global/../outside".into()],
            vec![".env".into()],
            vec!["global/".into()],
            vec!["same".into(), "same".into()],
        ] {
            assert!(selection(&names).is_err());
        }
        assert!(snapshot.check_selection(&["other".into()], &d).is_err());
    }
    #[test]
    fn serialized_receipts_exclude_content_and_reject_forged_revision_or_scope() {
        let d = Deadline::new(10000);
        let text = "owned {{ not_evaluated }}";
        let revision = Revision {
            revision_id: "a".repeat(64),
            scope: Scope::Project,
            storage_workspace_id: "b".repeat(64),
            source_workspace_id: "c".repeat(64),
            snapshot_id: "d".repeat(64),
            content_sha256: model::hash(text),
            anchors: vec![],
            trust: "authored_unverified".into(),
        };
        let mut snapshot = Snapshot {
            entries: BTreeMap::from([(
                "guide".into(),
                Entry::Available {
                    content: text.into(),
                    revision,
                },
            )]),
        };
        snapshot.validate(&d).unwrap();
        assert!(!snapshot.receipts().to_string().contains(text));
        if let Entry::Available { revision, .. } = snapshot.entries.get_mut("guide").unwrap() {
            revision.scope = Scope::Global;
        }
        assert!(snapshot.validate(&d).is_err());
    }
}
