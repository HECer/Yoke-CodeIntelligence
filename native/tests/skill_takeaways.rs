use serde_json::json;
use yoke::{
    capture::Deadline,
    skill_takeaways::{self, SkillAccess},
    store::Store,
};

#[test]
fn authored_skill_lifecycle_preserves_history_and_guarded_repository_bytes() {
    let personal = tempfile::tempdir().unwrap();
    let repository = tempfile::tempdir().unwrap();
    let store = Store::open(personal.path()).unwrap();
    let project = yoke::model::hash(b"explicit host project");
    let access = SkillAccess::new(&store, &project, repository.path(), true, true).unwrap();
    let d = Deadline::new(60000);
    let learned = skill_takeaways::invoke(&access,"learn",&json!({"skill":"PDF coords","text":"Use captured coordinates.\nKeep the source hash.","author":"Ada","date":"2026-10-15","expectedRevision":null}),&d).unwrap();
    assert_eq!(learned["skill"]["version"], 1);
    assert_eq!(learned["takeaway"]["skill"], "pdf-coords");
    assert!(std::path::Path::new(learned["takeaway"]["path"].as_str().unwrap()).is_file());
    let shown =
        skill_takeaways::invoke(&access, "show", &json!({"skill":"pdf-coords"}), &d).unwrap();
    assert!(
        shown["text"]
            .as_str()
            .unwrap()
            .contains("Corrections since v1")
    );
    let self_confirm = skill_takeaways::invoke(
        &access,
        "confirm",
        &json!({"skill":"pdf-coords","author":"ADA","expectedRevision":shown["revisionId"]}),
        &d,
    )
    .unwrap();
    assert_eq!(self_confirm["confirmed"].as_array().unwrap().len(), 0);
    let confirmed = skill_takeaways::invoke(
        &access,
        "confirm",
        &json!({"skill":"pdf-coords","author":"Grace","expectedRevision":shown["revisionId"]}),
        &d,
    )
    .unwrap();
    let folded = skill_takeaways::invoke(&access,"fold",&json!({"skill":"pdf-coords","confirmed":true,"expectedRevision":confirmed["revisionId"],"expectedFileHash":confirmed["contentHash"]}),&d).unwrap();
    assert_eq!(folded["skill"]["version"], 2);
    let published = skill_takeaways::invoke(&access,"publish",&json!({"skill":"pdf-coords","expectedRevision":folded["revisionId"],"expectedFileHash":folded["contentHash"]}),&d).unwrap();
    assert_eq!(published["skill"]["where"], "repo");
    let path = repository.path().join(".claude/skills/pdf-coords/SKILL.md");
    assert!(
        std::fs::read_to_string(&path)
            .unwrap()
            .contains("Keep the source hash.")
    );
    assert_eq!(published["skill"]["takeaways"][0]["foldedIn"], 2);
    assert_eq!(
        published["skill"]["takeaways"][0]["confirmedBy"],
        json!(["Grace"])
    );
    assert!(skill_takeaways::invoke(&access,"fold",&json!({"skill":"pdf-coords","expectedRevision":shown["revisionId"],"expectedFileHash":shown["contentHash"]}),&d).is_err());
    let retention = yoke::retention::plan(&store, &Default::default(), &d).unwrap();
    assert_eq!(
        retention["unknownDependencyKinds"],
        json!([]),
        "{retention}"
    );
}

