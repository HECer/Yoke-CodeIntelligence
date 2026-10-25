//! Owned temporary Windows fixtures only; never invokes a host maintenance CLI.
use super::super::tests::Fixture;
use super::*;
use std::fs;

fn d() -> Deadline {
    Deadline::new(30_000)
}
struct PurgeFixture {
    f: Fixture,
    archive: HistoryArchivePlan,
    access: HistoryArchivePurgeAccess,
    plan: HistoryArchivePurgePlan,
}
impl PurgeFixture {
    fn new() -> Self {
        Self::from_fixture(Fixture::new())
    }
    fn from_fixture(f: Fixture) -> Self {
        let archive = f.plan();
        assert!(f.apply(&archive).unwrap().complete);
        Self::from_archive(f, archive)
    }
    fn from_archive(f: Fixture, archive: HistoryArchivePlan) -> Self {
        let access = HistoryArchivePurgeAccess::new(
            &f.from,
            &f.to,
            &archive,
            &f.archive.root,
            &f.a,
            &f.b,
            &d(),
        )
        .unwrap();
        let plan = plan_history_archive_purge(&f.from, &f.to, &access, &archive, &d()).unwrap();
        Self {
            f,
            archive,
            access,
            plan,
        }
    }
    fn permission(&self) -> HistoryArchivePurgePermission {
        HistoryArchivePurgePermission::new(true).unwrap()
    }
    fn apply(&self) -> Result<HistoryArchivePurgeReview> {
        let approval = self
            .access
            .approve(&self.f.from, &self.f.to, &self.plan, &d())?;
        apply_history_archive_purge(
            &self.f.from,
            &self.f.to,
            &self.access,
            &approval,
            &self.permission(),
            &self.plan,
            64000,
            &d(),
        )
    }
    fn inspect(&self) -> Result<HistoryArchivePurgeReview> {
        inspect_history_archive_purge(&self.f.from, &self.f.to, &self.access, &self.plan, &d())
    }
    fn recover(
        &self,
        event: &str,
        direction: RecoveryDirection,
    ) -> Result<HistoryArchivePurgeReview> {
        recover_history_archive_purge(
            &self.f.from,
            &self.f.to,
            &self.access,
            &self.permission(),
            &self.plan,
            event,
            direction,
            64000,
            &d(),
        )
    }
    fn interrupt(&self, checkpoint: &'static str) {
        self.interrupt_plan(&self.plan, checkpoint);
    }
    fn interrupt_plan(&self, plan: &HistoryArchivePurgePlan, checkpoint: &'static str) {
        let approval = self
            .access
            .approve(&self.f.from, &self.f.to, plan, &d())
            .unwrap();
        STOP_AFTER.with(|slot| *slot.borrow_mut() = Some(checkpoint));
        let result = apply_history_archive_purge(
            &self.f.from,
            &self.f.to,
            &self.access,
            &approval,
            &self.permission(),
            plan,
            64000,
            &d(),
        );
        assert!(result.err().unwrap().to_string().contains(checkpoint));
    }
    fn frame_path(&self) -> PathBuf {
        archive_path(&self.f.archive.root, &self.archive)
    }
    fn save(&self, text: &str) {
        let snapshot = self.f.to.read("new", &d()).unwrap();
        let p = self
            .f
            .to
            .plan("new", &snapshot.revision, text, &d())
            .unwrap();
        let approval = self.f.b.approve(&self.f.to, &p, &d()).unwrap();
        self.f.to.apply(&self.f.b, &approval, &p, &d()).unwrap();
    }
}

