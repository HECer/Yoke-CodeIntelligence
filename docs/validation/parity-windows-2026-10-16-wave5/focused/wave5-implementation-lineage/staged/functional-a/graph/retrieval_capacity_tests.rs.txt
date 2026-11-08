//! Pure serializer boundary plus genuine saved-source coupling controls.
//! NOT compiled or executed. Plain serialization data grants no source access.

use super::*;
use crate::{
    Engine,
    model::{identity, valid_id},
};
use serde::ser::{Error, SerializeSeq};
use std::{
    fs,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use tempfile::TempDir;

const LIMIT: usize = 64_000_000;
const CHUNK: &str = concat!(
    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
);

// Serializes an exact-length JSON array using <=1002 bytes of auxiliary data.
// Literal wire length = n*(1000+quotes2+comma1)+tail+brackets2+tailquotes2.
struct RepeatedArray {
    wire_bytes: usize,
}
impl Serialize for RepeatedArray {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let n = (self.wire_bytes - 4) / 1003;
        let tail = (self.wire_bytes - 4) % 1003;
        let mut seq = serializer.serialize_seq(Some(n + 1))?;
        for _ in 0..n {
            seq.serialize_element(CHUNK)?;
        }
        seq.serialize_element(&"a".repeat(tail))?;
        seq.end()
    }
}
struct Poison(Arc<AtomicBool>);
impl Serialize for Poison {
    fn serialize<S: serde::Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
        self.0.store(true, Ordering::SeqCst);
        Err(S::Error::custom("SERIALIZER_SECRET_MUST_NOT_ESCAPE"))
    }
}
struct PrefixThenPoison(Arc<AtomicBool>);
impl Serialize for PrefixThenPoison {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // Prefix before its final bracket consumes exactly LIMIT bytes.
        // The comma before poison is byte LIMIT+1 and must refuse BEFORE the
        // poison serializer is invoked. Eager to_vec would invoke poison.
        let prefix_wire = LIMIT + 1;
        let n = (prefix_wire - 4) / 1003;
        let tail = (prefix_wire - 4) % 1003;
        let mut seq = serializer.serialize_seq(None)?;
        for _ in 0..n {
            seq.serialize_element(CHUNK)?;
        }
        seq.serialize_element(&"a".repeat(tail))?;
        seq.serialize_element(&Poison(self.0.clone()))?;
        seq.end()
    }
}

#[test]
fn graph_stream_exact_64mb_matches_independent_literal_sha() {
    assert_eq!(CHUNK.len(), 1000);
    let hash =
        bounded_graph_fingerprint(&RepeatedArray { wire_bytes: LIMIT }, &Deadline::new(60_000))
            .unwrap();
    // Filled by independent incremental Python SHA256 over the literal wire,
    // never by production bounded_graph_fingerprint.
    assert_eq!(
        hash,
        "ec27c9e8186e18955923e326ec81571b342e471d31277a4a57695fd87911c858"
    );
}

#[test]
fn graph_stream_plus_one_is_typed_capacity_during_serialization() {
    assert!(matches!(
        bounded_graph_fingerprint(
            &RepeatedArray {
                wire_bytes: LIMIT + 1
            },
            &Deadline::new(60_000)
        ),
        Err(RetrievalHardError::Capacity("graph_json_bytes"))
    ));
}

#[test]
fn graph_stream_stops_before_poison_tail_instead_of_buffering_the_whole_value() {
    let reached = Arc::new(AtomicBool::new(false));
    let result =
        bounded_graph_fingerprint(&PrefixThenPoison(reached.clone()), &Deadline::new(60_000));
    assert!(matches!(
        result,
        Err(RetrievalHardError::Capacity("graph_json_bytes"))
    ));
    assert!(
        !reached.load(Ordering::SeqCst),
        "serialization continued past byte64000000"
    );
}

#[test]
fn serializer_failure_is_internal_and_static_instead_of_policy_fallback() {
    let reached = Arc::new(AtomicBool::new(false));
    let result = bounded_graph_fingerprint(&Poison(reached.clone()), &Deadline::new(10_000));
    let error = result.unwrap_err();
    assert!(reached.load(Ordering::SeqCst));
    assert!(matches!(
        error,
        RetrievalHardError::Internal("graph_serialization")
    ));
    assert!(!error.to_string().contains("SERIALIZER_SECRET"));
}

#[test]
fn genuine_saved_graph_uses_the_same_stream_identity_as_existing_model_identity() {
    let root = TempDir::new().unwrap();
    fs::write(root.path().join("lib.rs"), "fn f(){} fn c(){f();}\n").unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let (snapshot, graph, _, _) = engine.index(&Deadline::new(30_000)).unwrap();
    assert_eq!(
        identity(&engine.store.snapshot(&snapshot.id).unwrap()),
        identity(&snapshot)
    );
    assert_eq!(
        identity(&engine.store.graph(&snapshot.id).unwrap()),
        identity(&graph)
    );
    assert_eq!(
        bounded_graph_fingerprint(&graph, &Deadline::new(10_000)).unwrap(),
        identity(&graph)
    );
}

#[test]
fn real_observation_schema_proves_one_million_cannot_fit_the_graph_byte_ceiling() {
    let root = TempDir::new().unwrap();
    fs::write(root.path().join("lib.rs"), "fn f(){} fn c(){f();}\n").unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let (_, graph, _, _) = engine.index(&Deadline::new(30_000)).unwrap();
    let observation = graph
        .observations
        .iter()
        .find(|o| o.relation == "calls")
        .unwrap();
    assert!(
        valid_id(&observation.id)
            && valid_id(&observation.snapshot_id)
            && valid_id(&observation.source)
    );
    // The current required observation keys with ONLY those3 IDs, all other
    // strings empty, target=null and evidenceIds=[] already take321 bytes.
    // Real data can only add bytes. Commas/brackets add n+1 for n rows.
    assert!(serde_json::to_vec(observation).unwrap().len() >= 321);
    let literal_array_lower_bound = 322_000_001usize;
    assert!(literal_array_lower_bound > LIMIT);
    // This is dominance evidence, NOT a passed observation-count guard test.
}

#[test]
fn canonical_path_4096_utf8_bytes_is_existing_syntax_only_not_source_authority() {
    let segment = "é".repeat(50); //100UTF8 bytes,50Unicode scalar values.
    let mut path = format!("{}/", segment).repeat(40); //4040 bytes.
    path.push_str(&"x".repeat(52));
    path.push_str(".txt");
    assert_eq!(path.len(), 4096);
    assert_eq!(crate::paths::normalized_syntax(&path).unwrap(), path);
    path.insert(path.len() - 4, 'x');
    assert_eq!(path.len(), 4097);
    assert!(crate::paths::normalized_syntax(&path).is_err());
}
