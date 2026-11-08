//! Optional dashboard provenance stays inside the original workspace journal.
//! Deserializing a binding never grants source-write or recovery authority.
use super::*;
use crate::model::valid_id;
use rusqlite::OptionalExtension;

const MAX_JOURNAL_BYTES: usize = 1_000_000;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct DocumentBinding {
    pub schema_version: u32,
    pub access_binding: String,
    pub document_id: String,
    pub project: String,
    pub workspace_id: String,
    pub path: String,
    pub max_document_bytes: usize,
}
impl DocumentBinding {
    pub(crate) fn validate(&self, store: &Store) -> Result<()> {
        ensure!(
            self.schema_version == 1
                && valid_id(&self.access_binding)
                && self.workspace_id == store.workspace_id
                && self.path.len() <= 64_000
                && (1..=64_000).contains(&self.max_document_bytes)
                && crate::paths::normalized_syntax(&self.path)? == self.path,
            "Dashboard workspace journal binding invalid"
        );
        crate::project_session::label(&self.document_id)?;
        crate::project_session::label(&self.project)
    }
    pub(super) fn check_plan(&self, store: &Store, plan: &Plan) -> Result<()> {
        self.validate(store)?;
        ensure!(
            plan.workspace_id == self.workspace_id
                && plan.directory_move.is_none()
                && plan.files.len() == 1
                && plan.files[0].path == self.path
                && plan.files[0].before_hash.is_some()
                && plan.files[0].after_hash.is_some(),
            "Dashboard workspace journal must describe one selected existing file"
        );
        Ok(())
    }
}

/// Distinguish no journal from an existing unbound journal. The latter cannot
/// be silently adopted by a new dashboard selection or actor.
pub(super) fn saved_binding(store: &Store, id: &str) -> Result<Option<Option<DocumentBinding>>> {
    store.check_deadline()?;
    let bytes: Option<Vec<u8>> = store
        .db
        .query_row(
            "SELECT substr(CAST(value AS BLOB),1,?2) FROM records WHERE kind='journal' AND id=?1",
            rusqlite::params![id, MAX_JOURNAL_BYTES + 1],
            |row| row.get(0),
        )
        .optional()?;
    bytes
        .map(|bytes| -> Result<_> {
            ensure!(
                bytes.len() <= MAX_JOURNAL_BYTES,
                "Dashboard journal capacity exceeded"
            );
            let mut value = crate::json::parse(&bytes, MAX_JOURNAL_BYTES)?;
            value
                .as_object_mut()
                .context("Invalid native journal")?
                .remove("dashboardDocument")
                .map(crate::json::decode)
                .transpose()
        })
        .transpose()
}

pub(super) fn check_saved_binding(
    store: &Store,
    id: &str,
    binding: &DocumentBinding,
) -> Result<()> {
    if let Some(saved) = saved_binding(store, id)? {
        ensure!(
            saved.as_ref() == Some(binding),
            "Existing workspace journal belongs to another or unbound workflow"
        );
    }
    Ok(())
}

pub(super) fn record_journal(
    store: &Store,
    receipt: &Receipt,
    binding: Option<&DocumentBinding>,
) -> Result<()> {
    store.check_deadline()?;
    if let Some(binding) = binding {
        check_saved_binding(store, &receipt.id, binding)?;
        let mut value = serde_json::to_value(receipt)?;
        value["dashboardDocument"] = serde_json::to_value(binding)?;
        return store.record("journal", &receipt.id, &value);
    }
    // Ordinary receipts keep their exact serialization. Only an already
    // present provenance member is carried across native/emergency recovery.
    let value = serde_json::to_string(receipt)?;
    store.check_deadline()?;
    store.db.execute(
        "INSERT INTO records(kind,id,value) VALUES('journal',?1,?2)
         ON CONFLICT(kind,id) DO UPDATE SET value=
         CASE WHEN json_type(CASE WHEN json_valid(records.value) THEN records.value ELSE '{}' END,'$.dashboardDocument') IS NOT NULL
         THEN json_set(excluded.value,'$.dashboardDocument',json_extract(records.value,'$.dashboardDocument'))
         ELSE excluded.value END",
        rusqlite::params![receipt.id,value])?;
    Ok(())
}

