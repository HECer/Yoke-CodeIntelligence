//! Labeled in-memory protocol/ordering faults; actual Git lives in integration tests.
use super::*;
fn d() -> Deadline {
    Deadline::new(10_000)
}
fn row(path: &str, stage: u8, oid: &str) -> Vec<u8> {
    format!("100644 {oid} {stage}\t{path}\0").into_bytes()
}
struct Fixture {
    raw: Vec<u8>,
    after: Option<Vec<u8>>,
    calls: usize,
    bodies: BTreeMap<String, Vec<u8>>,
    reads: usize,
    tamper: bool,
}
impl Fixture {
    fn new(path: &str, text: &[u8]) -> Self {
        let id = object_id("blob", text, 40);
        Self {
            raw: row(path, 2, &id),
            after: None,
            calls: 0,
            bodies: BTreeMap::from([(id, text.to_vec())]),
            reads: 0,
            tamper: false,
        }
    }
}
impl Objects for Fixture {
    fn format(&mut self, d: &Deadline) -> Result<usize> {
        d.check()?;
        Ok(40)
    }
    fn index(&mut self, _: usize, d: &Deadline) -> Result<Vec<u8>> {
        d.check()?;
        self.calls += 1;
        Ok(if self.calls > 1 {
            self.after.clone().unwrap_or(self.raw.clone())
        } else {
            self.raw.clone()
        })
    }
    fn size(&mut self, id: &str, d: &Deadline) -> Result<usize> {
        d.check()?;
        Ok(self.bodies[id].len())
    }
    fn bytes(&mut self, id: &str, _: usize, d: &Deadline) -> Result<Vec<u8>> {
        d.check()?;
        self.reads += 1;
        let mut b = self.bodies[id].clone();
        if self.tamper {
            b[0] ^= 1;
        }
        Ok(b)
    }
}
fn options() -> Options {
    Options {
        allow_index_read: true,
        allow_git_sources: true,
        ..Default::default()
    }
}
fn allowed() -> BTreeMap<String, Vec<String>> {
    BTreeMap::from([("café.rs".into(), vec!["3".repeat(64)])])
}
#[test]
fn stage_protocol_is_nul_exact_unique_and_hash_width_checked() {
    let id = "a".repeat(40);
    let raw = row("café.rs", 2, &id);
    assert_eq!(parse(&raw, 40, &Limits::default(), &d()).unwrap().len(), 1);
    for bad in [
        raw[..raw.len() - 1].to_vec(),
        row("café.rs", 0, &id),
        row("", 1, &id),
        row("ok.rs", 1, &"b".repeat(64)),
        [raw.clone(), raw.clone()].concat(),
        b"\0".to_vec(),
        format!("100644  {id} 1\tok.rs\0").into_bytes(),
    ] {
        assert!(parse(&bad, 40, &Limits::default(), &d()).is_err());
    }
    for l in [
        Limits {
            max_paths: 0,
            ..Default::default()
        },
        Limits {
            max_stage_rows: 0,
            ..Default::default()
        },
        Limits {
            max_index_bytes: raw.len() - 1,
            ..Default::default()
        },
    ] {
        assert!(parse(&raw, 40, &l, &d()).is_err());
    }
}
#[test]
fn duplicate_blob_cache_never_grants_a_protected_second_path_or_collapses_stage_positions() {
    let mut f = Fixture::new("café.rs", b"pub fn same(){}\r\n");
    let oid = f.bodies.keys().next().unwrap().clone();
    f.raw.extend(row(".env", 1, &oid));
    f.raw.extend(row("café.rs", 3, &oid));
    let v = observe(&mut f, &allowed(), &options(), &d()).unwrap();
    assert_eq!(f.reads, 1);
    assert_eq!(v["stageRows"].as_array().unwrap().len(), 3);
    assert_eq!(v["stageRows"][1]["path"], Value::Null);
    assert_eq!(v["stageRows"][1]["quote"], Value::Null);
    assert_eq!(
        v["stageRows"][1]["sourceUnavailableReason"],
        "protected_or_noncanonical_path"
    );
    assert_eq!(v["stageRows"][2]["rowOrdinal"], 2);
    assert_eq!(v["conflicts"][0]["stageRowOrdinals"], json!([0, 2]));
    assert_eq!(v["conflicts"][0]["oursTheirsObjectAndModeIdentical"], true);
    assert_eq!(v["conflicts"][0]["indexResolved"], false);
}
#[test]
fn metadata_only_and_unadmitted_paths_never_read_blob_bodies() {
    let mut f = Fixture::new("café.rs", b"pub fn test(){}");
    let mut o = options();
    o.allow_git_sources = false;
    let v = observe(&mut f, &allowed(), &o, &d()).unwrap();
    assert_eq!(f.reads, 0);
    assert_eq!(
        v["stageRows"][0]["sourceUnavailableReason"],
        "host_source_permission_required"
    );
    for path in [
        "../escape.rs",
        "C:/outside.rs",
        "dir\\escape.rs",
        ".env",
        "other.rs",
    ] {
        let mut f = Fixture::new(path, b"private body");
        let v = observe(&mut f, &allowed(), &options(), &d()).unwrap();
        assert_eq!(f.reads, 0);
        assert_eq!(v["stageRows"][0]["quote"], Value::Null);
        assert_eq!(v["complete"], false);
    }
}
#[test]
fn source_page_binds_index_stage_scope_hash_and_utf8_offsets() {
    let text = "// 🦀 café\r\n";
    let mut f = Fixture::new("café.rs", text.as_bytes());
    let mut o = options();
    let request = ReadRequest {
        schema_version: 1,
        expected_index_hash: hash(&f.raw),
        path: "café.rs".into(),
        stage: 2,
        start_byte: 3,
        max_bytes: 4,
    };
    o.read_request = Some(request.clone());
    let v = observe(&mut f, &allowed(), &o, &d()).unwrap();
    assert_eq!(v["selectedSourcePage"]["range"]["text"], "🦀");
    assert_eq!(v["selectedSourcePage"]["range"]["endByte"], 7);
    for fault in ["stale", "stage", "scope", "start", "small"] {
        let mut f = Fixture::new("café.rs", text.as_bytes());
        let mut r = request.clone();
        let mut a = allowed();
        match fault {
            "stale" => r.expected_index_hash = "0".repeat(64),
            "stage" => r.stage = 1,
            "scope" => a.clear(),
            "start" => r.start_byte = 4,
            "small" => r.max_bytes = 3,
            _ => unreachable!(),
        }
        o.read_request = Some(r);
        assert!(observe(&mut f, &a, &o, &d()).is_err(), "{fault}");
        if ["stale", "stage", "scope"].contains(&fault) {
            assert_eq!(f.reads, 0);
        }
    }
}
#[test]
fn content_tamper_cumulative_source_and_object_limits_refuse_before_unbounded_reads() {
    let mut f = Fixture::new("café.rs", b"pub fn test(){}");
    f.tamper = true;
    assert!(observe(&mut f, &allowed(), &options(), &d()).is_err());
    for fault in ["source", "blob", "objects"] {
        let mut f = Fixture::new("café.rs", b"pub fn test(){}");
        let mut o = options();
        match fault {
            "source" => o.limits.max_source_bytes = 1,
            "blob" => o.limits.max_blob_bytes = 1,
            "objects" => o.limits.max_blob_objects = 0,
            _ => unreachable!(),
        }
        assert!(observe(&mut f, &allowed(), &o, &d()).is_err());
        assert_eq!(f.reads, 0);
    }
}
#[test]
fn changing_index_and_unavailable_source_text_never_become_empty_complete_success() {
    let mut f = Fixture::new("café.rs", b"ok");
    f.after = Some(vec![]);
    assert!(observe(&mut f, &allowed(), &options(), &d()).is_err());
    for (text, status) in [
        (
            b"-----BEGIN PRIVATE KEY-----".as_slice(),
            "verified_obvious_secret_omitted",
        ),
        (b"nul\0text", "verified_unsupported_control_text"),
        (&[255, 254], "verified_non_utf8"),
    ] {
        let mut f = Fixture::new("café.rs", text);
        let v = observe(&mut f, &allowed(), &options(), &d()).unwrap();
        assert_eq!(v["stageRows"][0]["sourceStatus"], status);
        assert_eq!(v["stageRows"][0]["quote"], Value::Null);
        assert_eq!(v["stageRows"][0]["gitBlobHashVerified"], true);
        assert_eq!(v["complete"], false);
    }
    let mut f = Fixture::new("café.rs", b"");
    f.raw.clear();
    let v = observe(
        &mut f,
        &BTreeMap::new(),
        &Options {
            allow_index_read: true,
            ..Default::default()
        },
        &d(),
    )
    .unwrap();
    assert!(v["conflicts"].as_array().unwrap().is_empty());
    assert_eq!(v["indexObservationComplete"], true);
    assert_eq!(v["complete"], false);
}

#[test]
fn source_page_json_failures_use_one_static_public_marker_without_input_values() {
    for raw in [
        Vec::new(),
        b"{".to_vec(),
        b"{\"fixture_private_marker\":1}".to_vec(),
        b"{\"stage\":2,\"stage\":3}".to_vec(),
        vec![255],
        vec![b' '; 8193],
    ] {
        let error = ReadRequest::parse(&raw).unwrap_err().to_string();
        assert_eq!(error, "git_conflicts_input_invalid");
        assert!(!error.contains("fixture_private_marker"));
    }
}
