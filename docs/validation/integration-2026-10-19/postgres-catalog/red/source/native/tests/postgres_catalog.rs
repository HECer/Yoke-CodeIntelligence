//! Frozen actual PostgreSQL18.6 client rows, replayed without a server.
use yoke::{capture::Deadline,postgres_catalog::Catalog};
#[test]
fn actual_received_catalog_rows_preserve_oid_overloads_and_parallel_composite_keys(){
    let raw=include_bytes!("fixtures/postgres-catalog-response.jsonl");
    let catalog=Catalog::from_response(raw,&Deadline::new(30_000)).unwrap();
    let value=catalog.as_value();
    assert_eq!(value["nodes"].as_array().unwrap().len(),7);
    assert_eq!(value["observations"].as_array().unwrap().len(),3);
    let overloads:Vec<_>=value["nodes"].as_array().unwrap().iter().filter(|n|n["name"]=="overload").collect();
    assert_eq!(overloads.len(),2);assert_ne!(overloads[0]["id"],overloads[1]["id"]);
    for edge in value["observations"].as_array().unwrap(){
        assert_eq!(edge["relation"],"declared_foreign_key");assert_eq!(edge["columnPairs"].as_array().unwrap().len(),2);
        assert_eq!(edge["columnPairs"][0]["source"],"a");assert_eq!(edge["columnPairs"][1]["source"],"b");
    }
    assert!(value["nodes"].as_array().unwrap().iter().all(|n|n["kind"]!="column"));
    assert_eq!(value["complete"],false);assert_eq!(value["serverAttestation"],"unverified");
    assert_eq!(value["source"]["responseHash"],yoke::model::hash(raw));
}