/// A review is an opaque checked observation, not permission. It is never
/// deserialized from a browser request and is re-read under the writer lock.
pub(crate) struct ReviewedDocumentJournal {
    receipt: Receipt,
    plan: Plan,
    binding: DocumentBinding,
    event_id: String,
    before_content: String,
    after_content: String,
}
impl ReviewedDocumentJournal {
    pub(crate) fn receipt(&self) -> &Receipt {
        &self.receipt
    }
    pub(crate) fn plan(&self) -> &Plan {
        &self.plan
    }
    pub(crate) fn event_id(&self) -> &str {
        &self.event_id
    }
    pub(crate) fn before_content(&self) -> &str {
        &self.before_content
    }
    pub(crate) fn after_content(&self) -> &str {
        &self.after_content
    }
    pub(super) fn binding(&self) -> &DocumentBinding {
        &self.binding
    }
}

pub(crate) fn review_document_journal(
    store: &Store,
    id: &str,
    binding: &DocumentBinding,
    d: &Deadline,
) -> Result<ReviewedDocumentJournal> {
    d.check()?;
    binding.validate(store)?;
    ensure!(valid_id(id), "Invalid workspace journal ID");
    store.check_deadline()?;
    let raw: Vec<u8> = store.db.query_row(
        "SELECT substr(CAST(value AS BLOB),1,?2) FROM records WHERE kind='journal' AND id=?1",
        rusqlite::params![id, MAX_JOURNAL_BYTES + 1],
        |row| row.get(0),
    )?;
    ensure!(
        raw.len() <= MAX_JOURNAL_BYTES,
        "Dashboard journal capacity exceeded"
    );
    let event_id = crate::model::hash(&raw);
    let mut value = crate::json::parse(&raw, MAX_JOURNAL_BYTES)?;
    let recorded: DocumentBinding = crate::json::decode(
        value
            .as_object_mut()
            .context("Invalid native journal")?
            .remove("dashboardDocument")
            .context("Unbound workspace journal requires native host recovery")?,
    )?;
    ensure!(&recorded == binding, "Workspace journal selection changed");
    let receipt: Receipt = crate::json::decode(value.clone())?;
    ensure!(
        serde_json::to_value(&receipt)? == value
            && receipt.id == id
            && receipt.mode == "workspace"
            && receipt.sandbox_path.is_none()
            && valid_id(&receipt.plan_id)
            && receipt.idempotency_key.len() <= 150
            && !receipt.idempotency_key.is_empty()
            && receipt
                .idempotency_key
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "_.-".contains(c))
            && receipt.id == identity(&(&receipt.plan_id, &receipt.idempotency_key))
            && ["intent", "applied", "recovery_required", "rolled_back"]
                .contains(&receipt.status.as_str()),
        "Invalid dashboard workspace journal receipt"
    );
    let plan = load_checked_plan(store, &receipt.plan_id, d)?;
    binding.check_plan(store, &plan)?;
    ensure!(
        receipt.snapshot_id == plan.result_snapshot_id
            && receipt.changed_paths == vec![binding.path.clone()]
            && (receipt.applied_paths.is_empty() || receipt.applied_paths == receipt.changed_paths),
        "Dashboard workspace journal plan differs"
    );
    for snapshot in [&plan.base_snapshot_id, &plan.result_snapshot_id] {
        let snapshot = store.snapshot(snapshot)?;
        let file = snapshot
            .files
            .iter()
            .find(|f| f.path == binding.path)
            .context("Selected journal source missing")?;
        ensure!(
            file.size <= binding.max_document_bytes,
            "Selected journal source capacity exceeded"
        );
    }
    let file = &plan.files[0];
    let before_content = String::from_utf8(
        store.read_blob(
            file.before_hash
                .as_ref()
                .context("Journal before bytes required")?,
        )?,
    )?;
    let after_content = String::from_utf8(
        store.read_blob(
            file.after_hash
                .as_ref()
                .context("Journal after bytes required")?,
        )?,
    )?;
    ensure!(
        before_content.len() <= binding.max_document_bytes
            && after_content.len() <= binding.max_document_bytes,
        "Journal document bytes exceed selected capacity"
    );
    d.check()?;
    Ok(ReviewedDocumentJournal {
        receipt,
        plan,
        binding: binding.clone(),
        event_id,
        before_content,
        after_content,
    })
}

