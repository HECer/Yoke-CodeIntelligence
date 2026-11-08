use serde_json::{Value, json};
use yoke::{
    capture::Deadline,
    skill_takeaways::{self, SkillAccess},
    store::Store,
};

fn roots() -> (tempfile::TempDir, tempfile::TempDir, Store, String) {
    let personal = tempfile::tempdir().unwrap();
    let repository = tempfile::tempdir().unwrap();
    let store = Store::open(personal.path()).unwrap();
    (
        personal,
        repository,
        store,
        yoke::model::hash(b"independent skill contract fixture"),
    )
}
fn learn(
    access: &SkillAccess<'_>,
    author: &str,
    date: &str,
    text: &str,
    revision: Value,
    deadline: &Deadline,
) -> Value {
    skill_takeaways::invoke(access,"learn",&json!({"skill":"contract","author":author,"date":date,"text":text,"expectedRevision":revision}),deadline).unwrap()
}

#[test]
fn confirm_accepts_an_existing_takeaway_filename_suffix() {
    let (_personal, repository, store, project) = roots();
    let access = SkillAccess::new(&store, &project, repository.path(), true, true).unwrap();
    let deadline = Deadline::new(20000);
    let learned = learn(
        &access,
        "Ada",
        "2026-10-15",
        "First rule.",
        Value::Null,
        &deadline,
    );
    let confirmed = skill_takeaways::invoke(&access,"confirm",&json!({"skill":"contract","author":"Grace","takeaway":"ada-1","expectedRevision":learned["revisionId"]}),&deadline).unwrap();
    assert_eq!(
        confirmed["confirmed"].as_array().unwrap().len(),
        1,
        "Pinned selector matches a known filename suffix: {confirmed}"
    );
}

#[test]
fn confirm_accepts_the_known_absolute_takeaway_path_without_arbitrary_file_authority() {
    let (_personal, repository, store, project) = roots();
    let access = SkillAccess::new(&store, &project, repository.path(), true, true).unwrap();
    let deadline = Deadline::new(20000);
    let learned = learn(
        &access,
        "Ada",
        "2026-10-15",
        "First rule.",
        Value::Null,
        &deadline,
    );
    let file = store
        .dir
        .join("skill-library")
        .join(project)
        .join("contract/takeaways/2026-10-15-ada-1.md");
    assert!(file.is_file());
    let confirmed = skill_takeaways::invoke(&access,"confirm",&json!({"skill":"contract","author":"Grace","takeaway":file,"expectedRevision":learned["revisionId"]}),&deadline).unwrap();
    assert_eq!(confirmed["confirmed"].as_array().unwrap().len(), 1);
    let foreign = tempfile::tempdir().unwrap();
    let unknown = foreign.path().join("unregistered.md");
    std::fs::write(&unknown, "Keep these unrelated bytes.").unwrap();
    let unselected = skill_takeaways::invoke(&access,"confirm",&json!({"skill":"contract","author":"Another","takeaway":unknown,"expectedRevision":confirmed["revisionId"]}),&deadline).unwrap();
    assert_eq!(unselected["confirmed"], json!([]));
    assert_eq!(
        std::fs::read_to_string(unknown).unwrap(),
        "Keep these unrelated bytes."
    );
}

#[test]
fn unicode_lowercase_teacher_and_duplicate_confirmations_are_noops() {
    let (_personal, repository, store, project) = roots();
    let access = SkillAccess::new(&store, &project, repository.path(), true, true).unwrap();
    let deadline = Deadline::new(20000);
    let learned = learn(
        &access,
        "ÄDA",
        "2026-10-15",
        "First rule.",
        Value::Null,
        &deadline,
    );
    let own = skill_takeaways::invoke(
        &access,
        "confirm",
        &json!({"skill":"contract","author":"äda","expectedRevision":learned["revisionId"]}),
        &deadline,
    )
    .unwrap();
    assert_eq!(own["confirmed"], json!([]));
    assert_eq!(own["revisionId"], learned["revisionId"]);
    let confirmed = skill_takeaways::invoke(
        &access,
        "confirm",
        &json!({"skill":"contract","author":"GrÄCE","expectedRevision":own["revisionId"]}),
        &deadline,
    )
    .unwrap();
    assert_eq!(confirmed["confirmed"].as_array().unwrap().len(), 1);
    let duplicate = skill_takeaways::invoke(
        &access,
        "confirm",
        &json!({"skill":"contract","author":"gräce","expectedRevision":confirmed["revisionId"]}),
        &deadline,
    )
    .unwrap();
    assert_eq!(duplicate["confirmed"], json!([]));
    assert_eq!(duplicate["revisionId"], confirmed["revisionId"]);
}

