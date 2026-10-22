use serde_json::{Value, json};
use std::{fs, path::Path};
use yoke::{
    Engine,
    capture::{self, CaptureOptions, Deadline},
    changes::{self, Plan},
    model::{hash, identity},
    project_session::{HostProfile, HostProject, Session},
    source_exclusions::{Group, Policy},
};
fn d() -> Deadline {
    Deadline::new(30000)
}
fn root() -> tempfile::TempDir {
    let r = tempfile::tempdir().unwrap();
    for dir in ["src/private", "src/blocked", "src/base-hidden", "private"] {
        fs::create_dir_all(r.path().join(dir)).unwrap();
    }
    for (path, text) in [
        ("main.rs", "// \u{1f4a1} e\u{301}\r\nfn main() {}\r\n"),
        ("src/keep.rs", "fn visible() {}\n"),
        ("private/keep.rs", "fn keep() {}\r\n"),
        ("private/drop.rs", "fn drop_me() {}\r\n"),
        ("src/private/hide.rs", "fn hide() {}\n"),
        ("src/blocked/hide.rs", "fn blocked() {}\n"),
        ("src/base-hidden/hide.rs", "fn base_hidden() {}\n"),
        (".env", "OWNED_FIXTURE=not-a-credential\n"),
    ] {
        fs::write(r.path().join(path), text).unwrap();
    }
    r
}
fn profile(v: Value) -> HostProfile {
    serde_json::from_value(v).unwrap()
}
fn engine(path: &Path) -> Engine {
    let mut e = Engine::open(path).unwrap();
    e.allow_writes = true;
    e
}
fn open(path: &Path, p: HostProfile, id: &str) -> Session {
    Session::open(engine(path), p, Some(id), &d()).unwrap()
}
fn prepared(s: &mut Session, path: &str) -> Plan {
    let value = s
        .invoke(
            "edit.prepare",
            &json!({"edits":[{"kind":"create","path":path,"text":"fn planned() {}\n"}]}),
            &d(),
        )
        .unwrap();
    assert_eq!(value["status"], "ok", "{value}");
    serde_json::from_value(value["data"]["plan"].clone()).unwrap()
}
fn verify(e: &Engine, p: &Plan, path: &str) {
    let script = format!(
        "const fs=require('node:fs');process.exit(fs.readFileSync({},'utf8')==='fn planned() {{}}\\n'?0:1)",
        serde_json::to_string(path).unwrap()
    );
    let r = yoke::verify::execute(e, &p.id, "node", &["-e".into(), script], true, &d()).unwrap();
    assert_eq!(r["status"], "passed", "{r}");
}

#[test]
fn absent_null_and_empty_profiles_keep_original_capture_identity_and_session_bytes() {
    let r = root();
    let old = HostProfile::default();
    let bytes = serde_json::to_vec(&old).unwrap();
    let mut s = open(r.path(), old, "legacy");
    let config = s.invoke("configuration", &json!({}), &d()).unwrap();
    let (snapshot, _, _, _) = s.engine.index(&d()).unwrap();
    assert_eq!(
        snapshot.id,
        identity(&(
            &s.engine.store.workspace_id,
            "disk",
            s.engine.capture_epoch.producer(),
            &snapshot.files,
            &snapshot.omissions
        ))
    );
    let plan = prepared(&mut s, "allowed.rs");
    let raw: String = s
        .engine
        .store
        .db
        .query_row(
            "select value from records where kind='plan' and id=?1",
            [&plan.id],
            |v| v.get(0),
        )
        .unwrap();
    drop(s);
    for value in [
        Value::Null,
        json!({}),
        json!({"global":null}),
        json!({"global":[]}),
        json!({"global":[[]]}),
        json!({"projects":{"default":{"project":[],"local":null}}}),
    ] {
        let p = profile(json!({"sourceExclusions":value}));
        assert_eq!(serde_json::to_vec(&p).unwrap(), bytes);
        let mut s = open(r.path(), p, "legacy");
        assert_eq!(s.invoke("configuration", &json!({}), &d()).unwrap(), config);
        assert_eq!(s.engine.index(&d()).unwrap().0.id, snapshot.id);
        let checked = changes::load_checked_plan(&s.engine.store, &plan.id, &d()).unwrap();
        assert_eq!(serde_json::to_string(&checked).unwrap(), raw);
    }
    let options: CaptureOptions =
        serde_json::from_value(json!({"sourceExclusions":{"kind":"gitignore_v1","groups":[]}}))
            .unwrap();
    assert_eq!(options.validated().unwrap(), CaptureOptions::default());
}