#[test]
fn purge_removes_only_exact_committed_frame_and_retains_physical_receipts() {
    let f = PurgeFixture::new();
    let source = fs::read(&f.f.target).unwrap();
    let untouched = fs::read(f.f.source.join("untouched.rs")).unwrap();
    let current = observe_current(&f.f.from, &f.f.to, &f.access.archive, &f.archive, &d()).unwrap();
    let retained = current
        .retained
        .iter()
        .map(|file| (file.path.clone(), file.bytes.clone(), file.identity.clone()))
        .collect::<Vec<_>>();
    drop(current);
    let owner_path = owner_path(&f.f.to.documents["new"]).unwrap();
    let owner = fs::read(&owner_path).unwrap();
    let foreign = f.f.archive.root.join("foreign-\u{130}.txt");
    fs::write(&foreign, b"untouched foreign bytes").unwrap();
    let result = f.apply().unwrap();
    assert!(
        result.complete
            && result.evidence_unavailable
            && !result.frame_present
            && !result.marker_present
    );
    assert_eq!(
        result.confirmed_removed_content_bytes,
        f.plan.frame.byte_length
    );
    assert_eq!(result.unverified_removal_bytes, 0);
    assert!(result.measured_filesystem_space_bytes.is_none());
    assert!(!result.secure_erasure_claimed && !result.file_changed && !result.source_written);
    assert!(!f.frame_path().exists());
    for (path, bytes, identity) in retained {
        let held = io::read_owned_file(&path, &[&identity], MAX_PLAN, false, &d())
            .unwrap()
            .unwrap();
        assert_eq!(held.bytes, bytes);
    }
    assert_eq!(fs::read(&owner_path).unwrap(), owner);
    assert_eq!(fs::read(&f.f.target).unwrap(), source);
    assert_eq!(
        fs::read(f.f.source.join("untouched.rs")).unwrap(),
        untouched
    );
    assert_eq!(fs::read(&foreign).unwrap(), b"untouched foreign bytes");
    let archive =
        inspect_history_archive(&f.f.from, &f.f.to, &f.f.archive, &f.archive, &d()).unwrap();
    assert!(
        archive.complete
            && !archive.immutable_archive_verified
            && !archive.archived_record_evidence_available
    );
    assert!(archive.purge.unwrap().complete);
    let error = read_history_archive_record(
        &f.f.from,
        &f.f.to,
        &f.f.archive,
        &f.archive,
        &f.archive.records[0].name,
        &d(),
    )
    .err()
    .unwrap();
    assert!(
        error
            .to_string()
            .contains("evidence unavailable after recorded physical purge")
    );
    assert!(f.apply().is_err());
    assert!(
        f.recover(&result.event_id, RecoveryDirection::Resume)
            .unwrap()
            .complete
    );
    f.save("\u{feff}# after physical purge\r\nvalue: explicit save\r\n");
    assert!(f.inspect().unwrap().complete);
}

#[test]
fn all_purge_crash_windows_require_current_event_review_and_preserve_attribution() {
    for stop in [
        "history_purge_prepared",
        "history_purge_marked",
        "history_purge_intent",
        "history_purge_before_unlink",
        "history_purge_unlinked",
        "history_purge_progress",
        "history_purge_committed",
        "history_purge_completed",
    ] {
        let f = PurgeFixture::new();
        let source = fs::read(&f.f.target).unwrap();
        f.interrupt(stop);
        let inspected = f.inspect().unwrap();
        assert_eq!(inspected.complete, stop == "history_purge_completed");
        let unverified = stop == "history_purge_unlinked";
        if unverified {
            assert_eq!(inspected.unverified_removal_bytes, f.plan.frame.byte_length);
        }
        if !matches!(stop, "history_purge_prepared" | "history_purge_completed") {
            assert!(f.f.from.history_archive_blocked("old", &d()).unwrap());
            assert!(f.f.to.history_archive_blocked("new", &d()).unwrap());
            assert!(f.f.to.owner_binding("new", &d()).is_err());
            assert!(plan_binding_migration(&f.f.to, &f.f.from, "new", "old", &d()).is_err());
        }
        assert!(
            f.recover(&"f".repeat(64), RecoveryDirection::Resume)
                .is_err()
        );
        let result = f
            .recover(&inspected.event_id, RecoveryDirection::Resume)
            .unwrap();
        assert!(result.complete && !result.marker_present && !result.frame_present);
        assert_eq!(
            result.unverified_removal_bytes,
            if unverified {
                f.plan.frame.byte_length
            } else {
                0
            }
        );
        assert_eq!(
            result.confirmed_removed_content_bytes,
            if unverified {
                0
            } else {
                f.plan.frame.byte_length
            }
        );
        assert_eq!(fs::read(&f.f.target).unwrap(), source);
    }
}

#[test]
fn purge_cancellation_is_preintent_only_and_retains_frame_with_fresh_review_possible() {
    for stop in ["history_purge_prepared", "history_purge_marked"] {
        let f = PurgeFixture::new();
        let bytes = fs::read(f.frame_path()).unwrap();
        f.interrupt(stop);
        let e = f.inspect().unwrap();
        let result = f.recover(&e.event_id, RecoveryDirection::Rollback).unwrap();
        assert!(result.complete && result.frame_verified && !result.evidence_unavailable);
        assert_eq!(result.phase, "cancelled");
        assert_eq!(
            result.confirmed_removed_content_bytes + result.unverified_removal_bytes,
            0
        );
        assert_eq!(fs::read(f.frame_path()).unwrap(), bytes);
        assert!(
            f.recover(&result.event_id, RecoveryDirection::Resume)
                .is_err()
        );
        let fresh =
            plan_history_archive_purge(&f.f.from, &f.f.to, &f.access, &f.archive, &d()).unwrap();
        assert_ne!(fresh.id, f.plan.id);
        assert!(f.access.approve(&f.f.from, &f.f.to, &fresh, &d()).is_ok());
    }
    let f = PurgeFixture::new();
    f.interrupt("history_purge_intent");
    let e = f.inspect().unwrap();
    assert!(f.recover(&e.event_id, RecoveryDirection::Rollback).is_err());
    assert!(f.frame_path().exists());
}

