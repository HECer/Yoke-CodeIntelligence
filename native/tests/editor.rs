use serde_json::json;
use std::fs;
use tempfile::TempDir;
use yoke::{Engine, capture::Deadline, editor};

fn fixture() -> (TempDir, Engine) {
    let root = TempDir::new().unwrap();
    fs::write(root.path().join("main.rs"), "fn saved() {}\n").unwrap();
    let engine = Engine::open(root.path()).unwrap();
    (root, engine)
}
#[test]
fn editor_buffers_are_versioned_source_bound_and_never_write_saved_files() {
    let (root, mut engine) = fixture();
    let session = editor::open(&mut engine, &Deadline::new(10000)).unwrap();
    let input = json!({"path":"main.rs","version":1,"text":"// 🌍\nfn unsaved() {}\n"});
    let first = editor::update(&engine, &session.id, &input, &Deadline::new(10000)).unwrap();
    let again = editor::update(&engine, &session.id, &input, &Deadline::new(10000)).unwrap();
    assert_eq!(first["revision"], again["revision"]);
    assert_eq!(again["idempotent"], true);
    assert!(
        editor::update(
            &engine,
            &session.id,
            &json!({"path":"main.rs","version":1,"text":"different"}),
            &Deadline::new(10000)
        )
        .is_err()
    );
    let snapshot = editor::capture(&mut engine, &session.id, &Deadline::new(10000)).unwrap();
    assert_eq!(snapshot.kind, "overlay");
    let found = engine
        .invoke(
            "ci_find",
            json!({"snapshotId":snapshot.id,"query":"unsaved"}),
        )
        .unwrap();
    assert_eq!(found["data"]["items"][0]["name"], "unsaved");
    assert_eq!(
        fs::read_to_string(root.path().join("main.rs")).unwrap(),
        "fn saved() {}\n"
    );
    editor::close_file(
        &engine.store,
        &session.id,
        "main.rs",
        Some(1),
        &Deadline::new(10000),
    )
    .unwrap();
    let closed = editor::capture(&mut engine, &session.id, &Deadline::new(10000)).unwrap();
    assert_eq!(
        engine
            .invoke("ci_read", json!({"snapshotId":closed.id,"path":"main.rs"}))
            .unwrap()["data"]["text"],
        "fn saved() {}\n"
    );
    assert_eq!(
        engine
            .invoke(
                "ci_read",
                json!({"snapshotId":snapshot.id,"path":"main.rs"})
            )
            .unwrap()["data"]["text"],
        "// 🌍\nfn unsaved() {}\n"
    );
}

#[test]
fn editor_capture_refuses_foreign_disk_changes_and_accepts_a_real_saved_buffer() {
    let (root, mut engine) = fixture();
    let session = editor::open(&mut engine, &Deadline::new(10000)).unwrap();
    let text = "fn unsaved() {}\n";
    editor::update(
        &engine,
        &session.id,
        &json!({"path":"main.rs","version":2,"text":text}),
        &Deadline::new(10000),
    )
    .unwrap();
    fs::write(root.path().join("main.rs"), "fn foreign() {}\n").unwrap();
    assert!(editor::capture(&mut engine, &session.id, &Deadline::new(10000)).is_err());
    assert_eq!(
        fs::read_to_string(root.path().join("main.rs")).unwrap(),
        "fn foreign() {}\n"
    );
    fs::write(root.path().join("main.rs"), text).unwrap();
    editor::capture(&mut engine, &session.id, &Deadline::new(10000)).unwrap();
    editor::update(
        &engine,
        &session.id,
        &json!({"path":"main.rs","version":3,"expectedVersion":2,"text":"fn next() {}\n"}),
        &Deadline::new(10000),
    )
    .unwrap();
    assert!(
        editor::close_file(
            &engine.store,
            &session.id,
            "main.rs",
            Some(2),
            &Deadline::new(10000)
        )
        .is_err()
    );
    assert!(editor::capture(&mut engine, &session.id, &Deadline::new(10000)).is_ok());
}

