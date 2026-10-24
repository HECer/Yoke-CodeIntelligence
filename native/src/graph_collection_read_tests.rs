use super::*;

fn fixture() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    drop(Registry::open(root.path()).unwrap());
    root
}

#[test]
fn coordinated_reader_pins_committed_wal_before_the_first_public_query() {
    let root = fixture();
    let writer = rusqlite::Connection::open(root.path().join("yoke-collection.sqlite")).unwrap();
    writer
        .execute("INSERT INTO source_blobs VALUES ('first', x'0102')", [])
        .unwrap();
    assert!(
        fs::metadata(root.path().join("yoke-collection.sqlite-wal"))
            .unwrap()
            .len()
            > 32
    );
    let d = Deadline::new(5000);
    let reader = Registry::open_coordinated_snapshot(root.path(), &d).unwrap();
    writer
        .execute("INSERT INTO source_blobs VALUES ('later', x'03')", [])
        .unwrap();
    // Admission itself must pin the transaction, not the first query below.
    assert_eq!(
        reader
            .db
            .query_row("SELECT count(*) FROM source_blobs", [], |r| r
                .get::<_, u64>(0))
            .unwrap(),
        1
    );
    let next = Registry::open_coordinated_snapshot(root.path(), &d).unwrap();
    assert_eq!(
        next.db
            .query_row("SELECT count(*) FROM source_blobs", [], |r| r
                .get::<_, u64>(0))
            .unwrap(),
        2
    );
    assert!(reader.db.is_readonly("main").unwrap());
    assert!(reader.db.execute("DELETE FROM source_blobs", []).is_err());
    assert!(reader.remove("absent", &"0".repeat(64), &d).is_err());
    drop(next);
    drop(reader);
    assert_eq!(
        writer
            .query_row(
                "SELECT hex(bytes) FROM source_blobs WHERE hash='first'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        "0102"
    );
}

#[test]
fn coordinated_admission_never_creates_a_database_or_initializes_an_unknown_schema() {
    let root = tempfile::tempdir().unwrap();
    let d = Deadline::new(5000);
    assert!(Registry::open_coordinated_snapshot(root.path(), &d).is_err());
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
    let path = root.path().join("yoke-collection.sqlite");
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("CREATE TABLE unrelated(bytes BLOB)")
        .unwrap();
    drop(db);
    let before = fs::read(&path).unwrap();
    assert!(Registry::open_coordinated_snapshot(root.path(), &d).is_err());
    assert_eq!(fs::read(&path).unwrap(), before);
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
}

#[test]
fn canceled_and_expired_coordinated_admission_leave_the_directory_empty() {
    let root = tempfile::tempdir().unwrap();
    assert!(Registry::open_coordinated_snapshot(root.path(), &Deadline::new(0)).is_err());
    let canceled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let result = Deadline::run_bounded(&Deadline::new(5000), canceled, || {
        Registry::open_coordinated_snapshot(root.path(), &Deadline::new(5000))
    });
    assert!(result.is_err());
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
}

#[test]
fn coordinated_schema_reads_obey_the_host_lock_budget() {
    let root = fixture();
    let writer = rusqlite::Connection::open(root.path().join("yoke-collection.sqlite")).unwrap();
    writer
        .execute_batch("PRAGMA journal_mode=DELETE; BEGIN EXCLUSIVE")
        .unwrap();
    let start = std::time::Instant::now();
    assert!(Registry::open_coordinated_snapshot(root.path(), &Deadline::new(30)).is_err());
    assert!(start.elapsed() < Duration::from_secs(1));
    writer.execute_batch("ROLLBACK").unwrap();
    assert!(Registry::open_coordinated_snapshot(root.path(), &Deadline::new(5000)).is_ok());
}

#[test]
fn coordinated_reader_treats_unicode_and_uri_punctuation_as_path_bytes() {
    let parent = tempfile::tempdir().unwrap();
    let root = parent.path().join("registry café # = %");
    fs::create_dir(&root).unwrap();
    drop(Registry::open(&root).unwrap());
    let registry = Registry::open_coordinated_snapshot(&root, &Deadline::new(5000)).unwrap();
    assert_eq!(registry.list(&Deadline::new(5000)).unwrap()["count"], 0);
}

#[cfg(unix)]
#[test]
fn unix_public_admission_requires_explicit_coordination_permission() {
    let root = fixture();
    let d = Deadline::new(5000);
    assert!(Registry::open_existing_for_host(root.path(), false, &d).is_err());
    let reader = Registry::open_existing_for_host(root.path(), true, &d).unwrap();
    assert_eq!(reader.list(&d).unwrap()["count"], 0);
    assert!(reader.remove("missing", &"0".repeat(64), &d).is_err());
}

#[cfg(unix)]
#[test]
fn coordinated_reader_refuses_database_sidecar_and_ancestor_links() {
    use std::os::unix::fs::symlink;
    let d = Deadline::new(5000);
    let root = fixture();
    let alias = tempfile::tempdir().unwrap();
    let path = root.path().join("yoke-collection.sqlite");
    let before = fs::read(&path).unwrap();
    symlink(&path, alias.path().join("yoke-collection.sqlite")).unwrap();
    assert!(Registry::open_coordinated_snapshot(alias.path(), &d).is_err());
    let ancestor = alias.path().join("ancestor");
    symlink(root.path(), &ancestor).unwrap();
    assert!(Registry::open_coordinated_snapshot(&ancestor, &d).is_err());
    let target = alias.path().join("ordinary");
    fs::write(&target, b"do not modify").unwrap();
    let sidecar = root.path().join("yoke-collection.sqlite-shm");
    symlink(&target, &sidecar).unwrap();
    assert!(Registry::open_coordinated_snapshot(root.path(), &d).is_err());
    assert_eq!(fs::read(&target).unwrap(), b"do not modify");
    assert_eq!(fs::read(&path).unwrap(), before);
}

#[cfg(unix)]
#[test]
fn coordinated_reader_refuses_hardlink_aliases_that_can_hide_wal_state() {
    let root = fixture();
    let other = tempfile::tempdir().unwrap();
    let path = root.path().join("yoke-collection.sqlite");
    let alias = other.path().join("yoke-collection.sqlite");
    let before = fs::read(&path).unwrap();
    fs::hard_link(&path, &alias).unwrap();
    assert!(Registry::open_coordinated_snapshot(root.path(), &Deadline::new(5000)).is_err());
    assert_eq!(fs::read(&alias).unwrap(), before);
    fs::remove_file(&alias).unwrap();
    let sidecar = root.path().join("yoke-collection.sqlite-shm");
    fs::write(&sidecar, b"unchanged").unwrap();
    fs::hard_link(&sidecar, other.path().join("shm-alias")).unwrap();
    assert!(Registry::open_coordinated_snapshot(root.path(), &Deadline::new(5000)).is_err());
    assert_eq!(fs::read(&sidecar).unwrap(), b"unchanged");
}