#[test]
fn retained_cancellation_survives_later_purge_and_its_pending_marker() {
    for stop in [
        "complete",
        "history_purge_marked",
        "history_purge_intent",
        "history_purge_unlinked",
        "history_purge_progress",
        "history_purge_committed",
        "history_purge_cancelled",
    ] {
        let f = PurgeFixture::new();
        f.interrupt("history_purge_marked");
        let initial = f.inspect().unwrap();
        let cancelled = f
            .recover(&initial.event_id, RecoveryDirection::Rollback)
            .unwrap();
        let later =
            plan_history_archive_purge(&f.f.from, &f.f.to, &f.access, &f.archive, &d()).unwrap();
        if stop == "complete" {
            let approval = f.access.approve(&f.f.from, &f.f.to, &later, &d()).unwrap();
            apply_history_archive_purge(
                &f.f.from,
                &f.f.to,
                &f.access,
                &approval,
                &f.permission(),
                &later,
                64000,
                &d(),
            )
            .unwrap();
        } else {
            f.interrupt_plan(
                &later,
                if stop == "history_purge_cancelled" {
                    "history_purge_marked"
                } else {
                    stop
                },
            );
            if stop == "history_purge_cancelled" {
                let current =
                    inspect_history_archive_purge(&f.f.from, &f.f.to, &f.access, &later, &d())
                        .unwrap();
                STOP_AFTER.with(|slot| *slot.borrow_mut() = Some("history_purge_cancelled"));
                assert!(
                    recover_history_archive_purge(
                        &f.f.from,
                        &f.f.to,
                        &f.access,
                        &f.permission(),
                        &later,
                        &current.event_id,
                        RecoveryDirection::Rollback,
                        64000,
                        &d(),
                    )
                    .err()
                    .unwrap()
                    .to_string()
                    .contains("history_purge_cancelled")
                );
            }
        }
        let current =
            inspect_history_archive_purge(&f.f.from, &f.f.to, &f.access, &later, &d()).unwrap();
        let old = f.inspect().unwrap();
        assert!(
            old.complete && old.phase == "cancelled" && !old.marker_present,
            "{stop}"
        );
        assert_eq!(old.confirmed_removed_content_bytes, 0);
        assert_eq!(old.unverified_removal_bytes, 0);
        assert_eq!(
            old.subsequent_purge_plan_id.as_deref(),
            Some(later.id.as_str())
        );
        assert_eq!(old.subsequent_purge_complete, Some(current.complete));
        assert_eq!(old.evidence_unavailable, current.evidence_unavailable);
        let marker = purge_marker_path(&f.f.from.documents["old"]).unwrap();
        let marker_before = marker.exists().then(|| fs::read(&marker).unwrap());
        let replay = f
            .recover(&cancelled.event_id, RecoveryDirection::Rollback)
            .unwrap();
        assert!(replay.complete);
        assert_eq!(
            marker_before,
            marker.exists().then(|| fs::read(&marker).unwrap())
        );
        let archive =
            inspect_history_archive(&f.f.from, &f.f.to, &f.f.archive, &f.archive, &d()).unwrap();
        assert_eq!(archive.complete, current.complete, "{stop}");
        if !current.complete {
            recover_history_archive_purge(
                &f.f.from,
                &f.f.to,
                &f.access,
                &f.permission(),
                &later,
                &current.event_id,
                if stop == "history_purge_cancelled" {
                    RecoveryDirection::Rollback
                } else {
                    RecoveryDirection::Resume
                },
                64000,
                &d(),
            )
            .unwrap();
        }
        let final_old = f.inspect().unwrap();
        assert!(final_old.complete && final_old.phase == "cancelled");
        assert_eq!(final_old.confirmed_removed_content_bytes, 0);
        assert_eq!(final_old.unverified_removal_bytes, 0);
        assert_eq!(
            final_old.evidence_unavailable,
            stop != "history_purge_cancelled"
        );
        if stop != "history_purge_cancelled" {
            assert_eq!(final_old.subsequent_purge_complete, Some(true));
        } else {
            assert!(final_old.subsequent_purge_plan_id.is_none());
        }
    }
}

#[test]
fn cancellation_does_not_explain_unrecorded_frame_loss() {
    let f = PurgeFixture::new();
    f.interrupt("history_purge_marked");
    let initial = f.inspect().unwrap();
    f.recover(&initial.event_id, RecoveryDirection::Rollback)
        .unwrap();
    fs::remove_file(f.frame_path()).unwrap();
    assert!(f.inspect().is_err());
    assert!(inspect_history_archive(&f.f.from, &f.f.to, &f.f.archive, &f.archive, &d(),).is_err());
}

