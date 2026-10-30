//! Fixed parameterized Cypher. Received data never supplies executable queries.
use super::plan::{Mode, Plan, validate_payload};
use crate::{capture::Deadline, model::identity};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    ScopeConstraint,
    ElementConstraint,
    ObservationConstraint,
    ElementRevisionIndex,
    ObservationRevisionIndex,
    Namespace,
    Publish,
}
pub const SCHEMA_READ: &str = "CYPHER 5 SHOW CONSTRAINTS YIELD name,type,entityType,labelsOrTypes,properties WHERE name STARTS WITH 'yoke_native_' RETURN name,type,entityType,labelsOrTypes,properties ORDER BY name";
pub const INDEX_READ: &str = "CYPHER 5 SHOW INDEXES YIELD name,type,entityType,labelsOrTypes,properties,owningConstraint,state WHERE name STARTS WITH 'yoke_native_' RETURN name,type,entityType,labelsOrTypes,properties,owningConstraint,state ORDER BY name";
pub const SCOPE_READ: &str = "CYPHER 5 MATCH (s:YokeNativeScope {key:$scopeKey}) WITH s LIMIT 2 RETURN s.key AS scopeKey,s.ownerKind AS ownerKind,s.schemaVersion AS schemaVersion,s.activeRevision AS activeRevision,s.activeRevisionKey AS activeRevisionKey,s.createdByPlan AS createdByPlan";
pub const REVISION_READ: &str = "CYPHER 5 MATCH (n:YokeNativeElement:YokeNativeRevision {key:$revisionKey}) WITH n LIMIT 2 RETURN n.rowJson AS rowJson,n.rowHash AS rowHash,n.scopeKey AS scopeKey,n.revisionId AS revisionId,COUNT { MATCH (e:YokeNativeElement {scopeKey:n.scopeKey,revisionId:n.revisionId}) } AS elementCount,COUNT { MATCH ()-[r:YOKE_NATIVE_OBSERVATION {scopeKey:n.scopeKey,revisionId:n.revisionId}]->() } AS observationCount";
const NAMESPACE: &str = "CYPHER 5 CREATE (s:YokeNativeScope {key:$scopeKey,ownerKind:'yoke-native-neo4j-v1',schemaVersion:1,activeRevision:'absent',activeRevisionKey:'absent',lockCounter:0,createdByPlan:$planId}) RETURN {scopeKey:s.key,schemaVersion:s.schemaVersion,createdByPlan:s.createdByPlan,activeRevision:s.activeRevision} AS receipt";
const SCHEMA: [&str; 5] = [
    "CYPHER 5 CREATE CONSTRAINT yoke_native_scope_key_v1 IF NOT EXISTS FOR (s:YokeNativeScope) REQUIRE s.key IS UNIQUE",
    "CYPHER 5 CREATE CONSTRAINT yoke_native_element_key_v1 IF NOT EXISTS FOR (n:YokeNativeElement) REQUIRE n.key IS UNIQUE",
    "CYPHER 5 CREATE CONSTRAINT yoke_native_observation_key_v1 IF NOT EXISTS FOR ()-[r:YOKE_NATIVE_OBSERVATION]-() REQUIRE r.key IS UNIQUE",
    "CYPHER 5 CREATE INDEX yoke_native_element_revision_v1 IF NOT EXISTS FOR (n:YokeNativeElement) ON (n.scopeKey,n.revisionId)",
    "CYPHER 5 CREATE INDEX yoke_native_observation_revision_v1 IF NOT EXISTS FOR ()-[r:YOKE_NATIVE_OBSERVATION]-() ON (r.scopeKey,r.revisionId)",
];
pub fn operations(mode: Mode) -> Vec<Operation> {
    match mode {
        Mode::Bootstrap => vec![
            Operation::ScopeConstraint,
            Operation::ElementConstraint,
            Operation::ObservationConstraint,
            Operation::ElementRevisionIndex,
            Operation::ObservationRevisionIndex,
            Operation::Namespace,
        ],
        Mode::Publish => vec![Operation::Publish],
    }
}
fn publication() -> String {
    let mut q = String::from(
        "CYPHER 5 MATCH (scope:YokeNativeScope {key:$scopeKey}) WITH scope LIMIT 2 WITH collect(scope) AS scopes WHERE size(scopes)=1 UNWIND scopes AS scope SET scope.lockCounter=coalesce(scope.lockCounter,0)+1 WITH scope FOREACH (deny IN CASE WHEN scope.ownerKind='yoke-native-neo4j-v1' AND scope.schemaVersion=1 AND (scope.activeRevision=$expectedRevision OR scope.activeRevision=$revisionId) THEN [] ELSE [1] END | CREATE (:YokeNativeScope {key:$scopeKey})) ",
    );
    // All labels/parameter names below are compiled constants, never source labels.
    for (parameter, label) in [
        ("entities", "YokeNativeEntity"),
        ("evidence", "YokeNativeEvidence"),
        ("unresolved", "YokeNativeUnresolved"),
        ("revision", "YokeNativeRevision"),
    ] {
        let rows = if parameter == "revision" {
            "[$revision]".to_owned()
        } else {
            format!("${parameter}")
        };
        q.push_str(&format!("FOREACH (row IN {rows} | MERGE (n:YokeNativeElement:{label} {{key:row.key}}) ON CREATE SET n += row.props,n.rowHash=row.rowHash FOREACH (deny IN CASE WHEN n.rowHash=row.rowHash AND all(k IN keys(row.props) WHERE n[k]=row.props[k]) THEN [] ELSE [1] END | CREATE (:YokeNativeScope {{key:$scopeKey}}))) "));
    }
    q.push_str("FOREACH (row IN $observations | MERGE (src:YokeNativeElement {key:row.sourceKey}) MERGE (dst:YokeNativeElement {key:row.targetKey}) MERGE (src)-[r:YOKE_NATIVE_OBSERVATION {key:row.key}]->(dst) ON CREATE SET r += row.props,r.rowHash=row.rowHash FOREACH (deny IN CASE WHEN r.rowHash=row.rowHash AND all(k IN keys(row.props) WHERE r[k]=row.props[k]) THEN [] ELSE [1] END | CREATE (:YokeNativeScope {key:$scopeKey}))) WITH scope FOREACH (deny IN CASE WHEN COUNT { MATCH (n:YokeNativeElement {scopeKey:$scopeKey,revisionId:$revisionId}) }=size($entities)+size($evidence)+size($unresolved)+1 AND COUNT { MATCH ()-[r:YOKE_NATIVE_OBSERVATION {scopeKey:$scopeKey,revisionId:$revisionId}]->() }=size($observations) THEN [] ELSE [1] END | CREATE (:YokeNativeScope {key:$scopeKey})) SET scope.activeRevision=$revisionId,scope.activeRevisionKey=$revision.key RETURN {scopeKey:scope.key,revisionId:scope.activeRevision,payloadHash:$payloadHash,planId:$planId,entityCount:size($entities),observationCount:size($observations),evidenceCount:size($evidence),unresolvedCount:size($unresolved)} AS receipt");
    q
}
pub fn program_hash() -> String {
    // This is the application's policy source identity, with an explicit LF
    // materialization policy. Captured repository/source bytes are never normalized.
    let policy = include_str!("neo4j_publish_program.rs").replace("\r\n", "\n");
    let plan_policy = include_str!("neo4j_publish_plan.rs").replace("\r\n", "\n");
    let host_policy = [
        include_str!("neo4j_publish.rs"),
        include_str!("neo4j_publish_client.rs"),
        include_str!("neo4j_publish_transport.rs"),
        include_str!("neo4j_publish_checks.rs"),
        include_str!("neo4j_publish_journal.rs"),
        include_str!("neo4j_publish_cli.rs"),
    ]
    .map(|s| s.replace("\r\n", "\n"));
    identity(&(
        "neo4j-fixed-program-and-lf-policy-source-v1",
        policy,
        plan_policy,
        host_policy,
        SCHEMA,
        NAMESPACE,
        publication(),
        SCHEMA_READ,
        INDEX_READ,
        SCOPE_READ,
        REVISION_READ,
    ))
}
pub(crate) fn revision_key(scope: &str, revision: &str) -> String {
    identity(&("neo4j-element-v1", scope, revision, "revision", revision))
}
fn key(plan: &Plan, category: &str, id: &str) -> String {
    identity(&(
        "neo4j-element-v1",
        plan.scope(),
        plan.revision().unwrap(),
        category,
        id,
    ))
}
fn row(plan: &Plan, category: &str, id: &str, raw: &Value, mut props: Value) -> Value {
    let object = props.as_object_mut().unwrap();
    object.insert("scopeKey".into(), json!(plan.scope()));
    object.insert("revisionId".into(), json!(plan.revision().unwrap()));
    object.insert("category".into(), json!(category));
    object.insert("rowJson".into(), json!(serde_json::to_string(raw).unwrap()));
    json!({"key":key(plan,category,id),"rowHash":identity(&props),"props":props})
}
pub(crate) fn parameters(plan: &Plan, d: &Deadline) -> Result<Value> {
    plan.validate(d)?;
    if plan.mode() == Mode::Bootstrap {
        return Ok(json!({"scopeKey":plan.scope(),"planId":plan.id()}));
    }
    let payload = plan.payload().unwrap();
    let graph = validate_payload(payload, d)?;
    let files = payload["sourceFiles"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| (f["path"].as_str().unwrap(), &f["hash"]))
        .collect::<BTreeMap<_, _>>();
    let mut entities = Vec::new();
    for n in &graph.entities {
        d.check()?;
        entities.push(row(plan,"entity",&n.id,&json!(n),json!({"sourceId":n.id,"nativeKey":n.key,"label":n.qualified_name,"kind":n.kind,"language":n.language,"path":n.path,"startByte":n.start_byte,"endByte":n.end_byte,"contentHash":files[n.path.as_str()]})));
    }
    let mut evidence = Vec::new();
    for e in &graph.evidence {
        d.check()?;
        evidence.push(row(plan,"evidence",&e.id,&json!(e),json!({"evidenceId":e.id,"path":e.path,"contentHash":e.content_hash,"startByte":e.start_byte,"endByte":e.end_byte,"trust":e.trust,"producer":e.producer})));
    }
    let mut unresolved = Vec::new();
    let mut observations = Vec::new();
    for e in &graph.observations {
        d.check()?;
        let target = if let Some(id) = &e.target {
            key(plan, "entity", id)
        } else {
            unresolved.push(row(plan,"unresolved",&e.id,&json!({"observationId":e.id,"targetLabel":e.target_label,"sourceTargetWasNull":true}),json!({"observationId":e.id,"label":e.target_label,"sourceTargetWasNull":true})));
            key(plan, "unresolved", &e.id)
        };
        let props = json!({"scopeKey":plan.scope(),"revisionId":plan.revision().unwrap(),"observationId":e.id,"sourceId":e.source,"relation":e.relation,"resolution":e.resolution,"producer":e.producer,"evidenceIds":e.evidence_ids,"rowJson":serde_json::to_string(e)?});
        observations.push(json!({"key":identity(&("neo4j-observation-v1",plan.scope(),plan.revision().unwrap(),&e.id)),"sourceKey":key(plan,"entity",&e.source),"targetKey":target,"rowHash":identity(&props),"props":props}));
    }
    let raw = json!({"schemaVersion":1,"payloadHash":payload["payloadHash"],"graphHash":payload["graphHash"],"snapshotId":payload["snapshotId"],"workspaceId":payload["workspaceId"],"sourceFiles":payload["sourceFiles"],"coverage":payload["coverage"],"diagnostics":payload["diagnostics"],"sourceClaimsAttested":false,"sourceAuthority":false,"complete":false,"entityCount":entities.len(),"observationCount":observations.len(),"evidenceCount":evidence.len(),"unresolvedCount":unresolved.len()});
    let revision = row(
        plan,
        "revision",
        plan.revision().unwrap(),
        &raw,
        json!({"schemaVersion":1,"payloadHash":payload["payloadHash"],"graphHash":payload["graphHash"],"snapshotId":payload["snapshotId"],"workspaceId":payload["workspaceId"],"entityCount":entities.len(),"observationCount":observations.len(),"evidenceCount":evidence.len(),"unresolvedCount":unresolved.len(),"sourceAuthority":false,"complete":false}),
    );
    ensure!(
        revision["key"] == revision_key(plan.scope(), plan.revision().unwrap()),
        "neo4j_publish_program_invalid"
    );
    let result = json!({"scopeKey":plan.scope(),"planId":plan.id(),"revisionId":plan.revision(),"expectedRevision":plan.expected_revision(),"payloadHash":payload["payloadHash"],"entities":entities,"evidence":evidence,"observations":observations,"unresolved":unresolved,"revision":revision});
    crate::json::encode_bounded(&result, 32_000_000, d)?;
    Ok(result)
}
pub fn request(plan: &Plan, operation: Operation, d: &Deadline) -> Result<Value> {
    plan.validate(d)?;
    ensure!(
        operations(plan.mode()).contains(&operation),
        "neo4j_publish_operation_invalid"
    );
    let statement = match operation {
        Operation::ScopeConstraint => SCHEMA[0].into(),
        Operation::ElementConstraint => SCHEMA[1].into(),
        Operation::ObservationConstraint => SCHEMA[2].into(),
        Operation::ElementRevisionIndex => SCHEMA[3].into(),
        Operation::ObservationRevisionIndex => SCHEMA[4].into(),
        Operation::Namespace => NAMESPACE.into(),
        Operation::Publish => publication(),
    };
    let value = json!({"statement":statement,"parameters":parameters(plan,d)?});
    crate::json::encode_bounded(&value, 32_000_000, d)?;
    Ok(value)
}