#[test]
fn negative_groups_filter_capture_read_and_creation_without_undoing_hard_exclusions() {
    let r = root();
    let p = profile(
        json!({"sourceExclusions":{"global":[["private/**","!private/keep.rs"]],"projects":{"default":{"local":[["!private/drop.rs","!.env"]]}}}}),
    );
    let mut s = open(r.path(), p, "selected");
    let (snapshot, _, _, _) = s.engine.index(&d()).unwrap();
    assert!(snapshot.files.iter().any(|f| f.path == "private/keep.rs"));
    assert!(
        !snapshot
            .files
            .iter()
            .any(|f| f.path == "private/drop.rs" || f.path == ".env")
    );
    assert!(
        snapshot
            .omissions
            .iter()
            .any(|o| o.path == "private/drop.rs" && o.reason == "host-source-exclusion")
    );
    let policy = capture::options_for_snapshot(&s.engine.store, &snapshot)
        .unwrap()
        .source_exclusions
        .unwrap();
    assert_eq!(policy.groups().len(), 2);
    assert!(
        policy
            .groups()
            .iter()
            .all(|g| g.case_insensitive == cfg!(windows))
    );
    let reply = s
        .invoke("fs.read", &json!({"path":"private/drop.rs"}), &d())
        .unwrap();
    assert_eq!(reply["status"], "error");
    for path in ["private/new.rs", ".env"] {
        let reply = s
            .invoke(
                "edit.prepare",
                &json!({"edits":[{"kind":"create","path":path,"text":"fn denied() {}\n"}]}),
                &d(),
            )
            .unwrap();
        assert_eq!(reply["status"], "error", "{reply}");
    }
    let p = prepared(&mut s, "allowed.rs");
    changes::load_checked_plan(&s.engine.store, &p.id, &d()).unwrap();
    assert!(!r.path().join("allowed.rs").exists());
    assert_eq!(
        fs::read_to_string(r.path().join("private/drop.rs")).unwrap(),
        "fn drop_me() {}\r\n"
    );
}

#[test]
fn existing_approved_verified_plan_cannot_cross_current_selection_but_history_remains_exact() {
    let r = root();
    let mut old = open(r.path(), HostProfile::default(), "old");
    let p = prepared(&mut old, "private/new.rs");
    let bytes = serde_json::to_vec(&p).unwrap();
    changes::approve(&old.engine.store, &p.id, false).unwrap();
    verify(&old.engine, &p, "private/new.rs");
    drop(old);
    let mut selected = open(
        r.path(),
        profile(json!({"sourceExclusions":{"global":[["private/**"]]}})),
        "selected",
    );
    let checked = changes::load_checked_plan(&selected.engine.store, &p.id, &d()).unwrap();
    assert_eq!(serde_json::to_vec(&checked).unwrap(), bytes);
    let before = selected.engine.store.db.total_changes();
    let err = changes::apply(
        &selected.engine.store,
        r.path(),
        &json!({"planId":p.id,"idempotencyKey":"blocked","mode":"workspace"}),
        true,
        &d(),
    )
    .err()
    .unwrap();
    assert!(
        err.to_string()
            .contains("source_excluded_by_current_host_selection"),
        "{err}"
    );
    assert_eq!(selected.engine.store.db.total_changes(), before);
    assert!(!r.path().join("private/new.rs").exists());
    let receipt = changes::apply(
        &selected.engine.store,
        r.path(),
        &json!({"planId":p.id,"idempotencyKey":"old-exact","mode":"isolated"}),
        false,
        &d(),
    )
    .unwrap();
    assert_eq!(
        fs::read(Path::new(receipt.sandbox_path.as_ref().unwrap()).join("private/new.rs")).unwrap(),
        b"fn planned() {}\n"
    );
    let reply=selected.invoke("edit.prepare",&json!({"snapshotId":p.base_snapshot_id,"edits":[{"kind":"create","path":"private/other.rs","text":"fn denied() {}\n"}]}),&d()).unwrap();
    assert_eq!(reply["status"], "error", "{reply}");
    let p = prepared(&mut selected, "src/new.rs");
    changes::approve(&selected.engine.store, &p.id, false).unwrap();
    verify(&selected.engine, &p, "src/new.rs");
    let receipt = changes::apply(
        &selected.engine.store,
        r.path(),
        &json!({"planId":p.id,"idempotencyKey":"allowed","mode":"workspace"}),
        true,
        &d(),
    )
    .unwrap();
    assert_eq!(
        fs::read(r.path().join("src/new.rs")).unwrap(),
        b"fn planned() {}\n"
    );
    changes::recover(&selected.engine.store, r.path(), &receipt.id, true).unwrap();
    assert!(!r.path().join("src/new.rs").exists());
}