#[test]
fn archive_approval_and_serialized_hashes_cannot_grant_purge_or_deletion_permission() {
    let f = PurgeFixture::new();
    assert!(HistoryArchivePurgePermission::new(false).is_err());
    assert!(
        f.f.archive
            .approve(&f.f.from, &f.f.to, &f.archive, &d())
            .is_err()
    );
    assert!(
        HistoryArchivePurgeAccess::new(
            &f.f.from,
            &f.f.to,
            &f.archive,
            &f.f.archive.root,
            &f.f.b,
            &f.f.a,
            &d()
        )
        .is_err()
    );
    let wrong = HistoryArchivePurgeApproval {
        plan_id: "a".repeat(64),
        archive_identity: f.plan.archive.archive_identity.clone(),
    };
    assert!(
        apply_history_archive_purge(
            &f.f.from,
            &f.f.to,
            &f.access,
            &wrong,
            &f.permission(),
            &f.plan,
            64000,
            &d()
        )
        .is_err()
    );
    let wrong_root = HistoryArchivePurgeApproval {
        plan_id: f.plan.id.clone(),
        archive_identity: "foreign".into(),
    };
    assert!(
        apply_history_archive_purge(
            &f.f.from,
            &f.f.to,
            &f.access,
            &wrong_root,
            &f.permission(),
            &f.plan,
            64000,
            &d()
        )
        .is_err()
    );
    assert!(f.frame_path().exists());
    assert!(
        !purge_marker_path(&f.f.from.documents["old"])
            .unwrap()
            .exists()
    );
}

#[test]
fn current_source_after_archive_can_be_freshly_reviewed_but_later_changes_refuse() {
    let mut f = PurgeFixture::new();
    f.save("\u{feff}# independently saved after archive\r\nvalue: \u{1f980}\r\n");
    f.plan = plan_history_archive_purge(&f.f.from, &f.f.to, &f.access, &f.archive, &d()).unwrap();
    assert_ne!(f.plan.source.sha256, f.archive.source_hash);
    f.access.approve(&f.f.from, &f.f.to, &f.plan, &d()).unwrap();
    f.save("# changed after purge review\n");
    assert!(f.apply().is_err());
    f.plan = plan_history_archive_purge(&f.f.from, &f.f.to, &f.access, &f.archive, &d()).unwrap();
    assert!(f.apply().unwrap().complete);
}

#[test]
fn stale_recovery_source_and_missing_blocker_refuse_after_intent() {
    let f = PurgeFixture::new();
    f.interrupt("history_purge_intent");
    let e = f.inspect().unwrap();
    let bytes = fs::read(&f.f.target).unwrap();
    fs::write(&f.f.target, b"foreign current source").unwrap();
    assert!(f.recover(&e.event_id, RecoveryDirection::Resume).is_err());
    assert!(f.frame_path().exists());
    fs::write(&f.f.target, bytes).unwrap();
    let marker = purge_marker_path(&f.f.from.documents["old"]).unwrap();
    fs::remove_file(&marker).unwrap();
    assert!(f.recover(&e.event_id, RecoveryDirection::Resume).is_err());
    assert!(f.frame_path().exists());
}

#[test]
fn frame_identity_replacement_and_postunlink_foreign_occupants_stay_untouched() {
    let f = PurgeFixture::new();
    let path = f.frame_path();
    let saved = f.f._temp.path().join("retained-original-frame.bin");
    let bytes = fs::read(&path).unwrap();
    fs::rename(&path, &saved).unwrap();
    fs::write(&path, &bytes).unwrap();
    assert!(f.apply().is_err());
    assert_eq!(fs::read(&path).unwrap(), bytes);
    let f = PurgeFixture::new();
    f.interrupt("history_purge_unlinked");
    let event = load_purge_event(&f.f.archive.root, &f.plan, &d()).unwrap();
    fs::write(f.frame_path(), b"foreign occupant after unlink").unwrap();
    assert!(f.inspect().is_err());
    assert!(f.recover(&event.id, RecoveryDirection::Resume).is_err());
    assert_eq!(
        fs::read(f.frame_path()).unwrap(),
        b"foreign occupant after unlink"
    );
}

#[test]
fn held_readers_and_hardlinks_refuse_delete_access_without_creating_intent() {
    let f = PurgeFixture::new();
    let held = validate_archive(&f.f.from, &f.f.archive.root, &f.archive, &d()).unwrap();
    assert!(f.apply().is_err());
    assert!(!plan_path(&f.f.archive.root, &purge_stem(&f.plan)).exists());
    drop(held);
    let link = f.f._temp.path().join("foreign-hard-link.bin");
    fs::hard_link(f.frame_path(), &link).unwrap();
    assert!(f.apply().is_err());
    assert!(f.frame_path().exists() && link.exists());
}

