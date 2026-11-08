use serde_json::json;
use yoke::{
    capture::Deadline,
    model::hash,
    skill_takeaways::{self, SkillAccess},
    store::Store,
};

#[test]
fn oversized_fold_is_rejected_before_publishing_an_unrecoverable_plan() {
    let personal = tempfile::tempdir().unwrap();
    let repository = tempfile::tempdir().unwrap();
    let dir = repository.path().join(".claude/skills/large");
    std::fs::create_dir_all(&dir).unwrap();
    let original = format!("# Existing\n{}\n", "x".repeat(970_000));
    std::fs::write(dir.join("SKILL.md"), &original).unwrap();
    let store = Store::open(personal.path()).unwrap();
    let project = hash(b"review bounded skill fold");
    let access = SkillAccess::new(&store, &project, repository.path(), true, true).unwrap();
    let d = Deadline::new(60_000);
    let shown = skill_takeaways::invoke(&access, "show", &json!({"skill":"large"}), &d).unwrap();
    let learned = skill_takeaways::invoke(&access,"learn",&json!({"skill":"large","text":"z".repeat(64_000),"author":"Ada","expectedRevision":shown["revisionId"]}),&d).unwrap();
    assert!(skill_takeaways::invoke(&access,"fold",&json!({"skill":"large","expectedRevision":learned["revisionId"],"expectedFileHash":learned["contentHash"]}),&d).is_err());
    assert_eq!(
        std::fs::read_to_string(dir.join("SKILL.md")).unwrap(),
        original
    );
    let still_readable = skill_takeaways::invoke(&access, "show", &json!({"skill":"large"}), &d);
    assert!(
        still_readable.is_ok(),
        "Rejected oversized fold published an unrecoverable pending operation: {still_readable:?}"
    );
    assert!(skill_takeaways::invoke(&access, "recover", &json!({}), &d).is_ok());
}

#[test]
fn deeply_nested_skill_frontmatter_is_admitted_before_yaml_materialization() {
    const CHILD: &str = "YOKE_REVIEW_SKILL_YAML_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "deeply_nested_skill_frontmatter_is_admitted_before_yaml_materialization",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "Unbounded skill YAML was materialized or crashed the child: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    let personal = tempfile::tempdir().unwrap();
    let repository = tempfile::tempdir().unwrap();
    let dir = repository.path().join(".claude/skills/nested");
    std::fs::create_dir_all(&dir).unwrap();
    let markdown = format!(
        "---\nextra: {}0{}\nversion: 1\n---\n# Skill\n",
        "[".repeat(20_000),
        "]".repeat(20_000)
    );
    std::fs::write(dir.join("SKILL.md"), markdown).unwrap();
    let store = Store::open(personal.path()).unwrap();
    let project = hash(b"review bounded skill YAML");
    let access = SkillAccess::new(&store, &project, repository.path(), false, false).unwrap();
    let result = skill_takeaways::invoke(
        &access,
        "show",
        &json!({"skill":"nested"}),
        &Deadline::new(5_000),
    );
    assert!(
        result.is_err(),
        "Unbounded nested YAML was accepted without structural admission"
    );
}

#[test]
fn resealed_pending_operation_cannot_redirect_the_revision_to_another_skill() {
    let personal = tempfile::tempdir().unwrap();
    let repository = tempfile::tempdir().unwrap();
    let store = Store::open(personal.path()).unwrap();
    let project = hash(b"review pending skill projection");
    let access = SkillAccess::new(&store, &project, repository.path(), true, true).unwrap();
    let d = Deadline::new(60_000);
    store.db.execute_batch("CREATE TRIGGER review_reject_skill_head BEFORE INSERT ON records WHEN NEW.kind='native_skill_head_v1' BEGIN SELECT RAISE(ABORT,'review interruption'); END;").unwrap();
    assert!(skill_takeaways::invoke(&access,"learn",&json!({"skill":"intended","text":"Keep a source hash.","author":"Ada","expectedRevision":null}),&d).is_err());
    store
        .db
        .execute_batch("DROP TRIGGER review_reject_skill_head")
        .unwrap();
    let (old_id, raw): (String, String) = store
        .db
        .query_row(
            "SELECT id,value FROM records WHERE kind='native_skill_operation_v1'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    let mut operation: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let artifact = store.artifact(b"# Unrelated replacement\n").unwrap();
    operation["files"] = json!([{"location":"personal","relative":"unrelated/SKILL.md","before":null,"after":artifact}]);
    // Preserve the typed Operation field order when resealing; object Value uses
    // sorted keys, whereas this receipt identity hashes the typed serialization.
    #[derive(serde::Serialize, serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Operation {
        schema_version: u8,
        project_id: String,
        repository_hash: String,
        name: String,
        before: Option<String>,
        after: String,
        files: Vec<FileChange>,
    }
    #[derive(serde::Serialize, serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct FileChange {
        location: String,
        relative: String,
        before: Option<String>,
        after: Option<String>,
    }
    let typed: Operation = serde_json::from_value(operation).unwrap();
    let resealed = serde_json::to_vec(&typed).unwrap();
    let id = hash(&resealed);
    store
        .db
        .execute(
            "DELETE FROM records WHERE kind='native_skill_operation_receipt_v1' AND id=?1",
            [&old_id],
        )
        .unwrap();
    store
        .record(skill_takeaways::OPERATION_KIND, &id, &typed)
        .unwrap();
    store
        .record(
            skill_takeaways::RECEIPT_KIND,
            &id,
            &json!({"schemaVersion":1,"operationId":id,"status":"prepared"}),
        )
        .unwrap();
    let recovered = skill_takeaways::invoke(&access, "recover", &json!({}), &d);
    let unintended = store
        .dir
        .join("skill-library")
        .join(&project)
        .join("unrelated/SKILL.md");
    assert!(
        recovered.is_err(),
        "Resealed operation with unrelated presentation was accepted: {recovered:?}"
    );
    assert!(
        !unintended.exists(),
        "Recovery wrote a file absent from its bound skill revision"
    );
}

#[test]
fn malformed_recovery_is_rejected_before_any_prepared_publication() {
    let personal = tempfile::tempdir().unwrap();
    let repository = tempfile::tempdir().unwrap();
    let store = Store::open(personal.path()).unwrap();
    let project = hash(b"review validate before recovery");
    let access = SkillAccess::new(&store, &project, repository.path(), true, true).unwrap();
    let d = Deadline::new(60_000);
    store.db.execute_batch("CREATE TRIGGER review_invalid_recovery BEFORE INSERT ON records WHEN NEW.kind='native_skill_head_v1' BEGIN SELECT RAISE(ABORT,'review interruption'); END;").unwrap();
    assert!(skill_takeaways::invoke(&access,"learn",&json!({"skill":"prepared","text":"Retain the source.","author":"Ada","expectedRevision":null}),&d).is_err());
    store
        .db
        .execute_batch("DROP TRIGGER review_invalid_recovery")
        .unwrap();
    let receipt_before: String = store
        .db
        .query_row(
            "SELECT value FROM records WHERE kind='native_skill_operation_receipt_v1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(skill_takeaways::invoke(&access, "recover", &json!({"unexpected":true}), &d).is_err());
    let receipt_after: String = store
        .db
        .query_row(
            "SELECT value FROM records WHERE kind='native_skill_operation_receipt_v1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        receipt_after, receipt_before,
        "Malformed recover applied a prepared operation before rejecting its arguments"
    );
    let heads: i64 = store
        .db
        .query_row(
            "SELECT count(*) FROM records WHERE kind='native_skill_head_v1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(heads, 0, "Malformed recover published a new skill head");
}
