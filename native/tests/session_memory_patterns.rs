use serde_json::{Value, json};
use std::{collections::BTreeSet, fs};
use tempfile::TempDir;
use yoke::{
    Engine,
    capture::Deadline,
    project_session::{HostProfile, Session},
};

fn root() -> TempDir {
    let r = TempDir::new().unwrap();
    fs::write(r.path().join("main.rs"), "// β 🌍\nfn source() {}\n").unwrap();
    r
}
fn host(mut v: Value, global: Option<&TempDir>) -> HostProfile {
    if let Some(g) = global {
        v["globalNotesRoot"] = json!(g.path());
    }
    serde_json::from_value(v).unwrap()
}
fn open(r: &TempDir, h: HostProfile, id: &str) -> Session {
    Session::open(
        Engine::open(r.path()).unwrap(),
        h,
        Some(id),
        &Deadline::new(30000),
    )
    .unwrap()
}
fn call(s: &mut Session, op: &str, args: Value) -> anyhow::Result<Value> {
    s.invoke(op, &args, &Deadline::new(30000))
}
fn facade(s: &mut Session, method: &str, args: Value) -> anyhow::Result<Value> {
    yoke::agent_repl::invoke_facade(s, method, args, None, &Deadline::new(30000))
}
fn seed(r: &TempDir, g: Option<&TempDir>, values: &[(&str, &str)]) {
    let mut s = open(r, host(json!({}), g), "seed");
    for (name, text) in values {
        facade(
            &mut s,
            "mem.write_memory",
            json!({"memory_name":name,"content":text}),
        )
        .unwrap();
    }
}
fn note_records(r: &TempDir) -> Vec<(String, String, String)> {
    let e = Engine::open(r.path()).unwrap();
    e.store.db.prepare("SELECT kind,id,value FROM records WHERE kind IN ('native_named_note_head_v1','native_named_note_revision_v1') ORDER BY kind,id").unwrap().query_map([],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap().collect::<Result<_,_>>().unwrap()
}
fn listed(s: &mut Session) -> Value {
    facade(s, "mem.list_memories", json!({})).unwrap()
}
fn names(v: &Value) -> BTreeSet<&str> {
    v["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["name"].as_str().unwrap())
        .collect()
}

#[test]
fn regex_readonly_and_hidden_rules_govern_real_listing_read_and_all_mutation_paths() {
    let r = root();
    seed(
        &r,
        None,
        &[
            ("archive/hidden", "hidden"),
            ("keep1", "choice"),
            ("keep12", "free"),
        ],
    );
    let mut s = open(
        &r,
        host(
            json!({"steering":{"global":{"ignoredMemoryPatterns":["^archive/"],"readOnlyMemoryPatterns":["^keep[0-9]$"]}}}),
            None,
        ),
        "restricted",
    );
    let list = listed(&mut s);
    assert_eq!(names(&list), BTreeSet::from(["keep1", "keep12"]));
    assert_eq!(
        facade(&mut s, "mem.read_memory", json!({"memory_name":"keep1"})).unwrap()["readOnly"],
        true
    );
    assert_eq!(
        facade(&mut s, "mem.read_memory", json!({"memory_name":"keep12"})).unwrap()["readOnly"],
        false
    );
    let before = note_records(&r);
    for (op, args, message) in [
        (
            "notes.read",
            json!({"name":"archive/hidden"}),
            "unavailable under host policy",
        ),
        (
            "notes.write",
            json!({"name":"keep1","content":"changed"}),
            "read-only",
        ),
        (
            "notes.edit",
            json!({"name":"keep1","needle":"choice","replacement":"changed"}),
            "read-only",
        ),
        ("notes.delete", json!({"name":"keep1"}), "read-only"),
        (
            "notes.rename",
            json!({"oldName":"keep1","newName":"free"}),
            "read-only",
        ),
        (
            "notes.rename",
            json!({"oldName":"keep12","newName":"keep2"}),
            "read-only",
        ),
        (
            "notes.rename",
            json!({"oldName":"keep12","newName":"archive/new"}),
            "unavailable under host policy",
        ),
        (
            "notes.write",
            json!({"name":"archive/new","content":"blocked"}),
            "unavailable under host policy",
        ),
    ] {
        assert!(
            call(&mut s, op, args)
                .unwrap_err()
                .to_string()
                .contains(message)
        );
        assert_eq!(note_records(&r), before);
    }
    assert!(
        yoke::compatibility::invoke_serena(
            &mut s,
            "read_memory",
            &json!({"memory_name":"archive/hidden"}),
            &Deadline::new(30000)
        )
        .unwrap_err()
        .to_string()
        .contains("unavailable under host policy")
    );
    call(
        &mut s,
        "notes.write",
        json!({"name":"new-note","content":"permitted"}),
    )
    .unwrap();
    assert_eq!(
        fs::read(r.path().join("main.rs")).unwrap(),
        "// β 🌍\nfn source() {}\n".as_bytes()
    );
}

#[test]
fn canonical_global_namespace_regex_is_distinct_from_project_names_and_legacy_globs() {
    let r = root();
    let g = root();
    seed(
        &r,
        Some(&g),
        &[
            ("keep1", "project"),
            ("keep12", "project-free"),
            ("global/keep1", "global"),
            ("global/keep12", "global-free"),
            ("global/old", "legacy glob"),
        ],
    );
    let h = host(
        json!({"notesPolicy":{"writable":true,"readOnlyPatterns":["global/old"]},"steering":{"global":{"readOnlyMemoryPatterns":["^keep[0-9]$"],"ignoredMemoryPatterns":["^global/keep[0-9]$"]}}}),
        Some(&g),
    );
    let mut s = open(&r, h, "namespaces");
    let l = listed(&mut s);
    assert!(!names(&l).contains("global/keep1"));
    assert!(names(&l).contains("global/keep12") && names(&l).contains("keep1"));
    assert_eq!(
        facade(&mut s, "mem.read_memory", json!({"memory_name":"keep1"})).unwrap()["readOnly"],
        true
    );
    assert_eq!(
        facade(
            &mut s,
            "mem.read_memory",
            json!({"memory_name":"global/keep12"})
        )
        .unwrap()["readOnly"],
        false
    );
    assert_eq!(
        facade(
            &mut s,
            "mem.read_memory",
            json!({"memory_name":"global/old"})
        )
        .unwrap()["readOnly"],
        true
    );
    assert!(
        facade(
            &mut s,
            "mem.read_memory",
            json!({"memory_name":"global/keep1"})
        )
        .unwrap_err()
        .to_string()
        .contains("unavailable under host policy")
    );
    let before = note_records(&g);
    assert!(
        facade(
            &mut s,
            "mem.delete_memory",
            json!({"memory_name":"global/old"})
        )
        .is_err()
    );
    assert_eq!(note_records(&g), before);
    let mut project_only = open(
        &r,
        host(
            json!({"steering":{"global":{"ignoredMemoryPatterns":["^keep[0-9]$"]}}}),
            Some(&g),
        ),
        "project-only",
    );
    let l = listed(&mut project_only);
    assert!(!names(&l).contains("keep1") && names(&l).contains("global/keep1"));
}

#[test]
fn all_six_layers_accumulate_and_follow_the_active_registered_project() {
    let r = root();
    let t = root();
    let values = [
        ("global/n", "v"),
        ("project/n", "v"),
        ("local/n", "v"),
        ("context/n", "v"),
        ("mode/n", "v"),
        ("cli/n", "v"),
        ("visible/n", "v"),
    ];
    // The native global/ prefix belongs only to the global namespace.
    let values: Vec<_> = values
        .into_iter()
        .map(|(n, v)| {
            if n == "global/n" {
                ("everywhere/n", v)
            } else {
                (n, v)
            }
        })
        .collect();
    seed(&r, None, &values);
    seed(&t, None, &values);
    let h = host(
        json!({"projects":[{"name":"target","root":t.path()}],"context":"custom-context","modes":["custom-mode"],"steering":{"global":{"ignoredMemoryPatterns":["^everywhere/"]},"projects":{"default":{"project":{"ignoredMemoryPatterns":["^project/"]},"projectLocal":{"ignoredMemoryPatterns":["^local/"]}}},"contexts":{"custom-context":{"ignoredMemoryPatterns":["^context/"]}},"modes":{"custom-mode":{"ignoredMemoryPatterns":["^mode/"]}},"cli":{"ignoredMemoryPatterns":["^cli/"]}}}),
        None,
    );
    let mut s = open(&r, h, "layers");
    assert_eq!(names(&listed(&mut s)), BTreeSet::from(["visible/n"]));
    let c = call(&mut s, "activate", json!({"project":"target"})).unwrap();
    assert_eq!(
        c["steering"]["ignoredMemoryPatterns"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    assert_eq!(
        names(&listed(&mut s)),
        BTreeSet::from(["local/n", "project/n", "visible/n"])
    );
    call(&mut s, "switchModes", json!({"modes":["editing"]})).unwrap();
    assert!(names(&listed(&mut s)).contains("mode/n"));
}

#[test]
fn invalid_and_oversized_regex_policies_refuse_before_session_publication() {
    let mut cases = vec![
        json!({"global":{"ignoredMemoryPatterns":["["]}}),
        json!({"global":{"readOnlyMemoryPatterns":["(?=a)"]}}),
        json!({"global":{"ignoredMemoryPatterns":["(a)\\1"]}}),
        json!({"global":{"ignoredMemoryPatterns":["a","a"]}}),
        json!({"global":{"ignoredMemoryPatterns":["a\0b"]}}),
        json!({"global":{"ignoredMemoryPatterns":["[a-z]{100000}"]}}),
        json!({"global":{"ignoredMemoryPatterns":["x".repeat(4097)]}}),
        json!({"modes":{"unused":{"ignoredMemoryPatterns":["["]}}}),
    ];
    cases.push(json!({"global":{"ignoredMemoryPatterns":(0..101).map(|i|format!("^n{i}$")).collect::<Vec<_>>()}}));
    cases.push(json!({"global":{"ignoredMemoryPatterns":(0..9).map(|i|format!("{}{}","a".repeat(4095),i)).collect::<Vec<_>>()}}));
    for steering in cases {
        let r = root();
        assert!(
            Session::open(
                Engine::open(r.path()).unwrap(),
                host(json!({"steering":steering}), None),
                Some("bad"),
                &Deadline::new(30000)
            )
            .is_err()
        );
        let e = Engine::open(r.path()).unwrap();
        let count: u64 = e
            .store
            .db
            .query_row(
                "SELECT count(*) FROM records WHERE kind=?1",
                [yoke::project_session::RECORD_KIND],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
        assert!(note_records(&r).is_empty());
    }
}

#[test]
fn combined_mode_policy_capacity_refuses_before_cas_and_preserves_effective_state() {
    let r = root();
    let h = host(
        json!({"steering":{"global":{"ignoredMemoryPatterns":(0..100).map(|i|format!("^n{i}$")).collect::<Vec<_>>()},"modes":{"extra":{"ignoredMemoryPatterns":["^extra$"]}}}}),
        None,
    );
    let mut s = open(&r, h, "capacity");
    let before = call(&mut s, "configuration", json!({})).unwrap();
    assert!(
        call(&mut s, "switchModes", json!({"modes":["extra"]}))
            .unwrap_err()
            .to_string()
            .contains("capacity")
    );
    assert_eq!(call(&mut s, "configuration", json!({})).unwrap(), before);
}

#[test]
fn unicode_regex_is_literal_metadata_and_empty_expression_intentionally_matches_all() {
    let r = root();
    seed(
        &r,
        None,
        &[("📚/eins", "one"), ("📚/zwei", "two"), ("📚/drei", "three")],
    );
    let mut s = open(
        &r,
        host(
            json!({"steering":{"global":{"ignoredMemoryPatterns":["^📚/(eins|zwei)$"]}}}),
            None,
        ),
        "unicode",
    );
    assert_eq!(names(&listed(&mut s)), BTreeSet::from(["📚/drei"]));
    let c = call(&mut s, "configuration", json!({})).unwrap();
    assert_eq!(
        c["steering"]["ignoredMemoryPatterns"],
        json!(["^📚/(eins|zwei)$"])
    );
    let mut all = open(
        &r,
        host(
            json!({"steering":{"global":{"ignoredMemoryPatterns":[""]}}}),
            None,
        ),
        "all",
    );
    assert!(names(&listed(&mut all)).is_empty());
    assert!(
        call(
            &mut all,
            "notes.write",
            json!({"name":"new","content":"refused"})
        )
        .unwrap_err()
        .to_string()
        .contains("unavailable under host policy")
    );
}

#[test]
fn regex_rename_preserves_hidden_and_readonly_revisions_and_independent_anchor_bytes() {
    let r = root();
    seed(
        &r,
        None,
        &[
            ("topic/a", "choice"),
            ("ref/free", "mem:topic/a"),
            ("lock1/ref", "mem:topic/a"),
            ("hidden/ref", "mem:topic/a"),
        ],
    );
    let mut control = open(&r, host(json!({}), None), "control");
    let original = facade(
        &mut control,
        "mem.read_memory",
        json!({"memory_name":"lock1/ref"}),
    )
    .unwrap();
    let hidden = facade(
        &mut control,
        "mem.read_memory",
        json!({"memory_name":"hidden/ref"}),
    )
    .unwrap();
    let mut s = open(
        &r,
        host(
            json!({"steering":{"global":{"readOnlyMemoryPatterns":["^lock[0-9]/"],"ignoredMemoryPatterns":["^hidden/"]}}}),
            None,
        ),
        "rename",
    );
    let result = call(
        &mut s,
        "notes.rename",
        json!({"oldName":"topic/a","newName":"topic/b"}),
    )
    .unwrap();
    assert_eq!(result["referencesUpdated"], 1);
    assert_eq!(result["readOnlyReferencesUnchanged"], json!(["lock1/ref"]));
    assert!(!result.to_string().contains("hidden/ref"));
    assert_eq!(
        facade(
            &mut control,
            "mem.read_memory",
            json!({"memory_name":"lock1/ref"})
        )
        .unwrap()["note"],
        original["note"]
    );
    assert_eq!(
        facade(
            &mut control,
            "mem.read_memory",
            json!({"memory_name":"hidden/ref"})
        )
        .unwrap()["note"],
        hidden["note"]
    );
    assert_eq!(
        facade(&mut s, "mem.read_memory", json!({"memory_name":"ref/free"})).unwrap()["note"]["content"],
        "mem:topic/b"
    );
}

#[test]
fn cached_python_callbacks_obey_in_cell_policy_changes_and_external_revision_checks() {
    use yoke::agent_repl::{AgentRepl, ReplSpec};
    let r = root();
    seed(&r, None, &[("keep1", "owned")]);
    let h = host(
        json!({"steering":{"modes":{"restricted":{"ignoredMemoryPatterns":["^keep1$"]}}}}),
        None,
    );
    let mut s = open(&r, h.clone(), "python");
    let output = std::process::Command::new("python")
        .args(["-I", "-c", "import sys;print(sys.executable)"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let spec = ReplSpec {
        python: std::path::PathBuf::from(String::from_utf8(output.stdout).unwrap().trim()),
        trust_python: true,
        max_output_bytes: 100000,
        shell: None,
    };
    let mut worker = AgentRepl::spawn(spec.clone(), &Deadline::new(30000)).unwrap();
    let v = worker
        .execute(
            &mut s,
            "answer=42\nsaved=s.mem.read_memory\nsaved('keep1')['note']['content']",
            &Deadline::new(30000),
        )
        .unwrap();
    assert_eq!(v["result"], "owned");
    let before = note_records(&r);
    let v = worker
        .execute(
            &mut s,
            "s.invoke('switchModes',{'modes':['restricted']})\nsaved('keep1')",
            &Deadline::new(30000),
        )
        .unwrap();
    assert_eq!(v["error"]["type"], "RuntimeError");
    assert!(
        v["error"]["message"]
            .as_str()
            .unwrap()
            .contains("unavailable under host policy")
    );
    assert_eq!(v["stateLost"], false);
    assert_eq!(note_records(&r), before);
    assert_eq!(
        worker
            .execute(&mut s, "answer", &Deadline::new(30000))
            .unwrap()["result"],
        42
    );
    let mut other = open(&r, h.clone(), "python");
    call(&mut other, "switchModes", json!({"modes":["editing"]})).unwrap();
    let output = TempDir::new().unwrap();
    let marker = output.path().join("stale-cell-must-not-run.txt");
    let code = format!(
        "open({},'w',encoding='utf8').write('unexpected')",
        serde_json::to_string(marker.to_str().unwrap()).unwrap()
    );
    assert!(
        worker
            .execute(&mut s, &code, &Deadline::new(30000))
            .unwrap_err()
            .to_string()
            .contains("concurrently")
    );
    assert!(!marker.exists());
    assert!(
        call(&mut s, "notes.read", json!({"name":"keep1"}))
            .unwrap_err()
            .to_string()
            .contains("concurrently")
    );
    let mut resumed = open(&r, h, "python");
    assert!(
        worker
            .execute(&mut resumed, "answer", &Deadline::new(30000))
            .unwrap_err()
            .to_string()
            .contains("state was lost")
    );
    worker.close().unwrap();
    let mut fresh = AgentRepl::spawn(spec, &Deadline::new(30000)).unwrap();
    assert_eq!(
        fresh
            .execute(
                &mut resumed,
                "s.mem.read_memory('keep1')['note']['content']",
                &Deadline::new(30000)
            )
            .unwrap()["result"],
        "owned"
    );
    fresh.close().unwrap();
}

#[test]
fn empty_regex_fields_preserve_legacy_globs_and_cannot_grant_note_or_source_authority() {
    let r = root();
    seed(
        &r,
        None,
        &[
            ("lock/a", "locked"),
            ("hidden/a", "hidden"),
            ("visible/a", "visible"),
        ],
    );
    let h = host(
        json!({"notesPolicy":{"writable":true,"readOnlyPatterns":["lock/**"],"ignoredPatterns":["hidden/**"]},"steering":{"global":{"readOnlyMemoryPatterns":[],"ignoredMemoryPatterns":[]}}}),
        None,
    );
    let serialized = serde_json::to_value(&h).unwrap();
    assert!(
        serialized["steering"]["global"]
            .get("readOnlyMemoryPatterns")
            .is_none()
    );
    let mut s = open(&r, h, "legacy");
    assert_eq!(
        names(&listed(&mut s)),
        BTreeSet::from(["lock/a", "visible/a"])
    );
    assert_eq!(
        facade(&mut s, "mem.read_memory", json!({"memory_name":"lock/a"})).unwrap()["readOnly"],
        true
    );
    assert!(
        call(
            &mut s,
            "notes.read",
            json!({"name":"hidden/a","ignoredMemoryPatterns":[]})
        )
        .is_err()
    );
    let mut denied = open(
        &r,
        host(
            json!({"notesPolicy":{"writable":false},"steering":{"global":{"readOnlyMemoryPatterns":[]}}}),
            None,
        ),
        "no-write",
    );
    assert!(
        call(
            &mut denied,
            "notes.write",
            json!({"name":"new","content":"not allowed"})
        )
        .unwrap_err()
        .to_string()
        .contains("read-only")
    );
    assert_eq!(
        fs::read(r.path().join("main.rs")).unwrap(),
        "// β 🌍\nfn source() {}\n".as_bytes()
    );
}
