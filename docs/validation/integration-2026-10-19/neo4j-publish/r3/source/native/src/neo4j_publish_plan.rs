//! Closed, offline publication plans. Serialized plans grant no host authority.
use super::Limits;
use crate::{
    capture::Deadline,
    model::{Graph, Snapshot, SourceFile, identity, valid_id},
    store::Store,
};
use anyhow::{Result, ensure};
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    net::SocketAddr,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Target {
    pub base_url: String,
    pub address: SocketAddr,
    pub database: String,
    pub namespace: String,
    pub user: String,
    #[serde(default = "system_roots")]
    pub use_system_roots: bool,
    #[serde(default)]
    pub root_certificates_der_base64: Vec<String>,
}
fn system_roots() -> bool {
    true
}
impl Target {
    pub fn validate(&self) -> Result<()> {
        let url = self.url()?;
        ensure!(
            !self.database.is_empty()
                && self.database.len() <= 63
                && self.database != "system"
                && self.database.as_bytes()[0].is_ascii_alphanumeric()
                && self
                    .database
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b)),
            "neo4j_publish_target_invalid"
        );
        ensure!(
            !self.namespace.is_empty()
                && self.namespace.len() <= 256
                && !self.namespace.chars().any(char::is_control),
            "neo4j_publish_target_invalid"
        );
        ensure!(
            !self.user.is_empty()
                && self.user.len() <= 256
                && !self.user.contains(':')
                && !self.user.chars().any(char::is_control),
            "neo4j_publish_target_invalid"
        );
        ensure!(
            self.address.port() > 0
                && !self.address.ip().is_unspecified()
                && !self.address.ip().is_multicast()
                && url.port_or_known_default() == Some(self.address.port()),
            "neo4j_publish_target_invalid"
        );
        let literal = match url.host() {
            Some(url::Host::Ipv4(ip)) => Some(std::net::IpAddr::V4(ip)),
            Some(url::Host::Ipv6(ip)) => Some(std::net::IpAddr::V6(ip)),
            _ => None,
        };
        if let Some(ip) = literal {
            ensure!(ip == self.address.ip(), "neo4j_publish_target_invalid");
        }
        if url.scheme() == "http" {
            ensure!(
                literal.is_some_and(|ip| ip.is_loopback()) && self.address.ip().is_loopback(),
                "neo4j_publish_plain_http_requires_literal_loopback"
            );
        }
        ensure!(
            self.root_certificates_der_base64.len() <= 16,
            "neo4j_publish_target_invalid"
        );
        for encoded in &self.root_certificates_der_base64 {
            ensure!(encoded.len() <= 22000, "neo4j_publish_target_invalid");
            let der = base64::prelude::BASE64_STANDARD
                .decode(encoded)
                .map_err(|_| anyhow::anyhow!("neo4j_publish_tls_roots_invalid"))?;
            ensure!(
                (100..=16384).contains(&der.len()),
                "neo4j_publish_tls_roots_invalid"
            );
            reqwest::Certificate::from_der(&der)
                .map_err(|_| anyhow::anyhow!("neo4j_publish_tls_roots_invalid"))?;
        }
        ensure!(
            url.scheme() != "https"
                || self.use_system_roots
                || !self.root_certificates_der_base64.is_empty(),
            "neo4j_publish_tls_roots_required"
        );
        Ok(())
    }
    pub(crate) fn url(&self) -> Result<url::Url> {
        ensure!(
            self.base_url.len() <= 4096
                && self.base_url.is_ascii()
                && !self.base_url.contains('\\')
                && !self.base_url.chars().any(char::is_control),
            "neo4j_publish_target_invalid"
        );
        let url = url::Url::parse(&self.base_url)
            .map_err(|_| anyhow::anyhow!("neo4j_publish_target_invalid"))?;
        ensure!(
            matches!(url.scheme(), "http" | "https")
                && url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none()
                && url.query().is_none()
                && url.fragment().is_none()
                && url.path() == "/",
            "neo4j_publish_target_invalid"
        );
        Ok(url)
    }
    pub fn credential_binding(&self) -> Result<String> {
        self.validate()?;
        Ok(identity(&(
            "neo4j-credential-v1",
            self.url()?.as_str(),
            self.address,
            &self.database,
            &self.user,
            self.use_system_roots,
            &self.root_certificates_der_base64,
        )))
    }
    pub fn scope_key(&self) -> Result<String> {
        self.validate()?;
        Ok(identity(&(
            "neo4j-scope-v1",
            self.url()?.as_str(),
            &self.database,
            &self.namespace,
        )))
    }
    pub(crate) fn query_url(&self) -> Result<url::Url> {
        let mut url = self.url()?;
        url.set_path(&format!("/db/{}/query/v2", self.database));
        Ok(url)
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Bootstrap,
    Publish,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    schema_version: u32,
    kind: String,
    mode: Mode,
    program_hash: String,
    plan_id: String,
    target: Target,
    scope_key: String,
    expected_revision: Option<String>,
    revision_id: Option<String>,
    payload: Option<Value>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EncodedPlan {
    schema_version: u32,
    kind: String,
    mode: Mode,
    program_hash: String,
    plan_id: String,
    target: Target,
    scope_key: String,
    expected_revision: Option<String>,
    revision_id: Option<String>,
    payload: Option<Value>,
}
impl Plan {
    pub fn bootstrap(target: Target, d: &Deadline) -> Result<Self> {
        d.check()?;
        target.validate()?;
        let scope_key = target.scope_key()?;
        let mut plan = Self {
            schema_version: 1,
            kind: "native_neo4j_plan_v1".into(),
            mode: Mode::Bootstrap,
            program_hash: super::program::program_hash(),
            plan_id: String::new(),
            target,
            scope_key,
            expected_revision: None,
            revision_id: None,
            payload: None,
        };
        plan.plan_id = plan.identity(d)?;
        plan.validate(d)?;
        Ok(plan)
    }
    pub fn prepare(
        store: &Store,
        snapshot: &Snapshot,
        graph: &Graph,
        target: Target,
        expected_revision: &str,
        limits: &Limits,
        d: &Deadline,
    ) -> Result<Self> {
        d.check()?;
        target.validate()?;
        ensure!(
            expected_revision == "absent" || valid_id(expected_revision),
            "neo4j_publish_precondition_invalid"
        );
        let scope_key = target.scope_key()?;
        let payload = super::build_payload(store, snapshot, graph, limits, d)?;
        let revision = identity(&(
            "neo4j-revision-v1",
            &scope_key,
            &payload["payloadHash"],
            super::program::program_hash(),
        ));
        let mut plan = Self {
            schema_version: 1,
            kind: "native_neo4j_plan_v1".into(),
            mode: Mode::Publish,
            program_hash: super::program::program_hash(),
            plan_id: String::new(),
            target,
            scope_key,
            expected_revision: Some(expected_revision.into()),
            revision_id: Some(revision),
            payload: Some(payload),
        };
        plan.plan_id = plan.identity(d)?;
        plan.validate(d)?;
        Ok(plan)
    }
    fn identity(&self, d: &Deadline) -> Result<String> {
        let bytes = crate::json::encode_bounded(
            &(
                self.schema_version,
                &self.kind,
                self.mode,
                &self.program_hash,
                &self.target,
                &self.scope_key,
                &self.expected_revision,
                &self.revision_id,
                &self.payload,
            ),
            32_000_000,
            d,
        )?;
        Ok(crate::model::hash(bytes))
    }
    pub fn validate(&self, d: &Deadline) -> Result<()> {
        d.check()?;
        self.target.validate()?;
        ensure!(
            self.schema_version == 1
                && self.kind == "native_neo4j_plan_v1"
                && valid_id(&self.plan_id)
                && self.program_hash == super::program::program_hash()
                && self.scope_key == self.target.scope_key()?,
            "neo4j_publish_plan_invalid"
        );
        match self.mode {
            Mode::Bootstrap => ensure!(
                self.expected_revision.is_none()
                    && self.revision_id.is_none()
                    && self.payload.is_none(),
                "neo4j_publish_plan_invalid"
            ),
            Mode::Publish => {
                let expected = self
                    .expected_revision
                    .as_deref()
                    .ok_or_else(|| anyhow::anyhow!("neo4j_publish_plan_invalid"))?;
                ensure!(
                    expected == "absent" || valid_id(expected),
                    "neo4j_publish_precondition_invalid"
                );
                let payload = self
                    .payload
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("neo4j_publish_plan_invalid"))?;
                validate_payload(payload, d)?;
                ensure!(
                    self.revision_id.as_deref()
                        == Some(
                            identity(&(
                                "neo4j-revision-v1",
                                &self.scope_key,
                                &payload["payloadHash"],
                                super::program::program_hash()
                            ))
                            .as_str()
                        ),
                    "neo4j_publish_plan_invalid"
                );
            }
        }
        ensure!(
            self.plan_id == self.identity(d)?,
            "neo4j_publish_plan_invalid"
        );
        d.check()?;
        Ok(())
    }
    pub fn encode(&self, d: &Deadline) -> Result<Vec<u8>> {
        self.validate(d)?;
        crate::json::encode_bounded(self, 32_000_000, d)
    }
    pub fn decode(bytes: &[u8], d: &Deadline) -> Result<Self> {
        d.check()?;
        let raw: EncodedPlan =
            crate::json::parse(bytes, 32_000_000).and_then(crate::json::decode)?;
        let plan = Self {
            schema_version: raw.schema_version,
            kind: raw.kind,
            mode: raw.mode,
            program_hash: raw.program_hash,
            plan_id: raw.plan_id,
            target: raw.target,
            scope_key: raw.scope_key,
            expected_revision: raw.expected_revision,
            revision_id: raw.revision_id,
            payload: raw.payload,
        };
        plan.validate(d)?;
        Ok(plan)
    }
    pub fn id(&self) -> &str {
        &self.plan_id
    }
    pub fn mode(&self) -> Mode {
        self.mode
    }
    pub fn target(&self) -> &Target {
        &self.target
    }
    pub fn scope(&self) -> &str {
        &self.scope_key
    }
    pub fn revision(&self) -> Option<&str> {
        self.revision_id.as_deref()
    }
    pub fn expected_revision(&self) -> Option<&str> {
        self.expected_revision.as_deref()
    }
    pub fn payload(&self) -> Option<&Value> {
        self.payload.as_ref()
    }
    pub fn inspect(&self, d: &Deadline) -> Result<Value> {
        self.validate(d)?;
        let payload = self.payload.as_ref();
        Ok(
            json!({"kind":"native_neo4j_plan_inspection_v1","planId":self.plan_id,"mode":self.mode,"scopeKey":self.scope_key,"revisionId":self.revision_id,"expectedRevision":self.expected_revision,"programHash":self.program_hash,"target":{"baseUrl":self.target.url()?.as_str(),"address":self.target.address,"database":self.target.database,"namespace":self.target.namespace,"user":self.target.user,"credentialBinding":self.target.credential_binding()?},"entityCount":payload.and_then(|v|v["entities"].as_array()).map_or(0,Vec::len),"observationCount":payload.and_then(|v|v["observations"].as_array()).map_or(0,Vec::len),"evidenceCount":payload.and_then(|v|v["evidence"].as_array()).map_or(0,Vec::len),"schemaWriteRequired":self.mode==Mode::Bootstrap,"containsSourceDerivedText":self.mode==Mode::Publish,"hostAuthorityGranted":false,"sourceClaimsAttested":false,"sourceAuthority":false,"complete":false,"networkConnections":0,"modelCalls":0}),
        )
    }
}

