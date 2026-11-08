//! Source-only comparison budgets over losslessly padded SQL metadata.
//! Independent typed-envelope accounting and logical state checks guard exact limits.
//! The fixture configures no provider or Node worker.
#[path = "raw_typed_state.rs"]
mod state;

use super::{ArchiveProof, BuildOptions, View, archive, build, comparison_input, load};
use crate::{
    Engine,
    analysis::AnalyzerEpoch,
    capture::{Deadline, OPTIONS_RECORD_KIND},
    model::{Snapshot, identity},
    view_comparison::{ComparisonOptions, SelectionPin},
};
use rusqlite::{OptionalExtension, params};
use serde::Serialize;
use std::{fs, io::Write, path::PathBuf};
use tempfile::TempDir;

const MAX_RAW: usize = 32_000_000;
const MAX_FIXTURE_BYTES: usize = 1_000_000;
const PROFILE: &str = "yoke_view_comparison_input_1";
const BEFORE: &str =
    "// __RAW_TYPED_BEFORE_729dda__\nfn helper() -> i32 { 1 }\nfn caller() -> i32 { helper() }\n";
const AFTER: &str = "// __RAW_TYPED_AFTER_98364c__\nfn helper() -> i32 { 2 }\nfn caller() -> i32 { helper() + helper() }\n";

fn deadline() -> Deadline {
    Deadline::new(30_000)
}

struct Fixture {
    _directory: TempDir,
    engine: Engine,
    views: [View; 2],
    call_log: PathBuf,
}

fn fixture() -> Fixture {
    let directory = TempDir::new().unwrap();
    let root = directory.path().join("workspace");
    fs::create_dir(&root).unwrap();
    let call_log = directory.path().join("provider-calls.txt");
    let mut engine = Engine::open(&root).unwrap();
    engine.capture_epoch = AnalyzerEpoch::V9;
    let mut views = Vec::new();
    for source in [BEFORE, AFTER] {
        fs::write(root.join("main.rs"), source).unwrap();
        let (snapshot, graph, _, _) = engine.index(&deadline()).unwrap();
        assert_eq!(
            snapshot.files.len(),
            1,
            "Source-only fixture has one member"
        );
        assert_eq!(snapshot.files[0].path, "main.rs");
        assert_eq!(snapshot.files[0].size, source.len());
        let view = build(
            &engine.store,
            &snapshot,
            &graph,
            &BuildOptions {
                structural_kinds: Vec::new(),
                ..Default::default()
            },
            &deadline(),
        )
        .unwrap();
        assert!(view.overlays.is_empty() && view.meaning.is_none());
        assert!(
            !view.nodes.is_empty(),
            "Published source view must contain real nodes"
        );
        views.push(view);
    }
    let views: [View; 2] = views.try_into().unwrap();
    assert_ne!(views[0].snapshot_id, views[1].snapshot_id);
    assert_ne!(views[0].base_graph_hash, views[1].base_graph_hash);
    assert!(engine.servers.is_empty() && engine.server_configurations.is_empty());
    // No worker or ProviderSpec is installed. Absence is inventoried; this is not
    // a calibrated provider-execution tripwire or a claim about all processes.
    assert!(!call_log.exists());
    Fixture {
        _directory: directory,
        engine,
        views,
        call_log,
    }
}

fn safe_chain(error: &anyhow::Error) -> String {
    let text = format!("{error:#}");
    assert!(
        text.len() <= 256 && text.is_ascii(),
        "Comparison error must be bounded static metadata"
    );
    for source in [
        "__RAW_TYPED_BEFORE_729dda__",
        "__RAW_TYPED_AFTER_98364c__",
        BEFORE,
        AFTER,
    ] {
        assert!(
            !text.contains(source),
            "Comparison error exposed fixture source bytes"
        );
    }
    assert!(
        error.chain().all(|cause| {
            let category = cause.to_string();
            category.starts_with("comparison_")
                && category
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b == b'_')
        }),
        "Full comparison error chain must contain only safe categories"
    );
    text
}

fn positive<T>(result: anyhow::Result<T>, stage: &'static str) -> T {
    match result {
        Ok(value) => value,
        Err(error) => {
            let category = safe_chain(&error);
            panic!("{stage} expected a genuine positive pair; received {category}");
        }
    }
}

fn counters(accounting: &comparison_input::ComparisonInputAccounting) -> [usize; 5] {
    [
        accounting.before_proof_bytes(),
        accounting.after_proof_bytes(),
        accounting.total_proof_bytes(),
        accounting.distinct_source_bytes(),
        accounting.distinct_source_objects(),
    ]
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CountedSideProof<'a> {
    profile: &'static str,
    snapshot: &'a Snapshot,
    archive: &'a ArchiveProof,
}

fn independently_count(snapshot: &Snapshot, proof: &ArchiveProof) -> usize {
    // The envelope shape/profile are frozen in INPUT_API.md; the released archive
    // path supplies the complete proof independently of comparison_input.
    struct Counter(usize);
    impl Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            let next = self
                .0
                .checked_add(bytes.len())
                .expect("Fixture count overflow");
            assert!(
                next <= MAX_FIXTURE_BYTES,
                "Trusted fixture proof exceeds bound"
            );
            self.0 = next;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut counter = Counter(0);
    serde_json::to_writer(
        &mut counter,
        &CountedSideProof {
            profile: PROFILE,
            snapshot,
            archive: proof,
        },
    )
    .unwrap();
    counter.0
}