#[test]
fn root_replacement_overlap_traversal_ads_device_and_unicode_distinctions_refuse() {
    let f = PurgeFixture::new();
    for root in [
        f.f.source.clone(),
        f.f.from.journal.as_ref().unwrap().0.clone(),
        PathBuf::from("relative-archive"),
        f.f.archive.root.join(".."),
        f.f.archive.root.join("stream:archive"),
        PathBuf::from(r"\\.\NUL"),
    ] {
        assert!(
            HistoryArchivePurgeAccess::new(
                &f.f.from,
                &f.f.to,
                &f.archive,
                &root,
                &f.f.a,
                &f.f.b,
                &d()
            )
            .is_err()
        );
    }
    let composed = f.f._temp.path().join("archive-\u{e9}");
    let decomposed = f.f._temp.path().join("archive-e\u{301}");
    fs::create_dir(&composed).unwrap();
    fs::create_dir(&decomposed).unwrap();
    for root in [&composed, &decomposed] {
        assert!(
            HistoryArchivePurgeAccess::new(
                &f.f.from,
                &f.f.to,
                &f.archive,
                root,
                &f.f.a,
                &f.f.b,
                &d()
            )
            .is_err()
        );
    }
    let saved = f.f._temp.path().join("retained-archive-root");
    fs::rename(&f.f.archive.root, saved).unwrap();
    fs::create_dir(&f.f.archive.root).unwrap();
    assert!(f.apply().is_err());
}

#[test]
fn original_record_reappearance_unknown_names_and_occupied_roles_block_purge() {
    for role in ["original", "orphan", "backup", "output"] {
        let f = PurgeFixture::new();
        let original = &f
            .archive
            .records
            .iter()
            .find(|r| r.name.ends_with(".plan.json"))
            .unwrap()
            .name;
        let prefix = format!("yoke-doc-{}-", f.f.from.binding);
        let id = original
            .strip_prefix(&prefix)
            .unwrap()
            .strip_suffix(".plan.json")
            .unwrap();
        let selected = &f.f.from.documents["old"];
        let path = match role {
            "original" => f.f.from.journal.as_ref().unwrap().0.join(original),
            "orphan" => {
                f.f.from
                    .journal
                    .as_ref()
                    .unwrap()
                    .0
                    .join(format!("{prefix}{}.foreign.json", "b".repeat(64)))
            }
            "backup" => backup_path(selected, id),
            _ => output_path(selected, id, 2),
        };
        fs::write(&path, b"foreign relevant data").unwrap();
        assert!(f.apply().is_err(), "{role}");
        assert_eq!(fs::read(&path).unwrap(), b"foreign relevant data");
        assert!(f.frame_path().exists());
    }
}

#[test]
fn case_variant_relevant_orphans_after_approval_refuse_before_purge_publication() {
    for namespace in ["journal", "purge"] {
        for suffix in [".plan.json", ".e0001.json"] {
            let f = PurgeFixture::new();
            let approval = f.access.approve(&f.f.from, &f.f.to, &f.plan, &d()).unwrap();
            let (root, prefix) = if namespace == "journal" {
                (
                    f.f.from.journal.as_ref().unwrap().0.clone(),
                    format!("yoke-doc-{}-", f.f.from.binding),
                )
            } else {
                (f.f.archive.root.clone(), purge_prefix(&f.archive))
            };
            let foreign = root.join(format!(
                "{}{}{}",
                prefix.to_ascii_uppercase(),
                "a".repeat(64),
                suffix,
            ));
            fs::write(&foreign, b"foreign case-variant evidence").unwrap();
            let frame = fs::read(f.frame_path()).unwrap();
            assert!(
                apply_history_archive_purge(
                    &f.f.from,
                    &f.f.to,
                    &f.access,
                    &approval,
                    &f.permission(),
                    &f.plan,
                    64000,
                    &d(),
                )
                .is_err()
            );
            assert_eq!(fs::read(f.frame_path()).unwrap(), frame);
            assert_eq!(
                fs::read(&foreign).unwrap(),
                b"foreign case-variant evidence"
            );
            assert!(!plan_path(&f.f.archive.root, &purge_stem(&f.plan)).exists());
            assert!(
                !purge_marker_path(&f.f.from.documents["old"])
                    .unwrap()
                    .exists()
            );
        }
    }
    let f = PurgeFixture::new();
    f.interrupt("history_purge_marked");
    let initial = f.inspect().unwrap();
    let foreign = f.f.archive.root.join(format!(
        "{}.e0001.json",
        purge_stem(&f.plan).to_ascii_uppercase(),
    ));
    fs::write(&foreign, b"foreign selected-event spelling").unwrap();
    let frame = fs::read(f.frame_path()).unwrap();
    assert!(f.inspect().is_err());
    assert!(
        f.recover(&initial.event_id, RecoveryDirection::Resume)
            .is_err()
    );
    assert_eq!(fs::read(f.frame_path()).unwrap(), frame);
    assert_eq!(
        fs::read(foreign).unwrap(),
        b"foreign selected-event spelling"
    );
}

