use std::fs;
use yoke::{
    capture::Deadline,
    model::{Graph, Snapshot, SourceFile, identity},
    store::Store,
};

#[test]
fn snapshot_publication_rolls_back_when_document_indexing_expires() {
    let root = tempfile::tempdir().unwrap();
    let mut store = Store::open(root.path()).unwrap();
    let text = "word ".repeat(200000);
    let hash = store.blob(text.as_bytes()).unwrap();
    let files = (0..16)
        .map(|index| SourceFile {
            path: format!("doc{index}.md"),
            hash: hash.clone(),
            size: text.len(),
            language: "markdown".into(),
            mode: 0o644,
        })
        .collect::<Vec<_>>();
    let id = identity(&files);
    let snapshot = Snapshot {
        id: id.clone(),
        workspace_id: store.workspace_id.clone(),
        kind: "disk".into(),
        files,
        omissions: Vec::new(),
        capture_consistency: "stable-double-scan".into(),
    };
    let graph = Graph {
        snapshot_id: id.clone(),
        ..Default::default()
    };
    let deadline = Deadline::new(10);
    store.set_deadline(&deadline).unwrap();
    assert!(
        store.publish(&snapshot, &graph).is_err(),
        "Expired publication reported success"
    );
    store.clear_deadline();
    assert!(store.snapshot(&id).is_err());
    assert!(store.graph(&id).is_err());
    assert!(store.latest().unwrap().is_none());
    assert!(
        fs::read_dir(store.dir.join("blobs"))
            .unwrap()
            .next()
            .is_some()
    );
}