pub(crate) fn document_journal_page(
    store: &Store,
    binding: &DocumentBinding,
    cursor: Option<&str>,
    d: &Deadline,
) -> Result<Value> {
    binding.validate(store)?;
    d.check()?;
    ensure!(cursor.is_none_or(valid_id), "Invalid journal cursor");
    let prior = store.replace_deadline(Some(store.inherited_deadline(d)?));
    let result = (|| -> Result<Value> {
        store.check_deadline()?;
        let invalid_ids:bool=store.db.query_row(
            "SELECT EXISTS(SELECT 1 FROM records WHERE kind='journal' AND (length(id)!=64 OR id GLOB '*[^0-9a-f]*'))",[],|r|r.get(0))?;
        let rows = {
            let mut statement=store.db.prepare(
                "SELECT id,substr(CAST(value AS BLOB),1,?2) FROM records WHERE kind='journal' AND id>?1 AND length(id)=64 AND id NOT GLOB '*[^0-9a-f]*' ORDER BY id LIMIT 9")?;
            statement
                .query_map(
                    rusqlite::params![cursor.unwrap_or(""), MAX_JOURNAL_BYTES + 1],
                    |r| Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?)),
                )?
                .collect::<std::result::Result<Vec<_>, _>>()?
        };
        let total = rows.len();
        let mut scanned = 0;
        let mut last = None;
        let mut unavailable = usize::from(invalid_ids);
        let mut unbound = 0;
        let mut journals = Vec::new();
        for (id, raw) in rows.into_iter().take(8) {
            d.check()?;
            scanned += 1;
            last = Some(id.clone());
            let decoded = (|| -> Result<Option<ReviewedDocumentJournal>> {
                ensure!(
                    raw.len() <= MAX_JOURNAL_BYTES,
                    "Journal page entry capacity exceeded"
                );
                let value = crate::json::parse(&raw, MAX_JOURNAL_BYTES)?;
                let Some(data) = value.get("dashboardDocument") else {
                    let legacy: Receipt = crate::json::decode(value.clone())?;
                    ensure!(
                        serde_json::to_value(&legacy)? == value && legacy.id == id,
                        "Unsupported native journal shape"
                    );
                    if legacy.changed_paths.contains(&binding.path) {
                        unbound += 1;
                    }
                    return Ok(None);
                };
                let recorded: DocumentBinding = crate::json::decode(data.clone())?;
                if &recorded != binding {
                    if recorded.path == binding.path {
                        unbound += 1;
                    }
                    return Ok(None);
                }
                review_document_journal(store, &id, binding, d).map(Some)
            })();
            match decoded {
                Ok(Some(review)) => {
                    let receipt = review.receipt();
                    let terminal = receipt.status == "rolled_back";
                    journals.push(json!({"kind":"workspace","journalId":receipt.id,
                        "plan":{"id":review.plan().id,"content":review.after_content()},
                        "beforeContent":review.before_content(),"eventId":review.event_id(),
                        "phase":receipt.status,"pending":matches!(receipt.status.as_str(),"intent"|"recovery_required"),
                        "allowedDirections":if terminal {Vec::<&str>::new()} else {vec!["rollback"]},
                        "direction":if terminal {Some("rollback")} else {None},
                        "beforeHash":review.plan().files[0].before_hash,
                        "afterHash":review.plan().files[0].after_hash}));
                }
                Ok(None) => {}
                Err(_) => unavailable += 1,
            }
            if journals.len() == 2 {
                break;
            }
        }
        let more = scanned < total;
        Ok(
            json!({"journals":journals,"workspaceRecoveryAvailable":true,
            "coverage":"dashboard_bound_journals_for_current_selection",
            "fromCursor":cursor,"nextCursor":if more {last} else {None},
            "scannedEntries":scanned,"unavailableEntries":unavailable,
            "unboundEntriesRequireHost":unbound,
            "complete":cursor.is_none()&&!more&&unavailable==0&&unbound==0,
            "otherNativeJournalsRequireHost":true,
            "fileChanged":false,"metadataChanged":false,"sourceWritten":false}),
        )
    })();
    store.replace_deadline(prior);
    result
}

