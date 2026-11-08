//! Frozen actual PostgreSQL18.6 client rows, replayed without a server.
use serde_json::{Value, json};
use yoke::postgres_catalog::{Limits, Query};
use yoke::{capture::Deadline, postgres_catalog::Catalog};
fn records() -> Vec<Value> {
    include_bytes!("fixtures/postgres-catalog-response.jsonl")
        .split(|b| *b == b'\n')
        .filter(|l| !l.is_empty())
        .map(|l| serde_json::from_slice(l).unwrap())
        .collect()
}
fn encoded(values: &[Value]) -> Vec<u8> {
    let mut out = Vec::new();
    for v in values {
        serde_json::to_writer(&mut out, v).unwrap();
        out.push(b'\n');
    }
    out
}
fn admit(values: &[Value]) -> anyhow::Result<Catalog> {
    Catalog::from_response(&encoded(values), &Deadline::new(30000))
}
#[test]
fn actual_received_catalog_rows_preserve_oid_overloads_and_parallel_composite_keys() {
    let raw = include_bytes!("fixtures/postgres-catalog-response.jsonl");
    let catalog = Catalog::from_response(raw, &Deadline::new(30_000)).unwrap();
    let value = catalog.as_value();
    assert_eq!(value["nodes"].as_array().unwrap().len(), 7);
    assert_eq!(value["observations"].as_array().unwrap().len(), 3);
    let overloads: Vec<_> = value["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|n| n["name"] == "overload")
        .collect();
    assert_eq!(overloads.len(), 2);
    assert_ne!(overloads[0]["id"], overloads[1]["id"]);
    for edge in value["observations"].as_array().unwrap() {
        assert_eq!(edge["relation"], "declared_foreign_key");
        assert_eq!(edge["columnPairs"].as_array().unwrap().len(), 2);
        assert_eq!(edge["columnPairs"][0]["source"], "a");
        assert_eq!(edge["columnPairs"][1]["source"], "b");
    }
    assert!(
        value["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .all(|n| n["kind"] != "column")
    );
    assert_eq!(value["complete"], false);
    assert_eq!(value["serverAttestation"], "unverified");
    assert_eq!(value["source"]["responseHash"], yoke::model::hash(raw));
}
#[test]
fn raw_unicode_record_ranges_and_frozen_archive_replay_are_exact() {
    let raw = include_bytes!("fixtures/postgres-catalog-response.jsonl");
    let d = Deadline::new(30000);
    let catalog = Catalog::from_response(raw, &d).unwrap();
    for record in catalog.as_value()["records"].as_array().unwrap() {
        let range = record["startByte"].as_u64().unwrap() as usize
            ..record["endByte"].as_u64().unwrap() as usize;
        assert_eq!(
            serde_json::from_slice::<Value>(&raw[range]).unwrap(),
            record["record"]
        );
    }
    assert!(
        catalog.as_value()["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n["name"] == "view 💡")
    );
    let archive = catalog.encode_archive(&d).unwrap();
    let loaded = Catalog::load_archive(&archive, &Limits::default(), &d).unwrap();
    assert_eq!(loaded.response_bytes(), raw);
    assert_eq!(loaded.as_value(), catalog.as_value());
    let mut forged: Value = serde_json::from_slice(&archive).unwrap();
    forged["captureClaims"] =
        json!({"serverAttestation":"verified","networkConnections":0,"complete":true});
    let loaded = Catalog::load_archive(
        &serde_json::to_vec(&forged).unwrap(),
        &Limits::default(),
        &d,
    )
    .unwrap();
    assert_eq!(loaded.as_value()["serverAttestation"], "unverified");
    assert_eq!(loaded.as_value()["complete"], false);
    assert_eq!(
        loaded.capture_claims().unwrap()["serverAttestation"],
        "verified"
    );
    forged["responseHash"] = json!("0".repeat(64));
    assert!(
        Catalog::load_archive(
            &serde_json::to_vec(&forged).unwrap(),
            &Limits::default(),
            &d
        )
        .is_err()
    );
}
#[test]
fn invalid_protocol_duplicate_identity_and_missing_completion_are_refused() {
    let original = records();
    for mutation in 0..10 {
        let mut r = original.clone();
        match mutation {
            0 => {
                r.pop();
            }
            1 => r[0]["readOnly"] = json!(false),
            2 => r[0]["isolation"] = json!("read committed"),
            3 => r[0]["serverVersionNum"] = json!(190000),
            4 => {
                r.insert(2, r[1].clone());
            }
            5 => r[1]["schema"] = json!("pg_hidden"),
            6 => r[1]["extra"] = json!(true),
            7 => {
                let i = r.iter().position(|r| r["section"] == "foreignKey").unwrap();
                r[i]["targetColumns"] = json!([]);
            }
            8 => {
                let i = r.iter().position(|r| r["section"] == "routine").unwrap();
                r[i]["argumentTypeOids"] = json!([23]);
            }
            9 => r[1]["definition"] = json!("fake SELECT 1"),
            _ => unreachable!(),
        }
        assert!(admit(&r).is_err(), "mutation {mutation}");
    }
    let mut raw = encoded(&original);
    raw.extend_from_slice(b"{\"section\":\"end\",\"section\":\"meta\"}\n");
    assert!(Catalog::from_response(&raw, &Deadline::new(30000)).is_err());
    let raw = encoded(&original)
        .into_iter()
        .chain([0xff])
        .collect::<Vec<_>>();
    assert!(Catalog::from_response(&raw, &Deadline::new(30000)).is_err());
}
#[test]
fn independent_row_cell_node_observation_wire_and_query_quotas_refuse_overflow() {
    let raw = include_bytes!("fixtures/postgres-catalog-response.jsonl");
    let d = Deadline::new(30000);
    for which in 0..7 {
        let mut l = Limits::default();
        match which {
            0 => l.max_response_bytes = raw.len() - 1,
            1 => l.max_record_bytes = 10,
            2 => l.max_relations = 3,
            3 => l.max_routines = 2,
            4 => l.max_foreign_keys = 2,
            5 => l.max_nodes = 6,
            6 => l.max_observations = 2,
            _ => unreachable!(),
        };
        assert!(
            Catalog::from_response_with_limits(raw, &l, &d).is_err(),
            "quota {which}"
        );
    }
    let mut values = records();
    let i = values
        .iter()
        .position(|r| r["section"] == "routine")
        .unwrap();
    values[i]["definitionStatus"] = json!("available");
    values[i]["definition"] = json!("x".repeat(65537));
    assert!(admit(&values).is_err());
    let c = Catalog::from_response(raw, &d).unwrap();
    assert!(c.query(&Query::Inspect, 64, &d).is_err());
    let node = c.as_value()["nodes"][0]["id"].as_str().unwrap().to_owned();
    assert!(
        c.query(
            &Query::Trace {
                id: node,
                depth: 9,
                max_nodes: 10
            },
            32000,
            &d
        )
        .is_err()
    );
    let canceled = Deadline::new(0);
    assert!(Catalog::from_response(raw, &canceled).is_err());
}
#[test]
fn declared_direction_and_parallel_edges_survive_degree_and_trace_queries() {
    let c = admit(&records()).unwrap();
    let d = Deadline::new(30000);
    let nodes = c.as_value()["nodes"].as_array().unwrap();
    let child = nodes
        .iter()
        .find(|n| n["name"] == "child" && n["schema"] == "public")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let parent = nodes.iter().find(|n| n["name"] == "parent").unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let degree = c
        .query(&Query::Degree { id: child.clone() }, 32000, &d)
        .unwrap();
    assert_eq!(degree["outgoing"].as_array().unwrap().len(), 2);
    assert!(degree["incoming"].as_array().unwrap().is_empty());
    let trace = c
        .query(
            &Query::Trace {
                id: child.clone(),
                depth: 1,
                max_nodes: 2,
            },
            32000,
            &d,
        )
        .unwrap();
    assert_eq!(trace["observations"].as_array().unwrap().len(), 2);
    assert_eq!(trace["nodes"].as_array().unwrap().len(), 2);
    let reverse = c
        .query(
            &Query::Trace {
                id: parent,
                depth: 8,
                max_nodes: 5,
            },
            32000,
            &d,
        )
        .unwrap();
    assert!(reverse["observations"].as_array().unwrap().is_empty());
    assert!(
        c.query(
            &Query::Trace {
                id: child,
                depth: 1,
                max_nodes: 1
            },
            32000,
            &d
        )
        .is_err()
    );
}
#[test]
fn foreign_key_endpoint_only_references_are_explicit_and_conflicts_refused() {
    let mut r = records();
    let parent = r
        .iter()
        .position(|r| r["section"] == "relation" && r["name"] == "parent")
        .unwrap();
    r.remove(parent);
    let c = admit(&r).unwrap();
    assert_eq!(
        c.as_value()["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|n| n["kind"] == "unselected_relation_reference")
            .count(),
        1
    );
    let i = r.iter().position(|r| r["section"] == "foreignKey").unwrap();
    r[i]["targetName"] = json!("different");
    assert!(admit(&r).is_err());
}
#[test]
fn host_permissions_and_literal_loopback_are_required_before_executable_lookup() {
    use yoke::postgres_catalog_client::{Permissions, Security, Spec, receive};
    let mut spec = Spec {
        client: std::env::temp_dir().join("missing-untrusted-psql.exe"),
        client_sha256: "0".repeat(64),
        address: "127.0.0.1".parse().unwrap(),
        port: 5432,
        database: "db ' \\ 💡".into(),
        user: "reader".into(),
        security: Security::LoopbackTrust,
    };
    let e = receive(
        &spec,
        &Permissions::default(),
        &Limits::default(),
        &Deadline::new(30000),
    )
    .err()
    .unwrap();
    assert_eq!(e.to_string(), "postgres_catalog_host_permission_required");
    let permissions = Permissions {
        trust_client: true,
        allow_network: true,
        allow_loopback_trust: false,
    };
    let e = receive(
        &spec,
        &permissions,
        &Limits::default(),
        &Deadline::new(30000),
    )
    .err()
    .unwrap();
    assert_eq!(
        e.to_string(),
        "postgres_catalog_loopback_trust_permission_required"
    );
    spec.address = "192.0.2.1".parse().unwrap();
    assert!(spec.validate().is_err());
    spec.address = "127.0.0.1".parse().unwrap();
    spec.database = "postgres\npassword=secret".into();
    assert!(spec.validate().is_err());
}