fn row_bytes(store: &crate::store::Store, table: &str, id: &str) -> usize {
    let sql = match table {
        "snapshots" => "SELECT length(CAST(value AS BLOB)) FROM snapshots WHERE id=?1",
        "graphs" => "SELECT length(CAST(value AS BLOB)) FROM graphs WHERE id=?1",
        _ => panic!("Unexpected fixture row table"),
    };
    store.db.query_row(sql, [id], |row| row.get(0)).unwrap()
}

fn rewrite_losslessly(store: &crate::store::Store, table: &str, id: &str, target: usize) {
    assert!(target < MAX_FIXTURE_BYTES);
    assert!(target < MAX_RAW);
    let prior_size = row_bytes(store, table, id);
    assert!(
        prior_size < MAX_FIXTURE_BYTES,
        "Original fixture raw row exceeds bound"
    );
    let (select, update) = match table {
        "snapshots" => (
            "SELECT value FROM snapshots WHERE id=?1",
            "UPDATE snapshots SET value=?1 WHERE id=?2",
        ),
        "graphs" => (
            "SELECT value FROM graphs WHERE id=?1",
            "UPDATE graphs SET value=?1 WHERE id=?2",
        ),
        _ => panic!("Unexpected fixture row table"),
    };
    let original: String = store.db.query_row(select, [id], |row| row.get(0)).unwrap();
    assert_eq!(original.len(), prior_size);
    let typed_value: serde_json::Value = serde_json::from_str(&original).unwrap();
    let mut formatted = serde_json::to_string_pretty(&typed_value).unwrap();
    let padding = target
        .checked_sub(formatted.len())
        .expect("Explicit padding target must exceed formatted fixture row");
    assert!(
        padding > 0,
        "Fixture must explicitly pad beyond pretty JSON"
    );
    formatted.try_reserve_exact(padding).unwrap();
    formatted.extend(std::iter::repeat_n(' ', padding));
    assert_eq!(formatted.len(), target);
    let decoded: serde_json::Value = crate::json::parse(formatted.as_bytes(), MAX_RAW).unwrap();
    assert!(
        decoded == typed_value,
        "Formatting and whitespace must be lossless JSON"
    );
    // Intentional fixture SQL mutation. The API read-only baseline is taken ONLY
    // after all four rewritten rows and released identity checks below.
    assert_eq!(store.db.execute(update, params![formatted, id]).unwrap(), 1);
    assert_eq!(row_bytes(store, table, id), target);
}

fn admitted_raw_metadata(store: &crate::store::Store, id: &str) -> usize {
    let mut total = row_bytes(store, "snapshots", id)
        .checked_add(row_bytes(store, "graphs", id))
        .unwrap();
    // Actual physical rows reached by v2/v4 source-lineage metadata admission.
    // The real disk fixture has no origin chain or Git commit registration.
    for kind in [
        OPTIONS_RECORD_KIND,
        crate::capture::IGNORED_RECORD_KIND,
        crate::capture::DERIVED_OPTIONS_RECORD_KIND,
        crate::git_workflows::SOURCE_KIND,
        "overlay-snapshot",
    ] {
        let size: Option<usize> = store
            .db
            .query_row(
                "SELECT length(CAST(value AS BLOB)) FROM records WHERE kind=?1 AND id=?2",
                params![kind, id],
                |row| row.get(0),
            )
            .optional()
            .unwrap();
        if let Some(size) = size {
            assert_eq!(
                kind, OPTIONS_RECORD_KIND,
                "Fixture must have no derived/overlay/Git lineage"
            );
            assert!(size < MAX_RAW);
            total = total.checked_add(size).unwrap();
        }
    }
    total
}

