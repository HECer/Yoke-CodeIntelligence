use serde_json::json;
use yoke::{
    capture::Deadline,
    model::hash,
    skill_takeaways::{self, SkillAccess},
    store::Store,
};

#[test]
fn predictable_later_fold_capacity_failure_is_admitted_before_the_first_skill_changes() {
    let personal = tempfile::tempdir().unwrap();
    let repository = tempfile::tempdir().unwrap();
    for (name, body) in [
        ("alpha", "# Small skill\n".to_string()),
        ("beta", format!("# Large skill\n{}\n", "x".repeat(970_000))),
    ] {
        let directory = repository.path().join(".claude/skills").join(name);
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("SKILL.md"), body).unwrap();
    }
    let store = Store::open(personal.path()).unwrap();
    let project = hash(b"batch fold preflight review");
    let access = SkillAccess::new(&store, &project, repository.path(), true, true).unwrap();
    let d = Deadline::new(60_000);
    let mut revisions = serde_json::Map::new();
    let mut hashes = serde_json::Map::new();
    for name in ["alpha", "beta"] {
        let shown = skill_takeaways::invoke(&access, "show", &json!({"skill":name}), &d).unwrap();
        let learned = skill_takeaways::invoke(&access,"learn",&json!({"skill":name,"text":if name=="beta" {"z".repeat(64_000)}else{"Keep the exact source hash.".into()},"author":"Ada","expectedRevision":shown["revisionId"]}),&d).unwrap();
        revisions.insert(name.into(), learned["revisionId"].clone());
        hashes.insert(name.into(), learned["contentHash"].clone());
    }
    let alpha_file = repository.path().join(".claude/skills/alpha/SKILL.md");
    let original = std::fs::read(&alpha_file).unwrap();
    let records_before: i64 = store
        .db
        .query_row("SELECT COUNT(*) FROM records", [], |row| row.get(0))
        .unwrap();
    assert!(
        skill_takeaways::invoke(
            &access,
            "fold",
            &json!({"expectedRevisions":revisions,"expectedFileHashes":hashes}),
            &d
        )
        .is_err()
    );
    assert_eq!(
        std::fs::read(alpha_file).unwrap(),
        original,
        "A predictable later input capacity error must fail before publishing the first fold"
    );
    assert_eq!(
        store
            .db
            .query_row("SELECT COUNT(*) FROM records", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        records_before
    );
    let alpha = skill_takeaways::invoke(&access, "show", &json!({"skill":"alpha"}), &d).unwrap();
    assert_eq!(alpha["revisionId"], revisions["alpha"]);
}