#[test]
fn wrong_frame_source_owner_security_generation_and_retained_witnesses_refuse() {
    let f = PurgeFixture::new();
    for field in 0..10 {
        let mut plan = f.plan.clone();
        match field {
            0 => plan.frame.identity.push('x'),
            1 => plan.frame.sha256 = "c".repeat(64),
            2 => plan.frame.security_sha256 = "d".repeat(64),
            3 => plan.frame.byte_length += 1,
            4 => plan.source.security_sha256 = "d".repeat(64),
            5 => plan.owner.identity.push('x'),
            6 => plan.owner.security_sha256 = "d".repeat(64),
            7 => plan.source_revision = "c".repeat(64),
            8 => plan.archive_event_id = "d".repeat(64),
            _ => plan.retained[0].file.identity.push('x'),
        }
        plan.id = purge_plan_id(&plan);
        assert!(
            f.access.approve(&f.f.from, &f.f.to, &plan, &d()).is_err(),
            "{field}"
        );
    }
    let reverse = plan_binding_migration(&f.f.to, &f.f.from, "new", "old", &d()).unwrap();
    let authority =
        BindingMigrationAuthority::new(&f.f.to, &f.f.from, "new", "old", &f.f.b, &f.f.a).unwrap();
    let approval = authority
        .approve(&f.f.to, &f.f.from, &reverse, &d())
        .unwrap();
    apply_binding_migration(&f.f.to, &f.f.from, &authority, &approval, &reverse, &d()).unwrap();
    assert!(f.apply().is_err());
    assert!(f.frame_path().exists());
}

