//! Draft genuine-source capacity integration, loaded INSIDE retrieval_scopes.
//! Every heavy case is ignored until root explicitly qualifies its job.
//! Setup failure is never credited as a capacity RED or a passing skip.

use super::*;
use crate::Engine;
use rusqlite::hooks::{AuthAction, AuthContext, Authorization};
use serde::Serialize;
use serde_json::json;
use std::{
    fs,
    io::Write,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
use tempfile::TempDir;

fn write(root: &Path, path: &str, text: &str) {
    let full = root.join(path);
    fs::create_dir_all(full.parent().unwrap()).unwrap();
    fs::write(full, text).unwrap();
}
fn base36(mut index: usize) -> String {
    let alphabet = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut bytes = *b"0000";
    for slot in bytes.iter_mut().rev() {
        *slot = alphabet[index % 36];
        index /= 36;
    }
    assert_eq!(index, 0);
    format!("x{}", std::str::from_utf8(&bytes).unwrap())
}
struct Count(usize);
impl Write for Count {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0 = self
            .0
            .checked_add(bytes.len())
            .ok_or_else(|| std::io::Error::other("test byte overflow"))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn json_size<T: Serialize>(value: &T) -> usize {
    let mut count = Count(0);
    serde_json::to_writer(&mut count, value).unwrap();
    count.0
}
fn hard<T>(result: RetrievalResult<T>) -> RetrievalHardError {
    match result {
        Err(e) => e,
        Ok(_) => panic!("expected typed capacity, got success"),
    }
}
fn with_graph_reads<T>(store: &Store, deny: bool, run: impl FnOnce() -> T) -> (T, usize) {
    let reads = Arc::new(AtomicUsize::new(0));
    let hook_reads = reads.clone();
    store.db.authorizer(Some(move |context: AuthContext<'_>| {
        if let AuthAction::Read {
            table_name: "graphs",
            ..
        } = context.action
        {
            hook_reads.fetch_add(1, Ordering::SeqCst);
            if deny {
                return Authorization::Deny;
            }
        }
        Authorization::Allow
    }));
    struct Clear<'a>(&'a rusqlite::Connection);
    impl Drop for Clear<'_> {
        fn drop(&mut self) {
            self.0
                .authorizer(None::<fn(AuthContext<'_>) -> Authorization>);
        }
    }
    let clear = Clear(&store.db);
    let value = run();
    drop(clear);
    (value, reads.load(Ordering::SeqCst))
}
fn rename_owned(root: &Path, from: &str, to: &str) {
    let canonical_root = root.canonicalize().unwrap();
    let from = root.join(from);
    let to = root.join(to);
    assert!(
        from.parent()
            .unwrap()
            .canonicalize()
            .unwrap()
            .starts_with(&canonical_root)
    );
    assert!(
        to.parent()
            .unwrap()
            .canonicalize()
            .unwrap()
            .starts_with(&canonical_root)
    );
    fs::rename(from, to).unwrap();
}

#[test]
fn graph_read_tripwire_has_a_real_saved_positive_control() {
    let root = TempDir::new().unwrap();
    write(root.path(), "a/package.json", r#"{"name":"a"}"#);
    write(root.path(), "b/package.json", r#"{"name":"b"}"#);
    let mut engine = Engine::open(root.path()).unwrap();
    let d = Deadline::new(30_000);
    let (snapshot, graph, _, _) = engine.index(&d).unwrap();
    let (result, reads): (RetrievalResult<()>, usize) =
        with_graph_reads(&engine.store, false, || {
            let mut run = begin(&engine.store, &snapshot, &graph, &d, 4_000_000)?;
            match probe(&mut run)? {
                ProbeOutcome::Ready(_) => Ok(()),
                _ => panic!("two real roots"),
            }
        });
    result.unwrap();
    assert!(
        reads > 0,
        "tripwire must observe real stored-graph verification"
    );
}

#[test]
#[ignore = "qualified root job:100K real files, estimated<=2GB RAM/3GB disk; no SLA"]
fn real_100k_files_pass_then_one_real_function_rejects_entity_100001_before_graph_reload() {
    let root = TempDir::new().unwrap();
    for i in 0..99_999 {
        write(root.path(), &base36(i), "");
    }
    write(root.path(), "p.rs", "");
    let mut engine = Engine::open(root.path()).unwrap();
    let d = Deadline::new(600_000);
    let (snapshot, graph, _, _) = engine.index(&d).unwrap();
    assert_eq!(snapshot.files.len(), 100_000);
    assert_eq!(graph.entities.len(), 100_000);
    assert!(graph.observations.is_empty());
    assert!(
        json_size(&graph) < 64_000_000,
        "byte cap must not dominate this control"
    );
    let (result, reads) = with_graph_reads(&engine.store, false, || {
        let mut run = begin(&engine.store, &snapshot, &graph, &d, 4_000_000)?;
        assert!(matches!(probe(&mut run)?, ProbeOutcome::NotApplicable(_)));
        Ok::<_, RetrievalHardError>(())
    });
    result.unwrap();
    assert!(reads > 0);
    drop(graph);
    drop(snapshot);

    write(root.path(), "p.rs", "fn extra(){}\n");
    let (snapshot, graph, _, _) = engine.index(&d).unwrap();
    assert_eq!(snapshot.files.len(), 100_000);
    assert_eq!(
        graph.entities.len(),
        100_001,
        "actual Rust extraction, not forged graph metadata"
    );
    assert!(json_size(&graph) < 64_000_000);
    let (result, reads) = with_graph_reads(&engine.store, true, || {
        begin(&engine.store, &snapshot, &graph, &d, 4_000_000)
    });
    assert_eq!(hard(result), RetrievalHardError::Capacity("graph_entities"));
    assert_eq!(
        reads, 0,
        "count refusal must precede extra stored-graph load"
    );
    let before: usize = engine
        .store
        .db
        .query_row("SELECT COUNT(*) FROM snapshots", [], |r| r.get(0))
        .unwrap();
    drop(graph);
    drop(snapshot);

    // A genuine100001-path native snapshot cannot be constructed: existing
    // capture refuses it. This is upstream qualification, not a typed retrieval
    // guard receipt or permission to manufacture a saved manifest.
    write(root.path(), "zzzzz", "");
    let error = engine.index(&d).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("Snapshot capture budget exceeded"),
        "{error:#}"
    );
    let after: usize = engine
        .store
        .db
        .query_row("SELECT COUNT(*) FROM snapshots", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        after, before,
        "failed native capture cannot publish a new snapshot"
    );
}

fn sized_path(index: usize, bytes: usize) -> String {
    let path = format!("{}/{}", "d".repeat(bytes - 6), base36(index));
    assert_eq!(path.len(), bytes);
    path
}

#[test]
#[ignore = "qualified root job:64K real files/two64MB graphs, estimated<=2GB RAM/3GB disk"]
fn real_graph_exact_64mb_passes_and_plus_one_refuses_before_saved_graph_loading() {
    // Independently calibrate one genuine empty-file entity and graph header.
    // No authority fields/IDs/sizes are modified. The scalable fixture is then
    // created from real path/text changes, indexed normally and checked exactly.
    let calibration = TempDir::new().unwrap();
    write(calibration.path(), &sized_path(0, 119), "");
    let mut calibrator = Engine::open(calibration.path()).unwrap();
    let d = Deadline::new(600_000);
    let (_, one, _, _) = calibrator.index(&d).unwrap();
    assert_eq!(one.entities.len(), 1);
    assert!(one.observations.is_empty() && one.evidence.is_empty() && one.diagnostics.is_empty());
    assert_eq!(one.coverage.languages.len(), 1);
    let entity_bytes = json_size(&one.entities[0]);
    let header_bytes = json_size(&one) - entity_bytes;
    let n = 64_000usize;
    // Two coverage counts and the one language count change1->64000: +12.
    // N entity commas contribute N-1. Five Entity string fields repeat path.
    let base = header_bytes + n * entity_bytes + (n - 1) + 12;
    let difference = 64_000_000usize
        .checked_sub(base)
        .expect("fixture qualification: baseline overshoots64MB");
    assert!(
        difference < n * 5,
        "fixture calibration changed; not a feature RED"
    );
    let long_count = difference / 5;
    let remainder = difference % 5;
    drop(one);
    drop(calibrator);
    drop(calibration);

    let root = TempDir::new().unwrap();
    let padded = sized_path(n - 1, if n - 1 < long_count { 120 } else { 119 });
    for i in 0..n {
        let path = sized_path(i, if i < long_count { 120 } else { 119 });
        let body = if i == n - 1 {
            " ".repeat(10usize.pow(remainder as u32))
        } else {
            String::new()
        };
        write(root.path(), &path, &body);
    }
    let mut engine = Engine::open(root.path()).unwrap();
    let (snapshot, graph, _, _) = engine.index(&d).unwrap();
    assert_eq!(snapshot.files.len(), n);
    assert_eq!(graph.entities.len(), n);
    assert_eq!(
        json_size(&graph),
        64_000_000,
        "fixture must reach exact real bytes"
    );
    let (result, reads) = with_graph_reads(&engine.store, false, || {
        begin(&engine.store, &snapshot, &graph, &d, 4_000_000)
    });
    drop(result.unwrap());
    assert!(reads > 0);
    drop(graph);
    drop(snapshot);
    write(
        root.path(),
        &padded,
        &" ".repeat(10usize.pow(remainder as u32 + 1)),
    );
    let (snapshot, graph, _, _) = engine.index(&d).unwrap();
    assert_eq!(json_size(&graph), 64_000_001);
    let (result, reads) = with_graph_reads(&engine.store, true, || {
        begin(&engine.store, &snapshot, &graph, &d, 4_000_000)
    });
    assert_eq!(
        hard(result),
        RetrievalHardError::Capacity("graph_json_bytes")
    );
    assert_eq!(reads, 0, "stream bound must precede saved Graph allocation");
}

#[test]
#[ignore = "qualified root job:38277 real paths/two~53MB graphs, estimated<=2GB RAM/3GB disk"]
fn real_ownership_exact_16mb_passes_and_next_byte_refuses_before_body_scoring() {
    let root = TempDir::new().unwrap();
    let roots = [
        format!("a{}", "q".repeat(189)),
        format!("b{}", "q".repeat(189)),
    ];
    for (i, p) in roots.iter().enumerate() {
        assert_eq!(p.len(), 190);
        write(
            root.path(),
            &format!("{p}/package.json"),
            if i == 0 {
                r#"{"name":"a"}"#
            } else {
                r#"{"name":"b"}"#
            },
        );
    }
    for i in 0..38_275 {
        let path = format!(
            "{}/{}{}",
            roots[i % 2],
            base36(i),
            if i < 200 { "p" } else { "" }
        );
        write(root.path(), &path, "");
    }
    let mut engine = Engine::open(root.path()).unwrap();
    let d = Deadline::new(600_000);
    let (snapshot, graph, _, _) = engine.index(&d).unwrap();
    assert_eq!(snapshot.files.len(), 38_277);
    assert!(graph.entities.len() < 100_000 && json_size(&graph) < 64_000_000);
    // Literal independent charge:38275*(196+190+32)+2*(203+190+32)+200.
    assert_eq!(38_275 * 418 + 2 * 425 + 200, 16_000_000);
    let mut run = begin(&engine.store, &snapshot, &graph, &d, 4_000_000).unwrap();
    assert!(matches!(probe(&mut run).unwrap(), ProbeOutcome::Ready(_)));
    assert_eq!(run.usage().ownership_bytes(), 16_000_000);
    drop(run);
    drop(graph);
    drop(snapshot);
    let from = format!("{}/{}", roots[0], base36(200));
    rename_owned(root.path(), &from, &format!("{from}p"));
    let (snapshot, graph, _, _) = engine.index(&d).unwrap();
    assert_eq!(snapshot.files.len(), 38_277);
    assert!(graph.entities.len() < 100_000 && json_size(&graph) < 64_000_000);
    let mut run = begin(&engine.store, &snapshot, &graph, &d, 4_000_000).unwrap();
    assert_eq!(
        hard(probe(&mut run)),
        RetrievalHardError::Capacity("ownership_bytes")
    );
    assert!(
        run.usage().ownership_bytes() <= 16_000_000,
        "refuse before extra map insertion/debit"
    );
    drop(run);
    // Additional ordering witness: corrupt only the empty body CAS AFTER a
    // valid saved graph exists. Markers remain valid. The real driver must
    // refuse the map BEFORE it can observe body corruption; never a fallback.
    fs::write(
        engine.store.dir.join("blobs").join(crate::model::hash(b"")),
        b"!",
    )
    .unwrap();
    let error = crate::retrieval::find(
        &engine.store,
        &snapshot,
        &graph,
        &json!({"query":"needle","scopeMode":"manifest_balanced_v1"}),
        &d,
    )
    .unwrap_err();
    assert_eq!(
        error.downcast_ref::<RetrievalHardError>(),
        Some(&RetrievalHardError::Capacity("ownership_bytes"))
    );
}

fn long_path() -> String {
    let mut path = format!("{}/", "é".repeat(50)).repeat(40);
    path.push_str(&"x".repeat(52));
    path.push_str(".txt");
    assert_eq!(path.len(), 4096);
    path
}

#[test]
#[ignore = "qualified Windows long-path job; creation/capture failure is setup failure, never skip-as-pass"]
fn genuine_4096_utf8_path_is_captured_and_4097_is_refused_by_existing_capture_boundary() {
    let root = TempDir::new().unwrap();
    let path = long_path();
    write(root.path(), &path, "old captured text");
    write(root.path(), "a/package.json", r#"{"name":"a"}"#);
    write(root.path(), "b/package.json", r#"{"name":"b"}"#);
    let mut engine = Engine::open(root.path()).unwrap();
    let d = Deadline::new(120_000);
    let (snapshot, graph, _, _) = engine.index(&d).unwrap();
    assert!(
        snapshot
            .files
            .iter()
            .any(|f| f.path == path && f.size == 17)
    );
    let mut run = begin(&engine.store, &snapshot, &graph, &d, 4_000_000).unwrap();
    let p = match probe(&mut run).unwrap() {
        ProbeOutcome::Ready(p) => p,
        _ => panic!("two real packages"),
    };
    assert_eq!(p.scope(&p.owner(&path).unwrap()).unwrap().root(), ".");
    assert_eq!(run.read_source(&path).unwrap().text(), "old captured text");
    drop(p);
    drop(run);
    drop(graph);
    drop(snapshot);
    let mut plus_one = path.clone();
    plus_one.insert(plus_one.len() - 4, 'x');
    rename_owned(root.path(), &path, &plus_one);
    assert_eq!(plus_one.len(), 4097);
    let (snapshot, graph, _, _) = engine.index(&d).unwrap();
    assert!(
        snapshot
            .files
            .iter()
            .all(|f| f.path != path && f.path != plus_one)
    );
    assert_eq!(
        snapshot.omissions,
        vec![crate::model::Omission {
            path: plus_one.clone(),
            reason: "unsupported-path".into(),
        }]
    );
    assert_eq!(graph.coverage.omissions, 1);
    assert!(!graph.coverage.exhaustive);
    let mut run = begin(&engine.store, &snapshot, &graph, &d, 4_000_000).unwrap();
    let usage = run.usage();
    assert_eq!(
        hard(run.read_source(&plus_one)),
        RetrievalHardError::DeniedSource("captured_path")
    );
    assert_eq!(run.usage(), usage);
}

fn calls(count: usize) -> String {
    format!("fn f(){{}} fn c(){{{}}}\n", "f();".repeat(count))
}

#[test]
#[ignore = "optional very large job:qualify>=8GB RAM/12GB disk, no runtime guarantee; may hit earlier constructor limit"]
fn genuine_one_million_observations_is_byte_dominated_then_plus_one_refuses_before_graph_loading() {
    let calibration = TempDir::new().unwrap();
    write(calibration.path(), "a.rs", &calls(10));
    let mut calibrator = Engine::open(calibration.path()).unwrap();
    let d = Deadline::new(600_000);
    let (_, small, _, _) = calibrator.index(&d).unwrap();
    assert_eq!(
        small.observations.len(),
        12,
        "fixture calibration:10calls+2contains; otherwise not a capacity RED"
    );
    drop(small);
    drop(calibrator);
    drop(calibration);
    let root = TempDir::new().unwrap();
    for i in 0..20 {
        write(root.path(), &format!("f{i:02}.rs"), &calls(49_998));
    }
    let mut engine = Engine::open(root.path()).unwrap();
    let (snapshot, graph, _, _) = engine
        .index(&d)
        .expect("qualification:real constructor must succeed before guard evidence");
    assert_eq!(graph.observations.len(), 1_000_000);
    assert!(snapshot.files.len() < 100_000 && graph.entities.len() < 100_000);
    assert!(json_size(&graph) >= 322_000_001);
    let (result, reads) = with_graph_reads(&engine.store, true, || {
        begin(&engine.store, &snapshot, &graph, &d, 4_000_000)
    });
    assert_eq!(
        hard(result),
        RetrievalHardError::Capacity("graph_json_bytes"),
        "equality is allowed by observation-count guard, then bytes must refuse; NEVER Ready"
    );
    assert_eq!(reads, 0);
    drop(graph);
    drop(snapshot);
    write(root.path(), "f00.rs", &calls(49_999));
    let (snapshot, graph, _, _) = engine
        .index(&d)
        .expect("qualification:new actual constructor must succeed");
    assert_eq!(graph.observations.len(), 1_000_001);
    let (result, reads) = with_graph_reads(&engine.store, true, || {
        begin(&engine.store, &snapshot, &graph, &d, 4_000_000)
    });
    assert_eq!(
        hard(result),
        RetrievalHardError::Capacity("graph_observations")
    );
    assert_eq!(reads, 0);
}
