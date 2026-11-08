//! Compiled Cypher/Redis commands. Source values enter only encoded parameters.
use super::{Mode,Plan};
use crate::{capture::Deadline,model::identity,neo4j_publish::plan::validate_payload};
use anyhow::{Result,ensure};
use serde::{Serialize,Deserialize};
use serde_json::{Value,json};
use std::collections::BTreeMap;
#[derive(Clone,Copy,Debug,PartialEq,Eq,Serialize,Deserialize)]
#[serde(rename_all="snake_case")]
pub enum Operation{Initialize,ScopeIndex,ElementIndex,ObservationIndex,ElementRevisionIndex,ObservationRevisionIndex,ScopeConstraint,ElementConstraint,ObservationConstraint,Publish}
pub const SCOPE_READ:&str="MATCH (s:YokeNativeScope {key:$scopeKey}) WITH s LIMIT 2 RETURN s.key AS scopeKey,s.ownerKind AS ownerKind,s.schemaVersion AS schemaVersion,s.activeRevision AS activeRevision,s.activeRevisionKey AS activeRevisionKey,s.createdByPlan AS createdByPlan";
pub const CONSTRAINT_READ:&str="CALL db.constraints() YIELD type,label,properties,entitytype,status RETURN type,label,size(properties) AS propertyCount,properties[0] AS property0,entitytype,status ORDER BY label";
pub const INDEX_READ:&str="CALL db.indexes()";
const INITIALIZE:&str="CREATE (s:YokeNativeScope {key:$scopeKey,ownerKind:'yoke-native-falkor-v1',schemaVersion:1,activeRevision:'absent',activeRevisionKey:'absent',lockCounter:0,createdByPlan:$planId}) RETURN s.key AS scopeKey,s.schemaVersion AS schemaVersion,s.createdByPlan AS createdByPlan,s.activeRevision AS activeRevision";
const INDEXES:[&str;5]=[
    "CREATE INDEX FOR (s:YokeNativeScope) ON (s.key)",
    "CREATE INDEX FOR (n:YokeNativeElement) ON (n.key)",
    "CREATE INDEX FOR ()-[r:YOKE_NATIVE_OBSERVATION]-() ON (r.key)",
    "CREATE INDEX FOR (n:YokeNativeElement) ON (n.revisionId)",
    "CREATE INDEX FOR ()-[r:YOKE_NATIVE_OBSERVATION]-() ON (r.revisionId)",
];
pub fn operations(mode:Mode)->Vec<Operation>{if mode==Mode::Bootstrap{vec![Operation::Initialize,Operation::ScopeIndex,Operation::ElementIndex,Operation::ObservationIndex,Operation::ElementRevisionIndex,Operation::ObservationRevisionIndex,Operation::ScopeConstraint,Operation::ElementConstraint,Operation::ObservationConstraint]}else{vec![Operation::Publish]}}
fn publication()->String{
    let mut q=String::from("MATCH (scope:YokeNativeScope {key:$scopeKey}) WITH scope LIMIT 2 WITH collect(scope) AS scopes WHERE size(scopes)=1 UNWIND scopes AS scope SET scope.lockCounter=coalesce(scope.lockCounter,0)+1 WITH scope FOREACH (deny IN CASE WHEN scope.ownerKind='yoke-native-falkor-v1' AND scope.schemaVersion=1 AND (scope.activeRevision=$expectedRevision OR scope.activeRevision=$revisionId) THEN [] ELSE [1] END | CREATE (:YokeNativeScope {key:$scopeKey})) ");
    for (parameter,label) in [("entities","YokeNativeEntity"),("evidence","YokeNativeEvidence"),("unresolved","YokeNativeUnresolved"),("revision","YokeNativeRevision")]{
        let rows=if parameter=="revision"{"[$revision]".to_owned()}else{format!("${parameter}")};
        q.push_str(&format!("FOREACH (row IN {rows} | MERGE (n:YokeNativeElement:{label} {{key:row.key}}) ON CREATE SET n += row.props,n.rowHash=row.rowHash FOREACH (deny IN CASE WHEN n.rowHash=row.rowHash AND all(k IN keys(row.props) WHERE n[k]=row.props[k]) THEN [] ELSE [1] END | CREATE (:YokeNativeScope {{key:$scopeKey}}))) "));
    }
    q.push_str("FOREACH (row IN $observations | MERGE (src:YokeNativeElement {key:row.sourceKey}) MERGE (dst:YokeNativeElement {key:row.targetKey}) MERGE (src)-[r:YOKE_NATIVE_OBSERVATION {key:row.key}]->(dst) ON CREATE SET r += row.props,r.rowHash=row.rowHash FOREACH (deny IN CASE WHEN r.rowHash=row.rowHash AND all(k IN keys(row.props) WHERE r[k]=row.props[k]) THEN [] ELSE [1] END | CREATE (:YokeNativeScope {key:$scopeKey}))) WITH scope CALL { WITH scope MATCH (n:YokeNativeElement {scopeKey:$scopeKey,revisionId:$revisionId}) RETURN count(n) AS storedElements } CALL { WITH scope MATCH ()-[r:YOKE_NATIVE_OBSERVATION {scopeKey:$scopeKey,revisionId:$revisionId}]->() RETURN count(r) AS storedObservations } WITH scope,storedElements,storedObservations FOREACH (deny IN CASE WHEN storedElements=size($entities)+size($evidence)+size($unresolved)+1 AND storedObservations=size($observations) THEN [] ELSE [1] END | CREATE (:YokeNativeScope {key:$scopeKey})) SET scope.activeRevision=$revisionId,scope.activeRevisionKey=$revision.key RETURN scope.key AS scopeKey,scope.activeRevision AS revisionId,$payloadHash AS payloadHash,$planId AS planId,size($entities) AS entityCount,size($observations) AS observationCount,size($evidence) AS evidenceCount,size($unresolved) AS unresolvedCount");q
}
pub fn program_hash()->String{
    let policy=[include_str!("falkor_publish.rs"),include_str!("falkor_publish_plan.rs"),include_str!("falkor_publish_program.rs"),include_str!("falkor_wire.rs"),include_str!("neo4j_publish.rs"),include_str!("neo4j_publish_plan.rs")].map(|s|s.replace("\r\n","\n"));
    identity(&("falkor-fixed-program-lf-policy-v1",policy,INITIALIZE,INDEXES,publication(),SCOPE_READ,CONSTRAINT_READ,INDEX_READ))
}
pub(crate) fn revision_key(scope:&str,revision:&str)->String{identity(&("falkor-element-v1",scope,revision,"revision",revision))}