#[test]
fn malformed_frame_lengths_truncation_trailing_and_noncanonical_manifest_refuse_planning() {
    let f = PurgeFixture::new();
    let original = fs::read(f.frame_path()).unwrap();
    let manifest_length =
        u32::from_le_bytes(original[MAGIC.len()..MAGIC.len() + 4].try_into().unwrap()) as usize;
    let manifest_start = MAGIC.len() + 4;
    let manifest_end = manifest_start + manifest_length;
    let mut variants = vec![
        original[..original.len() - 1].to_vec(),
        b"unsupported-frame".to_vec(),
    ];
    let mut trailing = original.clone();
    trailing.push(0);
    variants.push(trailing);
    let mut oversize = original.clone();
    oversize[MAGIC.len()..MAGIC.len() + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    variants.push(oversize);
    let mut manifest = original[manifest_start..manifest_end].to_vec();
    manifest.push(b' ');
    let mut noncanonical = MAGIC.to_vec();
    push_length(&mut noncanonical, manifest.len()).unwrap();
    noncanonical.extend(manifest);
    noncanonical.extend_from_slice(&original[manifest_end..]);
    variants.push(noncanonical);
    for bytes in variants {
        fs::write(f.frame_path(), &bytes).unwrap();
        assert!(
            plan_history_archive_purge(&f.f.from, &f.f.to, &f.access, &f.archive, &d()).is_err()
        );
        assert_eq!(fs::read(f.frame_path()).unwrap(), bytes);
    }
    fs::write(f.frame_path(), original).unwrap();
}

#[test]
fn forged_purge_event_gaps_lineage_and_unknown_output_never_authorize_unlink() {
    for mutation in ["gap", "lineage", "skip", "unknown"] {
        let f = PurgeFixture::new();
        f.interrupt("history_purge_prepared");
        let initial = purge_initial(&f.plan);
        let mut event = initial.clone();
        event.sequence = 1;
        event.previous = Some(initial.id.clone());
        event.phase = PurgePhase::Deleting;
        let path = match mutation {
            "gap" => event_path(&f.f.archive.root, &purge_stem(&f.plan), 2),
            "unknown" => {
                f.f.archive
                    .root
                    .join(format!("{}.foreign.json", purge_stem(&f.plan)))
            }
            _ => event_path(&f.f.archive.root, &purge_stem(&f.plan), 1),
        };
        if mutation == "lineage" {
            event.previous = Some("f".repeat(64));
        }
        if mutation == "skip" {
            event.phase = PurgePhase::Committed;
            event.removal = Removal::Confirmed;
        }
        event.id = purge_event_id(&event);
        fs::write(&path, serde_json::to_vec(&event).unwrap()).unwrap();
        assert!(f.inspect().is_err());
        assert!(f.recover(&event.id, RecoveryDirection::Resume).is_err());
        assert!(f.frame_path().exists());
    }
}

#[test]
fn purge_output_deadline_and_archive_entry_budgets_refuse_before_intent() {
    let f = PurgeFixture::new();
    let approval = f.access.approve(&f.f.from, &f.f.to, &f.plan, &d()).unwrap();
    assert!(
        apply_history_archive_purge(
            &f.f.from,
            &f.f.to,
            &f.access,
            &approval,
            &f.permission(),
            &f.plan,
            256,
            &d()
        )
        .is_err()
    );
    assert!(
        apply_history_archive_purge(
            &f.f.from,
            &f.f.to,
            &f.access,
            &approval,
            &f.permission(),
            &f.plan,
            64000,
            &Deadline::new(0)
        )
        .is_err()
    );
    let existing = fs::read_dir(&f.f.archive.root).unwrap().count();
    for i in existing..MAX_ARCHIVE_ENTRIES - 3 {
        fs::write(f.f.archive.root.join(format!("foreign-{i:04}")), b"x").unwrap();
    }
    assert!(f.apply().is_err());
    assert!(f.frame_path().exists());
    assert!(!plan_path(&f.f.archive.root, &purge_stem(&f.plan)).exists());
}

#[test]
fn purge_byte_budget_reserves_recovery_headroom_before_deletion() {
    let f = PurgeFixture::new();
    let used = fs::read_dir(&f.f.archive.root)
        .unwrap()
        .map(|e| e.unwrap().metadata().unwrap().len())
        .sum::<u64>();
    let filler = f.f.archive.root.join("foreign-byte-budget.bin");
    let file = fs::File::create(&filler).unwrap();
    file.set_len(MAX_ARCHIVE_ROOT_BYTES - used - MAX_EVENT as u64)
        .unwrap();
    drop(file);
    assert!(f.apply().is_err());
    assert!(f.frame_path().exists());
    assert!(!plan_path(&f.f.archive.root, &purge_stem(&f.plan)).exists());
    assert!(filler.exists());
}

#[test]
fn selected_sibling_archive_stays_exact_and_readable_after_single_frame_purge() {
    let mut base = Fixture::with_saves(6);
    base.archive = base
        .archive
        .with_record_budget(MAX_EVENTS as usize)
        .unwrap();
    let first = base.plan();
    assert!(first.unselected_closed_journals_at_review > 0);
    base.apply(&first).unwrap();
    let second = base.plan();
    base.apply(&second).unwrap();
    let second_path = archive_path(&base.archive.root, &second);
    let second_bytes = fs::read(&second_path).unwrap();
    let f = PurgeFixture::from_archive(base, first);
    assert!(f.apply().unwrap().complete);
    assert_eq!(fs::read(&second_path).unwrap(), second_bytes);
    for record in &second.records {
        let read = read_history_archive_record(
            &f.f.from,
            &f.f.to,
            &f.f.archive,
            &second,
            &record.name,
            &d(),
        )
        .unwrap();
        assert_eq!(read.sha256, record.hash);
    }
}

#[test]
fn cancelled_and_unfinished_archives_are_ineligible_and_unexplained_loss_stays_an_error() {
    let base = Fixture::new();
    let archive = base.plan();
    let access = HistoryArchivePurgeAccess::new(
        &base.from,
        &base.to,
        &archive,
        &base.archive.root,
        &base.a,
        &base.b,
        &d(),
    )
    .unwrap();
    assert!(plan_history_archive_purge(&base.from, &base.to, &access, &archive, &d()).is_err());
    let approval = base
        .archive
        .approve(&base.from, &base.to, &archive, &d())
        .unwrap();
    STOP_AFTER.with(|slot| *slot.borrow_mut() = Some("history_archive_marked"));
    assert!(
        apply_history_archive(
            &base.from,
            &base.to,
            &base.archive,
            &approval,
            &archive,
            &d()
        )
        .is_err()
    );
    let e = inspect_history_archive(&base.from, &base.to, &base.archive, &archive, &d()).unwrap();
    recover_history_archive(
        &base.from,
        &base.to,
        &base.archive,
        &archive,
        &e.event_id,
        RecoveryDirection::Rollback,
        &d(),
    )
    .unwrap();
    assert!(plan_history_archive_purge(&base.from, &base.to, &access, &archive, &d()).is_err());
    let f = PurgeFixture::new();
    fs::remove_file(f.frame_path()).unwrap();
    assert!(inspect_history_archive(&f.f.from, &f.f.to, &f.f.archive, &f.archive, &d()).is_err());
    assert!(f.apply().is_err());
}

#[test]
fn purge_receipts_cannot_hide_lost_or_replaced_retained_archive_evidence() {
    let f = PurgeFixture::new();
    f.apply().unwrap();
    let header = plan_path(&f.f.archive.root, &history_stem(&f.archive));
    let bytes = fs::read(&header).unwrap();
    fs::remove_file(&header).unwrap();
    fs::write(&header, &bytes).unwrap();
    assert!(f.inspect().is_err());
    assert!(inspect_history_archive(&f.f.from, &f.f.to, &f.f.archive, &f.archive, &d()).is_err());
    assert_eq!(fs::read(&header).unwrap(), bytes);
}

#[test]
fn real_private_reparse_frame_is_refused_without_symlink_privilege_or_link_following() -> Result<()>
{
    use std::os::windows::{
        fs::{MetadataExt as _, OpenOptionsExt as _},
        io::AsRawHandle as _,
    };
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn DeviceIoControl(
            handle: *mut std::ffi::c_void,
            code: u32,
            input: *mut std::ffi::c_void,
            input_size: u32,
            output: *mut std::ffi::c_void,
            output_size: u32,
            returned: *mut u32,
            overlapped: *mut std::ffi::c_void,
        ) -> i32;
    }
    fn tag(path: &Path, remove: bool) -> Result<()> {
        let handle = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(0)
            .custom_flags(0x0020_0000)
            .open(path)?;
        let mut input = [
            0x0000_0042u32,
            0,
            0x6f77_6e65,
            0x642d_6669,
            0x7874_7572,
            0x652d_696f,
        ];
        let mut returned = 0;
        ensure!(
            unsafe {
                DeviceIoControl(
                    handle.as_raw_handle(),
                    if remove { 0x0009_00ac } else { 0x0009_00a4 },
                    input.as_mut_ptr().cast(),
                    std::mem::size_of_val(&input) as u32,
                    std::ptr::null_mut(),
                    0,
                    &mut returned,
                    std::ptr::null_mut(),
                )
            } != 0,
            "Owned private reparse fixture failed: {}",
            std::io::Error::last_os_error()
        );
        Ok(())
    }
    let f = PurgeFixture::new();
    let path = f.frame_path();
    let original = fs::read(&path)?;
    tag(&path, false)?;
    let is_reparse = fs::symlink_metadata(&path)?.file_attributes() & 0x400 != 0;
    let refused = f.apply().is_err();
    tag(&path, true)?;
    assert!(is_reparse && refused);
    assert_eq!(fs::read(&path)?, original);
    assert!(!plan_path(&f.f.archive.root, &purge_stem(&f.plan)).exists());
    Ok(())
}