pub(crate) fn retention_projection(value: &Value) -> Result<Value> {
    let mut projected = value.clone();
    // Selection hashes, labels and paths are provenance data. Only the
    // preceding native receipt/plan/snapshot fields retain source evidence.
    projected
        .as_object_mut()
        .context("Invalid native journal")?
        .remove("dashboardDocument");
    Ok(projected)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn d() -> Deadline {
        Deadline::new(120_000)
    }
    fn fixture() -> (tempfile::TempDir, crate::Engine, DocumentBinding, Plan) {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("settings-π.json"), b"{\"n\":1}\r\n").unwrap();
        let mut engine = crate::Engine::open(root.path()).unwrap();
        engine.allow_writes = true;
        let value=engine.invoke("ci_change_prepare",json!({"edits":[{"kind":"replace","path":"settings-π.json",
            "expectedHash":crate::model::hash(b"{\"n\":1}\r\n"),"startByte":0,"endByte":9,"text":"{\"n\":2}\r\n"}],"maxBytes":1000000})).unwrap();
        assert_eq!(value["status"], "ok", "{value}");
        let plan: Plan = serde_json::from_value(value["data"]["plan"].clone()).unwrap();
        let binding = DocumentBinding {
            schema_version: 1,
            access_binding: crate::model::hash(b"owned selection"),
            document_id: "settings".into(),
            project: "default".into(),
            workspace_id: engine.store.workspace_id.clone(),
            path: "settings-π.json".into(),
            max_document_bytes: 64000,
        };
        (root, engine, binding, plan)
    }
    fn apply(engine: &mut crate::Engine, binding: Option<DocumentBinding>, plan: &Plan) -> Receipt {
        engine.approve(&plan.id, false).unwrap();
        let program = std::env::current_exe().unwrap();
        let verified = crate::verify::execute(
            engine,
            &plan.id,
            program.to_str().unwrap(),
            &["--help".into()],
            true,
            &d(),
        )
        .unwrap();
        assert_eq!(verified["status"], "passed", "{verified}");
        let args = json!({"planId":plan.id,"idempotencyKey":"owned-dashboard-recovery","mode":"workspace","maxBytes":1000000});
        let result = if let Some(binding) = binding {
            engine.invoke_document_apply(args, binding)
        } else {
            engine.invoke("ci_change_apply", args)
        }
        .unwrap();
        assert_eq!(result["status"], "ok", "{result}");
        serde_json::from_value(result["data"]["receipt"].clone()).unwrap()
    }
    fn raw(store: &Store, id: &str) -> String {
        store
            .db
            .query_row(
                "SELECT value FROM records WHERE kind='journal' AND id=?1",
                [id],
                |r| r.get(0),
            )
            .unwrap()
    }

    #[test]
    fn absent_binding_keeps_native_journal_bytes_and_bound_progress_survives_host_rollback() {
        for selected in [false, true] {
            let (root, mut engine, binding, plan) = fixture();
            let receipt = apply(&mut engine, selected.then(|| binding.clone()), &plan);
            if selected {
                assert_eq!(
                    saved_binding(&engine.store, &receipt.id).unwrap(),
                    Some(Some(binding.clone()))
                );
                let mut progress = receipt.clone();
                progress.status = "recovery_required".into();
                record_journal(&engine.store, &progress, None).unwrap();
                assert_eq!(
                    saved_binding(&engine.store, &receipt.id).unwrap(),
                    Some(Some(binding.clone()))
                );
            } else {
                assert_eq!(
                    raw(&engine.store, &receipt.id),
                    serde_json::to_string(&receipt).unwrap()
                );
            }
            super::super::recover(&engine.store, root.path(), &receipt.id, true).unwrap();
            assert_eq!(
                fs::read(root.path().join("settings-π.json")).unwrap(),
                b"{\"n\":1}\r\n"
            );
            if selected {
                assert_eq!(
                    saved_binding(&engine.store, &receipt.id).unwrap(),
                    Some(Some(binding))
                );
            }
        }
    }

    #[test]
    fn reviewed_rollback_rechecks_event_under_writer_lock_and_preserves_foreign_bytes() {
        let (root, mut engine, binding, plan) = fixture();
        let receipt = apply(&mut engine, Some(binding.clone()), &plan);
        let review = review_document_journal(&engine.store, &receipt.id, &binding, &d()).unwrap();
        let mut progressed = receipt.clone();
        progressed.status = "recovery_required".into();
        record_journal(&engine.store, &progressed, None).unwrap();
        let mut started = false;
        assert!(
            super::super::recover_document(
                &engine.store,
                root.path(),
                &review,
                true,
                &d(),
                &mut started
            )
            .is_err()
        );
        assert!(!started);
        assert_eq!(
            fs::read(root.path().join("settings-π.json")).unwrap(),
            b"{\"n\":2}\r\n"
        );
        let review = review_document_journal(&engine.store, &receipt.id, &binding, &d()).unwrap();
        fs::write(root.path().join("settings-π.json"), b"foreign bytes").unwrap();
        let before = raw(&engine.store, &receipt.id);
        assert!(
            super::super::recover_document(
                &engine.store,
                root.path(),
                &review,
                true,
                &d(),
                &mut started
            )
            .is_err()
        );
        assert!(!started);
        assert_eq!(raw(&engine.store, &receipt.id), before);
        assert_eq!(
            fs::read(root.path().join("settings-π.json")).unwrap(),
            b"foreign bytes"
        );
    }

    #[test]
    fn reviewed_rollback_restores_exact_bytes_without_granting_writes_or_execution() {
        let (root, mut engine, binding, plan) = fixture();
        let receipt = apply(&mut engine, Some(binding.clone()), &plan);
        let review = review_document_journal(&engine.store, &receipt.id, &binding, &d()).unwrap();
        let mut started = false;
        assert!(
            super::super::recover_document(
                &engine.store,
                root.path(),
                &review,
                false,
                &d(),
                &mut started
            )
            .is_err()
        );
        assert!(!started);
        let (restored, written) = super::super::recover_document(
            &engine.store,
            root.path(),
            &review,
            true,
            &d(),
            &mut started,
        )
        .unwrap();
        assert!(started && written);
        assert_eq!(restored.status, "rolled_back");
        assert_eq!(
            fs::read(root.path().join("settings-π.json")).unwrap(),
            b"{\"n\":1}\r\n"
        );
        assert_eq!(
            saved_binding(&engine.store, &receipt.id).unwrap(),
            Some(Some(binding))
        );
        let mut started = false;
        assert!(
            super::super::recover_document(
                &engine.store,
                root.path(),
                &review,
                true,
                &d(),
                &mut started
            )
            .is_err()
        );
        assert!(!started);
        assert!(crate::validate("ci_change_apply",&json!({"planId":plan.id,"idempotencyKey":"injected","mode":"workspace","dashboardDocument":{}})).is_err());
    }

    #[test]
    fn recovery_selection_budget_and_deadline_refuse_without_mutation() {
        let (root, mut engine, binding, plan) = fixture();
        let receipt = apply(&mut engine, Some(binding.clone()), &plan);
        let mut foreign = binding.clone();
        foreign.access_binding = "a".repeat(64);
        assert!(review_document_journal(&engine.store, &receipt.id, &foreign, &d()).is_err());
        let review = review_document_journal(&engine.store, &receipt.id, &binding, &d()).unwrap();
        let mut started = false;
        let expired = Deadline::from_duration(std::time::Duration::ZERO);
        assert!(
            super::super::recover_document(
                &engine.store,
                root.path(),
                &review,
                true,
                &expired,
                &mut started
            )
            .is_err()
        );
        assert!(!started);
        fs::write(root.path().join("settings-π.json"), vec![b'x'; 64001]).unwrap();
        let before = raw(&engine.store, &receipt.id);
        assert!(
            super::super::recover_document(
                &engine.store,
                root.path(),
                &review,
                true,
                &d(),
                &mut started
            )
            .is_err()
        );
        assert!(!started);
        assert_eq!(raw(&engine.store, &receipt.id), before);
        assert_eq!(
            fs::metadata(root.path().join("settings-π.json"))
                .unwrap()
                .len(),
            64001
        );
    }

    #[test]
    fn unbound_journals_cannot_be_adopted_and_provenance_has_no_retention_authority() {
        let (_root, mut engine, binding, plan) = fixture();
        let receipt = apply(&mut engine, None, &plan);
        let before = raw(&engine.store, &receipt.id);
        assert!(review_document_journal(&engine.store, &receipt.id, &binding, &d()).is_err());
        assert!(record_journal(&engine.store, &receipt, Some(&binding)).is_err());
        assert_eq!(raw(&engine.store, &receipt.id), before);
        let value = json!({"snapshotId":"a".repeat(64),"dashboardDocument":{"accessBinding":"b".repeat(64),"path":"c".repeat(64)}});
        assert_eq!(
            retention_projection(&value).unwrap(),
            json!({"snapshotId":"a".repeat(64)})
        );
    }

    #[test]
    fn bounded_pages_keep_exact_cursors_and_disclose_unavailable_entries() {
        let (_root, mut engine, binding, plan) = fixture();
        let receipt = apply(&mut engine, Some(binding.clone()), &plan);
        // Authored receipt fixtures exercise paging only; they do not assert
        // additional source applications or external executions.
        for n in 0..8 {
            let mut row = receipt.clone();
            row.idempotency_key = format!("owned-page-{n}");
            row.id = identity(&(&row.plan_id, &row.idempotency_key));
            record_journal(&engine.store, &row, Some(&binding)).unwrap();
        }
        let mut cursor = None;
        let mut observed = std::collections::BTreeSet::new();
        let mut finished = false;
        for _ in 0..10 {
            let page =
                document_journal_page(&engine.store, &binding, cursor.as_deref(), &d()).unwrap();
            let journals = page["journals"].as_array().unwrap();
            assert!(journals.len() <= 2);
            for row in journals {
                assert!(observed.insert(row["journalId"].as_str().unwrap().to_owned()));
                assert_eq!(row["allowedDirections"], json!(["rollback"]));
            }
            if page["nextCursor"].is_null() {
                finished = true;
                break;
            }
            assert_eq!(page["complete"], false);
            cursor = Some(page["nextCursor"].as_str().unwrap().to_owned());
        }
        assert!(finished);
        assert_eq!(observed.len(), 9);
        engine
            .store
            .record(
                "journal",
                &"f".repeat(64),
                &json!({"corrupt":"owned fixture"}),
            )
            .unwrap();
        let mut cursor = Some("e".repeat(64));
        let mut unavailable = 0;
        for _ in 0..10 {
            let page =
                document_journal_page(&engine.store, &binding, cursor.as_deref(), &d()).unwrap();
            assert_eq!(page["complete"], false);
            unavailable += page["unavailableEntries"].as_u64().unwrap();
            cursor = page["nextCursor"].as_str().map(str::to_owned);
            if cursor.is_none() {
                break;
            }
        }
        assert!(cursor.is_none());
        assert_eq!(unavailable, 1);
        assert!(document_journal_page(&engine.store, &binding, Some("../journal"), &d()).is_err());
    }

    #[test]
    fn held_writer_lock_refuses_reviewed_recovery_before_any_mutation() {
        let (root, mut engine, binding, plan) = fixture();
        let receipt = apply(&mut engine, Some(binding.clone()), &plan);
        let review = review_document_journal(&engine.store, &receipt.id, &binding, &d()).unwrap();
        let before = raw(&engine.store, &receipt.id);
        let _guard = super::super::lock(&engine.store).unwrap();
        let mut started = false;
        assert!(
            super::super::recover_document(
                &engine.store,
                root.path(),
                &review,
                true,
                &d(),
                &mut started
            )
            .is_err()
        );
        assert!(!started);
        assert_eq!(raw(&engine.store, &receipt.id), before);
        assert_eq!(
            fs::read(root.path().join("settings-π.json")).unwrap(),
            b"{\"n\":2}\r\n"
        );
    }
}