#[test]
fn raw_sql_metadata_above_exact_typed_pair_budget_remains_a_read_only_positive() {
    let f = fixture();
    let pins = f.views.each_ref().map(SelectionPin::from_view);
    // Unavailable input must fail HERE; it never substitutes for the v2 raw-cap
    // regression below. The fixture is genuine before any deliberate SQL edit.
    let original = positive(
        comparison_input::prepare_published_pair(
            &f.engine.store,
            &pins[0],
            &pins[1],
            &ComparisonOptions::default(),
            &deadline(),
        ),
        "unmodified published fixture",
    );
    let original_counters = counters(original.accounting());
    assert_eq!(
        original_counters[0]
            .checked_add(original_counters[1])
            .unwrap(),
        original_counters[2]
    );
    assert_eq!(
        original_counters[3],
        BEFORE.len().checked_add(AFTER.len()).unwrap()
    );
    assert_eq!(original_counters[4], 2);
    let typed_budget = original_counters[2];
    assert!(typed_budget > 0 && typed_budget < MAX_FIXTURE_BYTES / 4);
    let originals = f.views.each_ref().map(|view| {
        let (snapshot, _, proof) = archive(&f.engine.store, view, &deadline()).unwrap();
        let canonical = f.engine.store.graph(&snapshot.id).unwrap();
        let envelope_size = independently_count(&snapshot, &proof);
        (
            serde_json::to_value(&snapshot).unwrap(),
            serde_json::to_value(&canonical).unwrap(),
            envelope_size,
        )
    });
    assert_eq!(originals[0].2, original_counters[0]);
    assert_eq!(originals[1].2, original_counters[1]);
    drop(original);

    let padded_size = typed_budget.checked_add(4096).unwrap();
    assert!(padded_size > typed_budget && padded_size < MAX_FIXTURE_BYTES && padded_size < MAX_RAW);
    for view in &f.views {
        for table in ["snapshots", "graphs"] {
            rewrite_losslessly(&f.engine.store, table, &view.snapshot_id, padded_size);
            let raw = row_bytes(&f.engine.store, table, &view.snapshot_id);
            assert!(
                raw > typed_budget && raw < MAX_RAW,
                "Raw physical row must exceed original typed pair cap and fit native cap"
            );
        }
        let initial_raw = row_bytes(&f.engine.store, "snapshots", &view.snapshot_id)
            .checked_add(row_bytes(&f.engine.store, "graphs", &view.snapshot_id))
            .unwrap();
        let lineage_raw = admitted_raw_metadata(&f.engine.store, &view.snapshot_id);
        assert!(initial_raw > typed_budget && initial_raw < MAX_RAW);
        assert!(
            lineage_raw >= initial_raw && lineage_raw < MAX_RAW,
            "Actual per-side raw metadata aggregate must fit fixed native cap"
        );
    }
    // Released readers independently establish unchanged complete typed values,
    // capture identity/source authority, canonical graph hash and full View.
    for (index, view) in f.views.iter().enumerate() {
        let snapshot = f.engine.store.snapshot(&view.snapshot_id).unwrap();
        let graph = f.engine.store.graph(&view.snapshot_id).unwrap();
        assert!(
            serde_json::to_value(&snapshot).unwrap() == originals[index].0,
            "Typed Snapshot changed after whitespace rewrite"
        );
        assert!(
            serde_json::to_value(&graph).unwrap() == originals[index].1,
            "Typed Graph changed after whitespace rewrite"
        );
        assert_eq!(snapshot.id, view.snapshot_id);
        assert_eq!(identity(&graph), view.base_graph_hash);
        let policy = crate::capture::source_scope_policy(&f.engine.store, &snapshot).unwrap();
        assert!(policy.file("main.rs").unwrap() == &snapshot.files[0]);
        let loaded = load(&f.engine.store, &view.id, &deadline()).unwrap();
        assert!(
            serde_json::to_value(loaded).unwrap() == serde_json::to_value(view).unwrap(),
            "Full released View changed after whitespace rewrite"
        );
    }

    // No mutation above is an API effect. This is the FIRST read-only baseline.
    let baseline = state::state(&f.engine, &f.call_log);
    let exact_options = ComparisonOptions {
        max_input_bytes: typed_budget,
        max_source_bytes: original_counters[3],
        ..Default::default()
    };
    let result = comparison_input::prepare_published_pair(
        &f.engine.store,
        &pins[0],
        &pins[1],
        &exact_options,
        &deadline(),
    );
    assert!(
        state::state(&f.engine, &f.call_log) == baseline,
        "Exact-budget preparation changed post-rewrite SQL/CAS/call-log/caller state"
    );
    // v2-functional must fail here with comparison_capacity_exceeded; v4 must
    // succeed. Never accept either outcome or relabel a refusal as a positive.
    let pair = positive(
        result,
        "lossless padded rows at original exact typed pair budget",
    );
    assert_eq!(counters(pair.accounting()), original_counters);
    for (side, expected) in [(pair.before(), &f.views[0]), (pair.after(), &f.views[1])] {
        assert!(
            serde_json::to_value(side.view()).unwrap() == serde_json::to_value(expected).unwrap(),
            "Prepared full View differs from original selection"
        );
        assert_eq!(side.primary_snapshot().id, expected.snapshot_id);
        assert_eq!(identity(side.canonical_graph()), expected.base_graph_hash);
    }
    drop(pair);
    assert!(
        state::state(&f.engine, &f.call_log) == baseline,
        "Dropping checked pair changed post-rewrite state"
    );

    // Genuine exact positive must precede this independent one-byte-below check.
    let below = ComparisonOptions {
        max_input_bytes: typed_budget.checked_sub(1).unwrap(),
        ..exact_options
    };
    let refusal = match comparison_input::prepare_published_pair(
        &f.engine.store,
        &pins[0],
        &pins[1],
        &below,
        &deadline(),
    ) {
        Ok(value) => {
            drop(value);
            panic!("One below exact typed pair budget must refuse")
        }
        Err(error) => error,
    };
    assert_eq!(safe_chain(&refusal), "comparison_capacity_exceeded");
    assert!(
        state::state(&f.engine, &f.call_log) == baseline,
        "One-below refusal changed post-rewrite state"
    );
}