#[test]
fn confirmed_only_fold_with_no_ready_takeaways_keeps_version_revision_and_empty_sections() {
    let (_personal, repository, store, project) = roots();
    let access = SkillAccess::new(&store, &project, repository.path(), true, true).unwrap();
    let deadline = Deadline::new(20000);
    let learned = learn(
        &access,
        "Ada",
        "2026-10-15",
        "First rule.",
        Value::Null,
        &deadline,
    );
    let before = std::fs::read(learned["skill"]["file"].as_str().unwrap()).unwrap();
    let folded = skill_takeaways::invoke(&access,"fold",&json!({"skill":"contract","confirmed":true,"expectedRevision":learned["revisionId"],"expectedFileHash":learned["contentHash"]}),&deadline).unwrap();
    assert_eq!(folded["from"], 1);
    assert_eq!(folded["to"], 1);
    assert_eq!(folded["skill"]["version"], 1);
    assert_eq!(folded["revisionId"], learned["revisionId"]);
    assert_eq!(folded["folded"], json!([]));
    assert_eq!(folded["sections"], json!([]));
    assert_eq!(folded["stillWaiting"], 1);
    assert_eq!(
        std::fs::read(folded["skill"]["file"].as_str().unwrap()).unwrap(),
        before
    );
}

#[test]
fn show_orders_waiting_takeaways_by_newest_date_then_filename_and_renders_short_dates() {
    let (_personal, repository, store, project) = roots();
    let access = SkillAccess::new(&store, &project, repository.path(), true, true).unwrap();
    let deadline = Deadline::new(20000);
    let first = learn(
        &access,
        "Ada",
        "2026-10-14",
        "oldest rule",
        Value::Null,
        &deadline,
    );
    let second = learn(
        &access,
        "Ada",
        "2026-10-15",
        "earlier same-date rule",
        first["revisionId"].clone(),
        &deadline,
    );
    learn(
        &access,
        "Ada",
        "2026-10-15",
        "newest same-date rule",
        second["revisionId"].clone(),
        &deadline,
    );
    let shown =
        skill_takeaways::invoke(&access, "show", &json!({"skill":"contract"}), &deadline).unwrap();
    let text = shown["text"].as_str().unwrap();
    assert!(
        text.find("newest same-date rule").unwrap() < text.find("earlier same-date rule").unwrap()
    );
    assert!(text.find("earlier same-date rule").unwrap() < text.find("oldest rule").unwrap());
    assert!(
        text.contains("(Ada, Oct 15)"),
        "Pinned shortDate rendering missing: {text}"
    );
}

#[test]
fn unknown_confirm_and_fold_succeed_without_creating_a_skill_or_operation() {
    let (_personal, repository, store, project) = roots();
    let access = SkillAccess::new(&store, &project, repository.path(), true, true).unwrap();
    let deadline = Deadline::new(20000);
    for (operation, arguments) in [
        (
            "confirm",
            json!({"skill":"unknown","author":"Ada","expectedRevision":null}),
        ),
        (
            "fold",
            json!({"skill":"unknown","expectedRevision":null,"expectedFileHash":"0".repeat(64)}),
        ),
    ] {
        assert!(
            skill_takeaways::invoke(&access, operation, &arguments, &deadline).is_ok(),
            "Unknown {operation} should be a successful no-op"
        );
    }
    let count: i64 = store
        .db
        .query_row(
            "SELECT COUNT(*) FROM records WHERE kind LIKE 'native_skill_%'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);
    assert!(!repository.path().join(".claude").exists());
    assert!(!store.dir.join("skill-library").exists());
}

#[test]
fn folded_result_takeaways_report_the_same_version_as_the_published_state() {
    let (_personal, repository, store, project) = roots();
    let access = SkillAccess::new(&store, &project, repository.path(), true, true).unwrap();
    let deadline = Deadline::new(20000);
    let learned = learn(
        &access,
        "Ada",
        "2026-10-15",
        "First rule.",
        Value::Null,
        &deadline,
    );
    let folded = skill_takeaways::invoke(&access,"fold",&json!({"skill":"contract","expectedRevision":learned["revisionId"],"expectedFileHash":learned["contentHash"]}),&deadline).unwrap();
    assert_eq!(folded["to"], 2);
    assert_eq!(
        folded["folded"][0]["foldedIn"], folded["to"],
        "Returned folded facts must match the new revision: {folded}"
    );
    assert_eq!(folded["skill"]["takeaways"][0]["foldedIn"], folded["to"]);
}
