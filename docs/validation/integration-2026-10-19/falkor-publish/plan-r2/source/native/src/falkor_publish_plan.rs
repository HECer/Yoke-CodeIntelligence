use crate::{
    capture::Deadline,
    model::{Graph, Snapshot, identity, valid_id},
    store::Store,
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::net::SocketAddr;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Target {
    pub base_url: String,
    pub address: SocketAddr,
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
    pub(crate) fn url(&self) -> Result<url::Url> {
        ensure!(
            self.base_url.len() <= 4096
                && self.base_url.is_ascii()
                && !self.base_url.contains('\\')
                && !self.base_url.chars().any(char::is_control),
            "falkor_publish_target_invalid"
        );
        let url = url::Url::parse(&self.base_url)
            .map_err(|_| anyhow::anyhow!("falkor_publish_target_invalid"))?;
        ensure!(
            matches!(url.scheme(), "redis" | "rediss")
                && url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none()
                && url.query().is_none()
                && url.fragment().is_none()
                && url.path() == "/"
                && url.port() == Some(self.address.port()),
            "falkor_publish_target_invalid"
        );
        Ok(url)
    }
    pub fn validate(&self) -> Result<()> {
        let url = self.url()?;
        ensure!(
            self.address.port() > 0
                && !self.address.ip().is_unspecified()
                && !self.address.ip().is_multicast()
                && !self.namespace.is_empty()
                && self.namespace.len() <= 256
                && !self.namespace.chars().any(char::is_control)
                && !self.user.is_empty()
                && self.user.len() <= 256
                && !self.user.chars().any(char::is_control),
            "falkor_publish_target_invalid"
        );
        let literal = match url.host() {
            Some(url::Host::Ipv4(ip)) => Some(std::net::IpAddr::V4(ip)),
            Some(url::Host::Ipv6(ip)) => Some(std::net::IpAddr::V6(ip)),
            // Non-special URL schemes keep dotted IPv4 hosts as domains.
            _ => url
                .host_str()
                .and_then(|host| host.parse::<std::net::IpAddr>().ok()),
        };
        if let Some(ip) = literal {
            ensure!(ip == self.address.ip(), "falkor_publish_target_invalid")
        }
        if url.scheme() == "redis" {
            ensure!(
                literal.is_some_and(|ip| ip.is_loopback()) && self.address.ip().is_loopback(),
                "falkor_publish_plaintext_requires_literal_loopback"
            )
        }
        ensure!(
            self.root_certificates_der_base64.len() <= 16,
            "falkor_publish_tls_roots_invalid"
        );
        use base64::Engine as _;
        for encoded in &self.root_certificates_der_base64 {
            ensure!(encoded.len() <= 22000, "falkor_publish_tls_roots_invalid");
            let der = base64::prelude::BASE64_STANDARD
                .decode(encoded)
                .map_err(|_| anyhow::anyhow!("falkor_publish_tls_roots_invalid"))?;
            ensure!(
                (100..=16384).contains(&der.len()),
                "falkor_publish_tls_roots_invalid"
            );
            let mut roots = rustls::RootCertStore::empty();
            roots
                .add(rustls::pki_types::CertificateDer::from(der))
                .map_err(|_| anyhow::anyhow!("falkor_publish_tls_roots_invalid"))?;
        }
        ensure!(
            url.scheme() != "rediss"
                || self.use_system_roots
                || !self.root_certificates_der_base64.is_empty(),
            "falkor_publish_tls_roots_required"
        );
        Ok(())
    }
    pub fn scope_key(&self) -> Result<String> {
        self.validate()?;
        Ok(identity(&(
            "falkor-scope-v1",
            self.url()?.as_str(),
            0,
            &self.namespace,
        )))
    }
    pub fn graph_key(&self) -> Result<String> {
        Ok(format!("yoke:native:v1:{}", self.scope_key()?))
    }
    pub fn credential_binding(&self) -> Result<String> {
        self.validate()?;
        Ok(identity(&(
            "falkor-credential-v1",
            self.url()?.as_str(),
            self.address,
            0,
            &self.user,
            self.use_system_roots,
            &self.root_certificates_der_base64,
        )))
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Bootstrap,
    Publish,
}
#[derive(Clone, Debug)]
pub struct Plan {
    target: Target,
    value: Value,
}
impl Plan {
    fn create(
        target: Target,
        mode: Mode,
        expected: Option<&str>,
        payload: Option<Value>,
        d: &Deadline,
    ) -> Result<Self> {
        d.check()?;
        target.validate()?;
        let scope = target.scope_key()?;
        let program = super::program::program_hash();
        let revision = payload
            .as_ref()
            .map(|p| identity(&("falkor-revision-v1", &scope, &p["payloadHash"], &program)));
        let mut value = json!({"schemaVersion":1,"kind":"native_falkor_plan_v1","mode":mode,"programHash":program,"target":target,"scopeKey":scope,"graphKey":target.graph_key()?,"expectedRevision":expected,"revisionId":revision,"payload":payload,"hostAuthorityGranted":false,"sourceClaimsAttested":false});
        value["planId"] = json!(identity(&value));
        let plan = Self { target, value };
        plan.validate(d)?;
        Ok(plan)
    }
    pub fn bootstrap(target: Target, d: &Deadline) -> Result<Self> {
        Self::create(target, Mode::Bootstrap, None, None, d)
    }
    pub fn prepare(
        store: &Store,
        snapshot: &Snapshot,
        graph: &Graph,
        target: Target,
        expected_revision: &str,
        limits: &super::PayloadLimits,
        d: &Deadline,
    ) -> Result<Self> {
        ensure!(
            expected_revision == "absent" || valid_id(expected_revision),
            "falkor_publish_precondition_invalid"
        );
        // A shared, data-only native base-graph export codec retains its v1 wire
        // kind. Its Neo4j origin does not grant any endpoint or write authority.
        let payload = crate::neo4j_publish::build_payload(store, snapshot, graph, limits, d)?;
        Self::create(
            target,
            Mode::Publish,
            Some(expected_revision),
            Some(payload),
            d,
        )
    }
    pub fn validate(&self, d: &Deadline) -> Result<()> {
        d.check()?;
        self.target.validate()?;
        crate::json::encode_bounded(&self.value, 32_000_000, d)?;
        let keys = [
            "schemaVersion",
            "kind",
            "mode",
            "programHash",
            "target",
            "scopeKey",
            "graphKey",
            "expectedRevision",
            "revisionId",
            "payload",
            "hostAuthorityGranted",
            "sourceClaimsAttested",
            "planId",
        ];
        let object = self
            .value
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("falkor_publish_plan_invalid"))?;
        ensure!(
            object.len() == keys.len()
                && keys.iter().all(|k| object.contains_key(*k))
                && self.value["schemaVersion"] == 1
                && self.value["kind"] == "native_falkor_plan_v1"
                && self.value["programHash"] == super::program::program_hash()
                && self.value["target"] == serde_json::to_value(&self.target)?
                && self.value["scopeKey"] == self.target.scope_key()?
                && self.value["graphKey"] == self.target.graph_key()?
                && self.value["hostAuthorityGranted"] == false
                && self.value["sourceClaimsAttested"] == false,
            "falkor_publish_plan_invalid"
        );
        let mode: Mode = serde_json::from_value(self.value["mode"].clone())
            .map_err(|_| anyhow::anyhow!("falkor_publish_plan_invalid"))?;
        if mode == Mode::Bootstrap {
            ensure!(
                self.value["payload"].is_null()
                    && self.value["revisionId"].is_null()
                    && self.value["expectedRevision"].is_null(),
                "falkor_publish_plan_invalid"
            )
        } else {
            ensure!(
                self.value["expectedRevision"]
                    .as_str()
                    .is_some_and(|s| s == "absent" || valid_id(s)),
                "falkor_publish_precondition_invalid"
            );
            crate::neo4j_publish::plan::validate_payload(&self.value["payload"], d)?;
            ensure!(
                self.value["revisionId"]
                    == identity(&(
                        "falkor-revision-v1",
                        &self.value["scopeKey"],
                        &self.value["payload"]["payloadHash"],
                        &self.value["programHash"]
                    )),
                "falkor_publish_plan_invalid"
            );
        }
        let mut plain = self.value.clone();
        let id = plain.as_object_mut().unwrap().remove("planId").unwrap();
        ensure!(
            id.as_str().is_some_and(valid_id) && id == identity(&plain),
            "falkor_publish_plan_invalid"
        );
        d.check()
    }
    pub fn encode(&self, d: &Deadline) -> Result<Vec<u8>> {
        self.validate(d)?;
        crate::json::encode_bounded(&self.value, 32_000_000, d)
    }
    pub fn decode(bytes: &[u8], d: &Deadline) -> Result<Self> {
        d.check()?;
        let value = crate::json::parse(bytes, 32_000_000)
            .map_err(|_| anyhow::anyhow!("falkor_publish_plan_invalid"))?;
        let target: Target = crate::json::decode(value["target"].clone())
            .map_err(|_| anyhow::anyhow!("falkor_publish_plan_invalid"))?;
        let plan = Self { target, value };
        plan.validate(d)?;
        Ok(plan)
    }
    pub fn id(&self) -> &str {
        self.value["planId"].as_str().unwrap()
    }
    pub fn scope(&self) -> &str {
        self.value["scopeKey"].as_str().unwrap()
    }
    pub fn graph_key(&self) -> &str {
        self.value["graphKey"].as_str().unwrap()
    }
    pub fn revision(&self) -> Option<&str> {
        self.value["revisionId"].as_str()
    }
    pub fn expected_revision(&self) -> Option<&str> {
        self.value["expectedRevision"].as_str()
    }
    pub fn payload(&self) -> Option<&Value> {
        if self.mode() == Mode::Publish {
            Some(&self.value["payload"])
        } else {
            None
        }
    }
    pub fn mode(&self) -> Mode {
        if self.value["mode"] == "bootstrap" {
            Mode::Bootstrap
        } else {
            Mode::Publish
        }
    }
    pub fn target(&self) -> &Target {
        &self.target
    }
    pub fn inspect(&self, d: &Deadline) -> Result<Value> {
        self.validate(d)?;
        Ok(
            json!({"kind":"native_falkor_plan_inspection_v1","planId":self.id(),"mode":self.mode(),"target":self.target,"scopeKey":self.scope(),"graphKey":self.graph_key(),"expectedRevision":self.expected_revision(),"revisionId":self.revision(),"schemaWriteRequired":self.mode()==Mode::Bootstrap,"containsSourceDerivedText":self.mode()==Mode::Publish,"credentialBinding":self.target.credential_binding()?,"hostAuthorityGranted":false,"sourceClaimsAttested":false,"sourceAuthority":false,"complete":false,"networkConnections":0,"modelCalls":0,"guardedPublicationPlatforms":["windows"]}),
        )
    }
}