#[test]
fn primary_project_filters_do_not_leak_into_independent_secondary_host_scope() {
    let primary = root();
    let secondary = root();
    let mut p = profile(
        json!({"sourceExclusions":{"global":[["src/blocked/**","!src/base-hidden/**"]],"projects":{"default":{"project":[["src/private/**"]],"local":[["!src/blocked/**"]]}}}}),
    );
    p.projects.push(HostProject {
        name: "secondary".into(),
        root: secondary.path().to_owned(),
        read_only: false,
        query_only: false,
    });
    let mut e = engine(primary.path());
    e.capture_options.only_dirs = vec!["src".into()];
    e.capture_options.source_exclusions = Some(Policy::GitignoreV1 {
        groups: vec![Group {
            patterns: vec!["src/base-hidden/**".into()],
            case_insensitive: false,
        }],
    });
    let mut s = Session::open(e, p, Some("projects"), &d()).unwrap();
    let (snapshot, _, _, _) = s.engine.index(&d()).unwrap();
    assert!(
        !snapshot
            .files
            .iter()
            .any(|f| f.path == "src/private/hide.rs")
    );
    s.invoke("activate", &json!({"project":"secondary"}), &d())
        .unwrap();
    let e = s.active_engine().unwrap();
    let (snapshot, _, _, _) = e.index(&d()).unwrap();
    assert!(
        snapshot
            .files
            .iter()
            .any(|f| f.path == "src/private/hide.rs")
    );
    assert!(
        !snapshot
            .files
            .iter()
            .any(|f| f.path == "src/base-hidden/hide.rs"
                || f.path == "src/blocked/hide.rs"
                || f.path == "main.rs")
    );
    prepared(&mut s, "src/private/new.rs");
    let reply = s
        .invoke(
            "edit.prepare",
            &json!({"edits":[{"kind":"create","path":"outside.rs","text":"fn denied() {}\n"}]}),
            &d(),
        )
        .unwrap();
    assert_eq!(reply["status"], "error");
    s.invoke("activate", &json!({"project":"default"}), &d())
        .unwrap();
    s.invoke("switchModes", &json!({"modes":["planning"]}), &d())
        .unwrap();
    assert!(s.invoke("edit.prepare", &json!({}), &d()).is_err());
    s.invoke("switchModes", &json!({"modes":["editing"]}), &d())
        .unwrap();
    let reply=s.invoke("edit.prepare",&json!({"edits":[{"kind":"create","path":"src/private/new.rs","text":"fn denied() {}\n"}]}),&d()).unwrap();
    assert_eq!(reply["status"], "error");
}