pub(crate) fn validate_payload(value: &Value, d: &Deadline) -> Result<Graph> {
    d.check()?;
    crate::json::encode_bounded(value, 16_000_000, d)?;
    let keys = [
        "schemaVersion",
        "kind",
        "snapshotId",
        "workspaceId",
        "graphHash",
        "sourceFiles",
        "entities",
        "observations",
        "evidence",
        "diagnostics",
        "coverage",
        "sourceValidation",
        "sourceAuthority",
        "canonicalCodeSnapshot",
        "complete",
        "networkConnections",
        "modelCalls",
        "projectCodeExecutionAuthorized",
        "limitations",
        "payloadHash",
    ];
    let object = value
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("neo4j_publish_payload_invalid"))?;
    ensure!(
        object.len() == keys.len() && keys.iter().all(|k| object.contains_key(*k)),
        "neo4j_publish_payload_invalid"
    );
    ensure!(
        value["schemaVersion"] == 1
            && value["kind"] == "native_neo4j_graph_payload_v1"
            && value["sourceAuthority"] == false
            && value["canonicalCodeSnapshot"] == false
            && value["complete"] == false
            && value["networkConnections"] == 0
            && value["modelCalls"] == 0
            && value["projectCodeExecutionAuthorized"] == false
            && value["sourceValidation"] == "native_published_base_graph_verified_before_export",
        "neo4j_publish_payload_invalid"
    );
    let mut plain = value.clone();
    let supplied = plain
        .as_object_mut()
        .unwrap()
        .remove("payloadHash")
        .unwrap();
    ensure!(
        supplied == identity(&plain),
        "neo4j_publish_payload_invalid"
    );
    let graph: Graph = crate::json::decode(
        json!({"snapshotId":value["snapshotId"],"entities":value["entities"],"observations":value["observations"],"evidence":value["evidence"],"diagnostics":value["diagnostics"],"coverage":value["coverage"]}),
    )?;
    let files: Vec<SourceFile> = crate::json::decode(value["sourceFiles"].clone())?;
    ensure!(
        valid_id(&graph.snapshot_id)
            && value["workspaceId"].as_str().is_some_and(valid_id)
            && identity(&graph) == value["graphHash"]
            && graph.entities.len() <= 5000
            && graph.observations.len() <= 50000
            && graph.evidence.len() <= 50000
            && files.len() <= 5000
            && graph.diagnostics.len() <= 50000,
        "neo4j_publish_payload_invalid"
    );
    for (name, typed) in [
        ("entities", json!(graph.entities)),
        ("observations", json!(graph.observations)),
        ("evidence", json!(graph.evidence)),
        ("diagnostics", json!(graph.diagnostics)),
        ("coverage", json!(graph.coverage)),
        ("sourceFiles", json!(files)),
    ] {
        ensure!(value[name] == typed, "neo4j_publish_payload_invalid");
    }
    let mut by_path = BTreeMap::new();
    for file in &files {
        d.check()?;
        crate::paths::normalized_syntax(&file.path)?;
        ensure!(
            valid_id(&file.hash)
                && file.size <= i64::MAX as usize
                && by_path.insert(file.path.as_str(), file).is_none(),
            "neo4j_publish_payload_invalid"
        );
    }
    let mut entities = BTreeSet::new();
    for node in &graph.entities {
        d.check()?;
        let file = by_path
            .get(node.path.as_str())
            .ok_or_else(|| anyhow::anyhow!("neo4j_publish_payload_invalid"))?;
        ensure!(
            valid_id(&node.id)
                && entities.insert(node.id.as_str())
                && node.snapshot_id == graph.snapshot_id
                && node.start_byte <= node.end_byte
                && node.end_byte <= file.size,
            "neo4j_publish_payload_invalid"
        );
    }
    let mut evidence = BTreeSet::new();
    for anchor in &graph.evidence {
        d.check()?;
        let file = by_path
            .get(anchor.path.as_str())
            .ok_or_else(|| anyhow::anyhow!("neo4j_publish_payload_invalid"))?;
        ensure!(
            valid_id(&anchor.id)
                && evidence.insert(anchor.id.as_str())
                && anchor.snapshot_id == graph.snapshot_id
                && anchor.content_hash == file.hash
                && anchor.start_byte <= anchor.end_byte
                && anchor.end_byte <= file.size,
            "neo4j_publish_payload_invalid"
        );
    }
    let mut observations = BTreeSet::new();
    for row in &graph.observations {
        d.check()?;
        ensure!(
            valid_id(&row.id)
                && observations.insert(row.id.as_str())
                && row.snapshot_id == graph.snapshot_id
                && entities.contains(row.source.as_str())
                && row
                    .target
                    .as_ref()
                    .is_none_or(|id| entities.contains(id.as_str()))
                && row.evidence_ids.iter().collect::<BTreeSet<_>>().len() == row.evidence_ids.len()
                && row
                    .evidence_ids
                    .iter()
                    .all(|id| evidence.contains(id.as_str())),
            "neo4j_publish_payload_invalid"
        );
    }
    d.check()?;
    Ok(graph)
}