#[test]
fn remaining_event_capacity_is_reserved_for_reviewed_recovery() {
    let f = PurgeFixture::new();
    f.interrupt("history_purge_intent");
    let event = f.inspect().unwrap().event_id;
    let existing = fs::read_dir(&f.f.archive.root).unwrap().count();
    // Two progress/completion events plus one publication temporary fit exactly.
    for i in existing..MAX_ARCHIVE_ENTRIES - 3 {
        fs::write(
            f.f.archive.root.join(format!("foreign-recovery-{i:04}")),
            b"x",
        )
        .unwrap();
    }
    assert!(
        f.recover(&event, RecoveryDirection::Resume)
            .unwrap()
            .complete
    );
    assert_eq!(
        fs::read_dir(&f.f.archive.root).unwrap().count(),
        MAX_ARCHIVE_ENTRIES - 2
    );
}

#[test]
fn retained_archive_reads_do_not_require_live_owner_but_new_purge_does() {
    let f = PurgeFixture::new();
    let owner = owner_path(&f.f.to.documents["new"]).unwrap();
    let retained_owner = f.f._temp.path().join("retained-owner-evidence.json");
    fs::rename(&owner, &retained_owner).unwrap();
    let record = &f.archive.records[0];
    let read = read_history_archive_record(
        &f.f.from,
        &f.f.to,
        &f.f.archive,
        &f.archive,
        &record.name,
        &d(),
    )
    .unwrap();
    assert_eq!(read.sha256, record.hash);
    assert!(
        inspect_history_archive(&f.f.from, &f.f.to, &f.f.archive, &f.archive, &d())
            .unwrap()
            .complete
    );
    assert!(plan_history_archive_purge(&f.f.from, &f.f.to, &f.access, &f.archive, &d()).is_err());
    assert!(f.apply().is_err());
    assert!(f.frame_path().exists() && retained_owner.exists());
    fs::rename(&retained_owner, &owner).unwrap();
    f.apply().unwrap();
    fs::rename(&owner, &retained_owner).unwrap();
    fs::rename(
        &f.f.target,
        f.f._temp.path().join("retained-authored-document.yaml"),
    )
    .unwrap();
    assert!(f.inspect().unwrap().complete);
    assert!(
        inspect_history_archive(&f.f.from, &f.f.to, &f.f.archive, &f.archive, &d())
            .unwrap()
            .complete
    );
    let error = read_history_archive_record(
        &f.f.from,
        &f.f.to,
        &f.f.archive,
        &f.archive,
        &record.name,
        &d(),
    )
    .err()
    .unwrap();
    assert!(
        error
            .to_string()
            .contains("evidence unavailable after recorded physical purge")
    );
}