#[test]
fn failed_publication_retains_a_recoverable_plan_and_refuses_foreign_file_changes() {
    let personal = tempfile::tempdir().unwrap();
    let repository = tempfile::tempdir().unwrap();
    let store = Store::open(personal.path()).unwrap();
    let project = yoke::model::hash(b"recovery project");
    let access = SkillAccess::new(&store, &project, repository.path(), true, true).unwrap();
    let d = Deadline::new(60000);
    store.db.execute_batch("CREATE TRIGGER reject_skill_head BEFORE INSERT ON records WHEN NEW.kind='native_skill_head_v1' BEGIN SELECT RAISE(ABORT,'intentional fixture interruption'); END;").unwrap();
    assert!(skill_takeaways::invoke(&access,"learn",&json!({"skill":"recoverable","text":"Keep the exact source.","author":"Ada","expectedRevision":null}),&d).is_err());
    assert!(skill_takeaways::invoke(&access, "show", &json!({"skill":"recoverable"}), &d).is_err());
    store
        .db
        .execute_batch("DROP TRIGGER reject_skill_head")
        .unwrap();
    let readonly = SkillAccess::new(&store, &project, repository.path(), false, false).unwrap();
    assert!(skill_takeaways::invoke(&readonly, "recover", &json!({}), &d).is_err());
    skill_takeaways::invoke(&access, "recover", &json!({}), &d).unwrap();
    let shown =
        skill_takeaways::invoke(&access, "show", &json!({"skill":"recoverable"}), &d).unwrap();
    assert_eq!(shown["skill"]["takeaways"].as_array().unwrap().len(), 1);
    assert!(
        shown["text"]
            .as_str()
            .unwrap()
            .contains("Keep the exact source.")
    );
}

#[test]
fn repo_frontmatter_is_preserved_and_readonly_discovery_creates_no_state() {
    let personal = tempfile::tempdir().unwrap();
    let repository = tempfile::tempdir().unwrap();
    let dir = repository.path().join(".claude/skills/existing");
    std::fs::create_dir_all(&dir).unwrap();
    let original = "---\nname: existing\n# preserved team comment\nlicense: MIT\nallowed-tools: [Read]\nversion: 7\ndescription: Team authored skill\n---\n\n# Existing\n\n## Coordinates\nOriginal line.\n\n## Later\nLater text.\n";
    std::fs::write(dir.join("SKILL.md"), original).unwrap();
    std::fs::write(
        dir.parent().unwrap().join("README.md"),
        "# Skill library notes\n",
    )
    .unwrap();
    let store = Store::open(personal.path()).unwrap();
    let project = yoke::model::hash(b"repo preexisting");
    let d = Deadline::new(60000);
    let readonly = SkillAccess::new(&store, &project, repository.path(), false, false).unwrap();
    let listed = skill_takeaways::invoke(&readonly, "list", &json!({}), &d).unwrap();
    assert_eq!(listed["count"], 1);
    let count: i64 = store
        .db
        .query_row("SELECT count(*) FROM records", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0);
    let access = SkillAccess::new(&store, &project, repository.path(), true, true).unwrap();
    let learned=skill_takeaways::invoke(&access,"learn",&json!({"skill":"existing","text":"Retain UTF-8 byte offsets.","section":"coordinates","author":"Ada","expectedRevision":listed["skills"][0]["revisionId"]}),&d).unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.join("SKILL.md")).unwrap(),
        original
    );
    let folded=skill_takeaways::invoke(&access,"fold",&json!({"skill":"existing","expectedRevision":learned["revisionId"],"expectedFileHash":learned["contentHash"]}),&d).unwrap();
    assert_eq!(folded["to"], 8);
    let bytes = std::fs::read_to_string(dir.join("SKILL.md")).unwrap();
    assert!(
        bytes.contains("# preserved team comment\nlicense: MIT\nallowed-tools: [Read]\nversion: 8")
    );
    assert!(bytes.find("Retain UTF-8").unwrap() < bytes.find("## Later").unwrap());
}