#[test]
fn editor_cannot_restore_omitted_ignored_protected_or_secret_source() {
    let (root, mut engine) = fixture();
    fs::write(root.path().join(".gitignore"), "hidden/**\n").unwrap();
    fs::write(root.path().join("binary.dat"), [0, 1, 2]).unwrap();
    let session = editor::open(&mut engine, &Deadline::new(10000)).unwrap();
    for path in [
        "../escape.rs",
        ".env",
        "binary.dat",
        "hidden/new.rs",
        "target/new.rs",
    ] {
        assert!(
            editor::update(
                &engine,
                &session.id,
                &json!({"path":path,"version":1,"text":"fn injected() {}\n"}),
                &Deadline::new(10000)
            )
            .is_err(),
            "Overlay bypass: {path}"
        );
    }
    let token = format!("ghp_{}", "b".repeat(36));
    assert!(
        editor::update(
            &engine,
            &session.id,
            &json!({"path":"new.rs","version":1,"text":token}),
            &Deadline::new(10000)
        )
        .is_err()
    );
    editor::update(
        &engine,
        &session.id,
        &json!({"path":"new.rs","version":1,"text":"fn new_file() {}\n"}),
        &Deadline::new(10000),
    )
    .unwrap();
    let overlay = editor::capture(&mut engine, &session.id, &Deadline::new(10000)).unwrap();
    assert!(overlay.files.iter().any(|f| f.path == "new.rs"));
    assert!(!root.path().join("new.rs").exists());
}

#[test]
fn nested_ignore_policies_are_scoped_and_git_excludes_cannot_be_bypassed() {
    let (root, mut engine) = fixture();
    fs::create_dir(root.path().join("src")).unwrap();
    fs::write(root.path().join("src/.gitignore"), "/hidden.rs\n").unwrap();
    fs::create_dir_all(root.path().join(".git/info")).unwrap();
    fs::write(root.path().join(".git/info/exclude"), "excluded.rs\n").unwrap();
    let session = editor::open(&mut engine, &Deadline::new(10000)).unwrap();
    for path in ["src/hidden.rs", "excluded.rs"] {
        assert!(
            editor::update(
                &engine,
                &session.id,
                &json!({"path":path,"version":1,"text":"fn hidden(){}"}),
                &Deadline::new(10000)
            )
            .is_err()
        );
    }
    assert!(
        editor::update(
            &engine,
            &session.id,
            &json!({"path":"hidden.rs","version":1,"text":"fn allowed(){}"}),
            &Deadline::new(10000)
        )
        .is_ok()
    );
}

#[test]
fn ancestor_ignore_policies_also_protect_unsaved_new_files() {
    for name in [".ignore", ".gitignore"] {
        let parent = TempDir::new().unwrap();
        let root = parent.path().join("workspace");
        fs::create_dir(&root).unwrap();
        fs::write(parent.path().join(name), "private.rs\n").unwrap();
        fs::write(root.join("main.rs"), "fn saved() {}\n").unwrap();
        fs::write(root.join("private.rs"), "fn ignored() {}\n").unwrap();
        let mut engine = Engine::open(&root).unwrap();
        let (disk, _, _, _) = engine.index(&Deadline::new(10000)).unwrap();
        assert!(!disk.files.iter().any(|file| file.path == "private.rs"));
        fs::remove_file(root.join("private.rs")).unwrap();
        let session = editor::open(&mut engine, &Deadline::new(10000)).unwrap();
        assert!(
            editor::update(
                &engine,
                &session.id,
                &json!({"path":"private.rs","version":1,"text":"fn unsaved_ignored() {}\n"}),
                &Deadline::new(10000)
            )
            .is_err(),
            "Ancestor {name} was bypassed"
        );
    }
}

#[test]
fn effective_ignore_whitelists_apply_equally_to_saved_and_unsaved_new_paths() {
    let (root, mut engine) = fixture();
    fs::create_dir(root.path().join("src")).unwrap();
    fs::write(root.path().join(".gitignore"), "src/*.rs\n").unwrap();
    fs::write(root.path().join("src/.gitignore"), "!allowed.rs\n").unwrap();
    fs::write(root.path().join("src/allowed.rs"), "fn allowed(){}\n").unwrap();
    let (saved, _, _, _) = engine.index(&Deadline::new(10000)).unwrap();
    assert!(saved.files.iter().any(|file| file.path == "src/allowed.rs"));
    fs::remove_file(root.path().join("src/allowed.rs")).unwrap();
    let session = editor::open(&mut engine, &Deadline::new(10000)).unwrap();
    editor::update(
        &engine,
        &session.id,
        &json!({"path":"src/allowed.rs","version":1,"text":"fn whitelisted(){}\n"}),
        &Deadline::new(10000),
    )
    .unwrap();
    let overlay = editor::capture(&mut engine, &session.id, &Deadline::new(10000)).unwrap();
    assert!(
        overlay
            .files
            .iter()
            .any(|file| file.path == "src/allowed.rs")
    );
    assert!(
        editor::update(
            &engine,
            &session.id,
            &json!({"path":"src/private.rs","version":1,"text":"fn denied(){}\n"}),
            &Deadline::new(10000)
        )
        .is_err()
    );
}
