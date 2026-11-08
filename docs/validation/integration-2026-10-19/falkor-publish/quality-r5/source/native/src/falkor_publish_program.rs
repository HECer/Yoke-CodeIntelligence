//! Compiled Cypher/Redis commands. Source values enter only encoded parameters.
use super::{Mode, Plan};
use crate::{capture::Deadline, model::identity, neo4j_publish::plan::validate_payload};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    Initialize,
    ScopeIndex,
    ElementIndex,
    ObservationIndex,
    ElementRevisionIndex,
    ObservationRevisionIndex,
    ScopeConstraint,
    ElementConstraint,
    ObservationConstraint,
    Publish,
}
pub const SCOPE_READ: &str = "MATCH (s:YokeNativeScope) WITH s LIMIT 2 RETURN s.key AS scopeKey,s.ownerKind AS ownerKind,s.schemaVersion AS schemaVersion,s.activeRevision AS activeRevision,s.activeRevisionKey AS activeRevisionKey,s.createdByPlan AS createdByPlan";
pub const CONSTRAINT_READ: &str = "CALL db.constraints() YIELD type,label,properties,entitytype,status RETURN type,label,size(properties) AS propertyCount,properties[0] AS property0,entitytype,status ORDER BY label";
pub const INDEX_READ: &str = "CALL db.indexes() YIELD label,properties,types,entitytype,status UNWIND properties AS property RETURN label,property,size(types[property]) AS typeCount,types[property][0] AS type0,entitytype,status ORDER BY label,property";
pub const REVISION_READ: &str = "MATCH (n:YokeNativeElement:YokeNativeRevision {key:$revisionKey}) WITH n LIMIT 2 CALL { WITH n MATCH (e:YokeNativeElement {scopeKey:n.scopeKey,revisionId:n.revisionId}) RETURN count(e) AS elementCount } CALL { WITH n MATCH ()-[r:YOKE_NATIVE_OBSERVATION {scopeKey:n.scopeKey,revisionId:n.revisionId}]->() RETURN count(r) AS observationCount } RETURN n.rowJson AS rowJson,n.rowHash AS rowHash,n.scopeKey AS scopeKey,n.revisionId AS revisionId,elementCount,observationCount";
const INITIALIZE: &str = "CREATE (s:YokeNativeScope {key:$scopeKey,ownerKind:'yoke-native-falkor-v1',schemaVersion:1,activeRevision:'absent',activeRevisionKey:'absent',lockCounter:0,createdByPlan:$planId}) RETURN s.key AS scopeKey,s.schemaVersion AS schemaVersion,s.createdByPlan AS createdByPlan,s.activeRevision AS activeRevision";
const INDEXES: [&str; 5] = [
    "CREATE INDEX FOR (s:YokeNativeScope) ON (s.key)",
    "CREATE INDEX FOR (n:YokeNativeElement) ON (n.key)",
    "CREATE INDEX FOR ()-[r:YOKE_NATIVE_OBSERVATION]-() ON (r.key)",
    "CREATE INDEX FOR (n:YokeNativeElement) ON (n.revisionId)",
    "CREATE INDEX FOR ()-[r:YOKE_NATIVE_OBSERVATION]-() ON (r.revisionId)",
];
pub fn operations(mode: Mode) -> Vec<Operation> {
    if mode == Mode::Bootstrap {
        vec![
            Operation::Initialize,
            Operation::ScopeIndex,
            Operation::ElementIndex,
            Operation::ObservationIndex,
            Operation::ElementRevisionIndex,
            Operation::ObservationRevisionIndex,
            Operation::ScopeConstraint,
            Operation::ElementConstraint,
            Operation::ObservationConstraint,
        ]
    } else {
        vec![Operation::Publish]
    }
}
fn publication() -> String {
    let mut q = String::from(
        "MATCH (scope:YokeNativeScope {key:$scopeKey}) WITH scope LIMIT 2 WITH collect(scope) AS scopes WHERE size(scopes)=1 UNWIND scopes AS scope SET scope.lockCounter=coalesce(scope.lockCounter,0)+1 WITH scope FOREACH (deny IN CASE WHEN scope.ownerKind='yoke-native-falkor-v1' AND scope.schemaVersion=1 AND (scope.activeRevision=$expectedRevision OR scope.activeRevision=$revisionId) THEN [] ELSE [1] END | CREATE (:YokeNativeScope {key:$scopeKey})) ",
    );
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
    q.push_str("FOREACH (row IN $observations | MERGE (src:YokeNativeElement {key:row.sourceKey}) MERGE (dst:YokeNativeElement {key:row.targetKey}) MERGE (src)-[r:YOKE_NATIVE_OBSERVATION {key:row.key}]->(dst) ON CREATE SET r += row.props,r.rowHash=row.rowHash FOREACH (deny IN CASE WHEN r.rowHash=row.rowHash AND all(k IN keys(row.props) WHERE r[k]=row.props[k]) THEN [] ELSE [1] END | CREATE (:YokeNativeScope {key:$scopeKey}))) WITH scope CALL { WITH scope MATCH (n:YokeNativeElement {scopeKey:$scopeKey,revisionId:$revisionId}) RETURN count(n) AS storedElements } CALL { WITH scope MATCH ()-[r:YOKE_NATIVE_OBSERVATION {scopeKey:$scopeKey,revisionId:$revisionId}]->() RETURN count(r) AS storedObservations } WITH scope,storedElements,storedObservations FOREACH (deny IN CASE WHEN storedElements=size($entities)+size($evidence)+size($unresolved)+1 AND storedObservations=size($observations) THEN [] ELSE [1] END | CREATE (:YokeNativeScope {key:$scopeKey})) SET scope.activeRevision=$revisionId,scope.activeRevisionKey=$revision.key RETURN scope.key AS scopeKey,scope.activeRevision AS revisionId,$payloadHash AS payloadHash,$planId AS planId,size($entities) AS entityCount,size($observations) AS observationCount,size($evidence) AS evidenceCount,size($unresolved) AS unresolvedCount");
    q
}
pub fn program_hash() -> String {
    let policy = [
        include_str!("falkor_publish.rs"),
        include_str!("falkor_publish_plan.rs"),
        include_str!("falkor_publish_program.rs"),
        include_str!("falkor_wire.rs"),
        include_str!("falkor_publish_transport.rs"),
        include_str!("falkor_publish_checks.rs"),
        include_str!("falkor_publish_journal.rs"),
        include_str!("falkor_publish_client.rs"),
        include_str!("falkor_publish_cli.rs"),
        include_str!("neo4j_publish.rs"),
        include_str!("neo4j_publish_plan.rs"),
    ]
    .map(|s| s.replace("\r\n", "\n"));
    identity(&(
        "falkor-fixed-program-lf-policy-v1",
        policy,
        INITIALIZE,
        INDEXES,
        publication(),
        SCOPE_READ,
        CONSTRAINT_READ,
        INDEX_READ,
        REVISION_READ,
    ))
}
pub(crate) fn revision_key(scope: &str, revision: &str) -> String {
    identity(&("falkor-element-v1", scope, revision, "revision", revision))
}
fn key(plan: &Plan, category: &str, id: &str) -> String {
    identity(&(
        "falkor-element-v1",
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
        observations.push(json!({"key":identity(&("falkor-observation-v1",plan.scope(),plan.revision().unwrap(),&e.id)),"sourceKey":key(plan,"entity",&e.source),"targetKey":target,"rowHash":identity(&props),"props":props}));
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
        "falkor_publish_program_invalid"
    );
    let result = json!({"scopeKey":plan.scope(),"planId":plan.id(),"revisionId":plan.revision(),"expectedRevision":plan.expected_revision(),"payloadHash":payload["payloadHash"],"entities":entities,"evidence":evidence,"observations":observations,"unresolved":unresolved,"revision":revision});
    crate::json::encode_bounded(&result, 32_000_000, d)?;
    Ok(result)
}
fn literal(value: &Value, out: &mut String, depth: usize, d: &Deadline) -> Result<()> {
    d.check()?;
    ensure!(depth <= 16, "falkor_publish_parameter_capacity_exceeded");
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(v) => out.push_str(if *v { "true" } else { "false" }),
        Value::Number(n) => {
            ensure!(n.as_i64().is_some(), "falkor_publish_parameter_invalid");
            out.push_str(&n.to_string());
        }
        Value::String(v) => out.push_str(&serde_json::to_string(v)?),
        Value::Array(values) => {
            out.push('[');
            for (i, value) in values.iter().enumerate() {
                if i > 0 {
                    out.push(',')
                }
                literal(value, out, depth + 1, d)?;
            }
            out.push(']');
        }
        Value::Object(values) => {
            out.push('{');
            for (i, (key, value)) in values.iter().enumerate() {
                ensure!(
                    !key.is_empty()
                        && key.bytes().enumerate().all(|(i, b)| b.is_ascii_alphabetic()
                            || b == b'_'
                            || (i > 0 && b.is_ascii_digit())),
                    "falkor_publish_parameter_key_invalid"
                );
                if i > 0 {
                    out.push(',')
                }
                out.push_str(key);
                out.push(':');
                literal(value, out, depth + 1, d)?;
            }
            out.push('}');
        }
    }
    ensure!(
        out.len() <= 32_000_000,
        "falkor_publish_parameter_capacity_exceeded"
    );
    Ok(())
}
pub(crate) fn query(
    graph_key: &str,
    statement: &str,
    params: &Value,
    read_only: bool,
    d: &Deadline,
) -> Result<Vec<Vec<u8>>> {
    d.check()?;
    crate::json::encode_bounded(params, 32_000_000, d)?;
    let object = params
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("falkor_publish_parameter_invalid"))?;
    let mut text = String::new();
    if !object.is_empty() {
        text.push_str("CYPHER ");
        for (key, value) in object {
            ensure!(
                !key.is_empty()
                    && key.bytes().enumerate().all(|(i, b)| b.is_ascii_alphabetic()
                        || b == b'_'
                        || (i > 0 && b.is_ascii_digit())),
                "falkor_publish_parameter_key_invalid"
            );
            text.push_str(key);
            text.push('=');
            literal(value, &mut text, 0, d)?;
            text.push(' ');
        }
    }
    text.push_str(statement);
    ensure!(
        text.len() <= 32_000_000,
        "falkor_publish_parameter_capacity_exceeded"
    );
    let command = vec![
        if read_only {
            b"GRAPH.RO_QUERY".to_vec()
        } else {
            b"GRAPH.QUERY".to_vec()
        },
        graph_key.as_bytes().to_vec(),
        text.into_bytes(),
        b"TIMEOUT".to_vec(),
        d.remaining()
            .as_millis()
            .clamp(1, 600000)
            .to_string()
            .into_bytes(),
    ];
    crate::falkor_wire::encode(
        &command.iter().map(Vec::as_slice).collect::<Vec<_>>(),
        &Default::default(),
        d,
    )?;
    Ok(command)
}
pub fn command(plan: &Plan, op: Operation, d: &Deadline) -> Result<Vec<Vec<u8>>> {
    plan.validate(d)?;
    ensure!(
        operations(plan.mode()).contains(&op),
        "falkor_publish_operation_invalid"
    );
    let definition = match op {
        Operation::ScopeConstraint => Some(("NODE", "YokeNativeScope")),
        Operation::ElementConstraint => Some(("NODE", "YokeNativeElement")),
        Operation::ObservationConstraint => Some(("RELATIONSHIP", "YOKE_NATIVE_OBSERVATION")),
        _ => None,
    };
    if let Some((entity, label)) = definition {
        return Ok([
            "GRAPH.CONSTRAINT",
            "CREATE",
            plan.graph_key(),
            "UNIQUE",
            entity,
            label,
            "PROPERTIES",
            "1",
            "key",
        ]
        .map(|s| s.as_bytes().to_vec())
        .to_vec());
    }
    let statement = match op {
        Operation::Initialize => INITIALIZE.to_owned(),
        Operation::ScopeIndex => INDEXES[0].into(),
        Operation::ElementIndex => INDEXES[1].into(),
        Operation::ObservationIndex => INDEXES[2].into(),
        Operation::ElementRevisionIndex => INDEXES[3].into(),
        Operation::ObservationRevisionIndex => INDEXES[4].into(),
        Operation::Publish => publication(),
        _ => unreachable!(),
    };
    query(
        plan.graph_key(),
        &statement,
        &parameters(plan, d)?,
        false,
        d,
    )
}