#[test]
fn unicode_exclusions_preserve_original_paths_and_resealed_false_membership_refuses() {
    let r = root();
    for name in ["caf\u{e9}.rs", "cafe\u{301}.rs"] {
        fs::write(
            r.path().join("src").join(name),
            "// \u{1f4a1}\r\nfn unicode() {}\r\n",
        )
        .unwrap();
    }
    let mut s = open(
        r.path(),
        profile(json!({"sourceExclusions":{"global":[["src/caf\u{e9}.rs"]]}})),
        "unicode",
    );
    let (snapshot, _, _, _) = s.engine.index(&d()).unwrap();
    assert!(
        snapshot
            .files
            .iter()
            .any(|f| f.path == "src/cafe\u{301}.rs")
    );
    assert!(!snapshot.files.iter().any(|f| f.path == "src/caf\u{e9}.rs"));
    let mut options = capture::options_for_snapshot(&s.engine.store, &snapshot).unwrap();
    options.source_exclusions = Some(Policy::GitignoreV1 {
        groups: vec![Group {
            patterns: vec!["**".into()],
            case_insensitive: false,
        }],
    });
    let options_hash = identity(&options);
    let mut forged = snapshot.clone();
    forged.id = identity(&(
        &s.engine.store.workspace_id,
        "disk",
        s.engine.capture_epoch.producer(),
        &forged.files,
        &forged.omissions,
        &options_hash,
    ));
    let record = capture::CaptureOptionsRecord {
        snapshot_id: forged.id.clone(),
        workspace_id: forged.workspace_id.clone(),
        analyzer: s.engine.capture_epoch.producer().into(),
        options_hash,
        options,
    };
    s.engine
        .store
        .record(capture::OPTIONS_RECORD_KIND, &forged.id, &record)
        .unwrap();
    assert!(capture::source_scope_policy(&s.engine.store, &forged).is_err());
    let raw = fs::read(r.path().join("src/cafe\u{301}.rs")).unwrap();
    assert_eq!(
        hash(raw),
        snapshot
            .files
            .iter()
            .find(|f| f.path == "src/cafe\u{301}.rs")
            .unwrap()
            .hash
    );
}

#[test]
fn invalid_profiles_and_configuration_changes_fail_without_granting_authority() {
    let r = root();
    for value in [
        json!({"global":["wrong-shape"]}),
        json!({"allowWrites":true}),
        json!({"global":[[true]]}),
    ] {
        assert!(serde_json::from_value::<HostProfile>(json!({"sourceExclusions":value})).is_err());
    }
    for value in [
        json!({"projects":{"unregistered":{"project":[["private/**"]]}}}),
        json!({"global":[["[bad"]]}),
    ] {
        assert!(
            Session::open(
                engine(r.path()),
                profile(json!({"sourceExclusions":value})),
                Some("bad"),
                &d()
            )
            .is_err()
        );
    }
    let p = profile(json!({"sourceExclusions":{"global":[["private/**"]]}}));
    let mut s = open(r.path(), p.clone(), "bound");
    let old = s.invoke("configuration", &json!({}), &d()).unwrap();
    drop(s);
    let mut resumed = open(r.path(), p, "bound");
    assert_eq!(
        resumed.invoke("configuration", &json!({}), &d()).unwrap(),
        old
    );
    drop(resumed);
    assert!(
        Session::open(
            engine(r.path()),
            HostProfile::default(),
            Some("bound"),
            &d()
        )
        .is_err()
    );
    let mut readonly = open(
        r.path(),
        profile(json!({"primaryReadOnly":true,"sourceExclusions":{"global":[["private/**"]]}})),
        "readonly",
    );
    let before = readonly.engine.store.db.total_changes();
    assert!(readonly.invoke("edit.prepare", &json!({}), &d()).is_err());
    assert_eq!(before, readonly.engine.store.db.total_changes());
}

#[test]
fn explicit_empty_selection_is_observable_and_never_reads_excluded_source_bytes() {
    let r = root();
    let mut s = open(
        r.path(),
        profile(json!({"sourceExclusions":{"global":[["**"]]}})),
        "empty",
    );
    let (snapshot, graph, _, _) = s.engine.index(&d()).unwrap();
    assert!(snapshot.files.is_empty());
    assert!(graph.entities.is_empty());
    assert!(
        snapshot
            .omissions
            .iter()
            .any(|o| o.reason == "host-source-exclusion")
    );
    assert!(
        capture::options_for_snapshot(&s.engine.store, &snapshot)
            .unwrap()
            .source_exclusions
            .is_some()
    );
    let reply = s
        .invoke("fs.read", &json!({"path":"main.rs"}), &d())
        .unwrap();
    assert_eq!(reply["status"], "error");
}