#[test]
fn fold_without_a_skill_selects_each_checked_skill_once() {
    let personal = tempfile::tempdir().unwrap();
    let repository = tempfile::tempdir().unwrap();
    let store = Store::open(personal.path()).unwrap();
    let project = yoke::model::hash(b"all selected skills");
    let access = SkillAccess::new(&store, &project, repository.path(), true, true).unwrap();
    let d = Deadline::new(60000);
    let mut revisions = serde_json::Map::new();
    let mut hashes = serde_json::Map::new();
    for name in ["first", "second"] {
        let learned=skill_takeaways::invoke(&access,"learn",&json!({"skill":name,"text":"Keep exact source bytes.","author":"Ada","expectedRevision":null}),&d).unwrap();
        revisions.insert(name.into(), learned["revisionId"].clone());
        hashes.insert(name.into(), learned["contentHash"].clone());
    }
    let folded = skill_takeaways::invoke(
        &access,
        "fold",
        &json!({"expectedRevisions":revisions,"expectedFileHashes":hashes}),
        &d,
    )
    .unwrap();
    assert_eq!(folded["results"].as_array().unwrap().len(), 2);
    for result in folded["results"].as_array().unwrap() {
        assert_eq!(result["from"], 1);
        assert_eq!(result["to"], 2);
    }
}

#[test]
fn a_successful_learning_never_makes_its_default_skill_view_unreadable() {
    let personal = tempfile::tempdir().unwrap();
    let repo = tempfile::tempdir().unwrap();
    let store = Store::open(personal.path()).unwrap();
    let project = yoke::model::hash(b"usable skill output");
    let access = SkillAccess::new(&store, &project, repo.path(), true, true).unwrap();
    let d = Deadline::new(180000);
    let text = "A precise authored correction. ".repeat(2000);
    assert!(text.len() <= 64000);
    let mut expected = serde_json::Value::Null;
    for index in 0..70 {
        let result = skill_takeaways::invoke(
            &access,
            "learn",
            &json!({"skill":"bounded","text":text,"author":"Ada","date":"2026-10-15","expectedRevision":expected}),
            &d,
        );
        match result {
            Ok(result) => {
                expected = result["revisionId"].clone();
                assert!(
                    skill_takeaways::invoke(&access, "show", &json!({"skill":"bounded"}), &d)
                        .is_ok(),
                    "Learning {index} succeeded but its promised default view is over capacity"
                );
            }
            Err(error) => {
                assert!(!expected.is_null(), "Initial fixture failed: {error:#}");
                let shown =
                    skill_takeaways::invoke(&access, "show", &json!({"skill":"bounded"}), &d)
                        .unwrap();
                assert_eq!(
                    shown["revisionId"], expected,
                    "Rejected mutation changed the durable skill"
                );
                return;
            }
        }
    }
    panic!("Fixture did not reach the documented output capacity");
}

#[test]
fn host_argument_preflight_is_readonly_and_never_overrides_explicit_expectations() {
    let personal = tempfile::tempdir().unwrap();
    let repo = tempfile::tempdir().unwrap();
    let store = Store::open(personal.path()).unwrap();
    let project = yoke::model::hash(b"readonly skill preflight");
    let access = SkillAccess::new(&store, &project, repo.path(), false, false).unwrap();
    let d = Deadline::new(60000);
    let prepared = skill_takeaways::prepare_arguments(
        &access,
        "learn",
        &json!({"skill":"new","text":"Exact bytes.","author":"Ada"}),
        &d,
    )
    .unwrap();
    assert_eq!(
        prepared.get("expectedRevision"),
        Some(&serde_json::Value::Null)
    );
    let explicit = json!({"skill":"new","text":"Exact bytes.","author":"Ada","expectedRevision":"1".repeat(64)});
    assert_eq!(
        skill_takeaways::prepare_arguments(&access, "learn", &explicit, &d).unwrap(),
        explicit
    );
    let maps = skill_takeaways::prepare_arguments(&access, "fold", &json!({}), &d).unwrap();
    assert_eq!(maps["expectedRevisions"], json!({}));
    assert_eq!(maps["expectedFileHashes"], json!({}));
    assert_eq!(
        store
            .db
            .query_row("SELECT count(*) FROM records", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert!(!repo.path().join(".claude").exists());
    assert!(!store.dir.join("skill-library").exists());
}
