//! Explicit received-catalog data; never a canonical code SourceSnapshot.
use crate::capture::Deadline;
use anyhow::{Result, bail};
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Limits {
    pub max_response_bytes: usize,
    pub max_record_bytes: usize,
    pub max_relations: usize,
    pub max_routines: usize,
    pub max_foreign_keys: usize,
    pub max_nodes: usize,
    pub max_observations: usize,
    pub max_cell_bytes: usize,
    pub max_archive_bytes: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_response_bytes: 16_000_000,
            max_record_bytes: 1_000_000,
            max_relations: 4096,
            max_routines: 4096,
            max_foreign_keys: 8192,
            max_nodes: 16384,
            max_observations: 8192,
            max_cell_bytes: 65536,
            max_archive_bytes: 32_000_000,
        }
    }
}
pub(crate) fn input(ok: bool) -> Result<()> {
    if !ok {
        bail!("postgres_catalog_input_invalid")
    }
    Ok(())
}
pub(crate) fn capacity(ok: bool) -> Result<()> {
    if !ok {
        bail!("postgres_catalog_capacity_exceeded")
    }
    Ok(())
}
pub(crate) fn check(d: &Deadline) -> Result<()> {
    d.check()
        .map_err(|_| anyhow::anyhow!("postgres_catalog_deadline_exceeded"))
}
pub(crate) fn text<'a>(v: &'a Value, k: &str) -> Result<&'a str> {
    v[k].as_str()
        .ok_or_else(|| anyhow::anyhow!("postgres_catalog_input_invalid"))
}
pub(crate) fn rows<'a>(v: &'a Value, k: &str) -> Result<&'a [Value]> {
    v[k].as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| anyhow::anyhow!("postgres_catalog_input_invalid"))
}
fn fields(v: &Value, keys: &[&str]) -> Result<()> {
    input(
        v.as_object()
            .is_some_and(|m| m.len() == keys.len() && keys.iter().all(|k| m.contains_key(*k))),
    )
}
fn name(s: &str) -> Result<()> {
    input(!s.is_empty() && s.len() <= 256 && !s.contains('\0') && !crate::paths::obvious_secret(s))
}
fn ns(s: &str) -> Result<()> {
    name(s)?;
    input(!s.starts_with("pg_") && s != "information_schema")
}
fn oid(v: &Value, k: &str) -> Result<u32> {
    v[k].as_u64()
        .and_then(|n| u32::try_from(n).ok())
        .filter(|n| *n != 0)
        .ok_or_else(|| anyhow::anyhow!("postgres_catalog_input_invalid"))
}
fn definition(v: &Value, l: &Limits) -> Result<()> {
    match text(v, "definitionStatus")? {
        "available" => {
            let s = text(v, "definition")?;
            capacity(s.len() <= l.max_cell_bytes)?;
            input(!s.contains('\0') && !crate::paths::obvious_secret(s))?;
        }
        "not_visible_to_role" | "exceeds_cell_limit" | "not_stored" | "not_applicable" => {
            input(v["definition"].is_null())?
        }
        _ => input(false)?,
    }
    Ok(())
}
impl Limits {
    pub fn validate(&self) -> Result<()> {
        let h = Self::default();
        input(
            self.max_response_bytes > 0
                && self.max_record_bytes > 0
                && self.max_archive_bytes > 0
                && self.max_response_bytes <= h.max_response_bytes
                && self.max_record_bytes <= h.max_record_bytes
                && self.max_relations <= h.max_relations
                && self.max_routines <= h.max_routines
                && self.max_foreign_keys <= h.max_foreign_keys
                && self.max_nodes <= h.max_nodes
                && self.max_observations <= h.max_observations
                && self.max_cell_bytes <= h.max_cell_bytes
                && self.max_archive_bytes <= h.max_archive_bytes,
        )
    }
    pub fn sql(&self) -> Result<String> {
        self.validate()?;
        Ok(include_str!("postgres_catalog_query.sql")
            .replace("\r\n", "\n")
            .replace("__CELL_BYTES__", &self.max_cell_bytes.to_string())
            .replace("__RELATION_ROWS__", &(self.max_relations + 1).to_string())
            .replace("__ROUTINE_ROWS__", &(self.max_routines + 1).to_string())
            .replace(
                "__FOREIGN_KEY_ROWS__",
                &(self.max_foreign_keys + 1).to_string(),
            ))
    }
}
#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum Query {
    Inspect {},
    Node {
        id: String,
    },
    Degree {
        id: String,
    },
    Trace {
        id: String,
        depth: usize,
        #[serde(rename = "maxNodes")]
        max_nodes: usize,
    },
}
impl Catalog {
    pub fn query(&self, query: &Query, max_bytes: usize, d: &Deadline) -> Result<Value> {
        check(d)?;
        capacity(max_bytes > 0 && max_bytes <= 32_000_000)?;
        let nodes = rows(&self.value, "nodes")?;
        let edges = rows(&self.value, "observations")?;
        let mut result = match query {
            Query::Inspect {} => self.value.clone(),
            Query::Node { id } | Query::Degree { id } | Query::Trace { id, .. } => {
                input(crate::model::valid_id(id))?;
                let node = nodes
                    .iter()
                    .find(|n| n["id"] == *id)
                    .ok_or_else(|| anyhow::anyhow!("postgres_catalog_node_absent"))?;
                match query {
                    Query::Node { .. } => {
                        json!({"node":node,"record":self.value["records"][node["recordOrdinal"].as_u64().unwrap() as usize]})
                    }
                    Query::Degree { .. } => {
                        let mut incoming = Vec::new();
                        let mut outgoing = Vec::new();
                        for e in edges {
                            check(d)?;
                            if e["source"] == *id {
                                outgoing.push(e)
                            }
                            if e["target"] == *id {
                                incoming.push(e)
                            }
                        }
                        json!({"node":node,"incoming":incoming,"outgoing":outgoing})
                    }
                    Query::Trace {
                        depth, max_nodes, ..
                    } => {
                        capacity(*depth <= 8 && *max_nodes > 0 && *max_nodes <= 4096)?;
                        let mut seen = BTreeSet::from([id.clone()]);
                        let mut frontier = seen.clone();
                        let mut selected = BTreeSet::new();
                        for _ in 0..*depth {
                            check(d)?;
                            let mut next = BTreeSet::new();
                            for e in edges {
                                check(d)?;
                                let source = text(e, "source")?;
                                if frontier.contains(source) {
                                    selected.insert(text(e, "id")?.to_owned());
                                    let target = text(e, "target")?.to_owned();
                                    if seen.insert(target.clone()) {
                                        capacity(seen.len() <= *max_nodes)?;
                                        next.insert(target);
                                    }
                                }
                            }
                            frontier = next;
                            if frontier.is_empty() {
                                break;
                            }
                        }
                        json!({"direction":"outgoing_declared_foreign_keys","depth":depth,"nodes":nodes.iter().filter(|n|seen.contains(n["id"].as_str().unwrap())).collect::<Vec<_>>(),"observations":edges.iter().filter(|e|selected.contains(e["id"].as_str().unwrap())).collect::<Vec<_>>()})
                    }
                    Query::Inspect {} => unreachable!(),
                }
            }
        };
        if !matches!(query, Query::Inspect {}) {
            result["kind"] = json!("native_postgres_catalog_query_v1");
            result["catalogId"] = self.value["catalogId"].clone();
            result["complete"] = json!(false);
            result["serverAttestation"] = json!("unverified");
            result["networkConnections"] = json!(0);
            result["modelCalls"] = json!(0);
        }
        crate::json::encode_bounded(&result, max_bytes, d)
            .map_err(|_| anyhow::anyhow!("postgres_catalog_capacity_exceeded"))?;
        check(d)?;
        Ok(result)
    }
}
pub struct Catalog {
    value: Value,
    response: Vec<u8>,
    limits: Limits,
    capture_claims: Option<Value>,
}
impl Catalog {
    pub fn from_response(bytes: &[u8], d: &Deadline) -> Result<Self> {
        Self::from_response_with_limits(bytes, &Limits::default(), d)
    }
    pub fn from_response_with_limits(bytes: &[u8], l: &Limits, d: &Deadline) -> Result<Self> {
        l.validate()?;
        check(d)?;
        capacity(bytes.len() <= l.max_response_bytes)?;
        let response_hash = crate::model::hash(bytes);
        let mut records = Vec::new();
        let mut meta = None;
        let mut ended = false;
        let mut offset = 0;
        let mut relation_ids = BTreeSet::new();
        let mut routine_ids = BTreeSet::new();
        let mut key_ids = BTreeSet::new();
        let mut stage = 0;
        for (line_index, line) in bytes.split_inclusive(|b| *b == b'\n').enumerate() {
            check(d)?;
            let start = offset;
            offset += line.len();
            capacity(line.len() <= l.max_record_bytes)?;
            if line.iter().all(u8::is_ascii_whitespace) {
                continue;
            }
            input(!ended)?;
            let v = crate::json::parse(line, l.max_record_bytes)
                .map_err(|_| anyhow::anyhow!("postgres_catalog_input_invalid"))?;
            let section = text(&v, "section")?;
            match section {
                "meta" => {
                    fields(
                        &v,
                        &[
                            "section",
                            "contract",
                            "database",
                            "user",
                            "serverVersionNum",
                            "readOnly",
                            "isolation",
                            "clientEncoding",
                            "tls",
                        ],
                    )?;
                    input(
                        records.is_empty()
                            && meta.is_none()
                            && text(&v, "contract")? == "native_postgres_catalog_rows_v1"
                            && v["readOnly"] == true
                            && text(&v, "isolation")? == "serializable"
                            && text(&v, "clientEncoding")? == "UTF8"
                            && v["tls"].is_boolean(),
                    )?;
                    let version = v["serverVersionNum"]
                        .as_u64()
                        .ok_or_else(|| anyhow::anyhow!("postgres_catalog_input_invalid"))?;
                    if !(140000..190000).contains(&version) {
                        bail!("postgres_catalog_server_version_unsupported");
                    }
                    name(text(&v, "database")?)?;
                    name(text(&v, "user")?)?;
                    meta = Some(v.clone());
                }
                "relation" => {
                    fields(
                        &v,
                        &[
                            "section",
                            "oid",
                            "schema",
                            "name",
                            "kind",
                            "definitionStatus",
                            "definition",
                            "definitionKind",
                        ],
                    )?;
                    input(meta.is_some() && stage <= 1)?;
                    stage = 1;
                    let id = oid(&v, "oid")?;
                    input(relation_ids.insert(id))?;
                    capacity(relation_ids.len() <= l.max_relations)?;
                    ns(text(&v, "schema")?)?;
                    name(text(&v, "name")?)?;
                    let kind = text(&v, "kind")?;
                    input(matches!(kind, "r" | "p" | "f" | "v" | "m"))?;
                    definition(&v, l)?;
                    if matches!(kind, "v" | "m") {
                        input(
                            text(&v, "definitionKind")? == "pg_node_tree"
                                && text(&v, "definitionStatus")? != "not_applicable",
                        )?;
                    } else {
                        input(
                            v["definitionKind"].is_null()
                                && text(&v, "definitionStatus")? == "not_applicable",
                        )?;
                    }
                }
                "routine" => {
                    fields(
                        &v,
                        &[
                            "section",
                            "oid",
                            "schema",
                            "name",
                            "kind",
                            "language",
                            "argumentTypeOids",
                            "definitionStatus",
                            "definition",
                            "definitionKind",
                        ],
                    )?;
                    input(meta.is_some() && stage <= 2)?;
                    stage = 2;
                    let id = oid(&v, "oid")?;
                    input(routine_ids.insert(id))?;
                    capacity(routine_ids.len() <= l.max_routines)?;
                    ns(text(&v, "schema")?)?;
                    name(text(&v, "name")?)?;
                    name(text(&v, "language")?)?;
                    input(
                        matches!(text(&v, "kind")?, "f" | "p" | "w")
                            && matches!(
                                text(&v, "definitionKind")?,
                                "pg_proc.prosrc" | "pg_node_tree"
                            )
                            && text(&v, "definitionStatus")? != "not_applicable",
                    )?;
                    definition(&v, l)?;
                    let args = rows(&v, "argumentTypeOids")?;
                    capacity(args.len() <= 256)?;
                    for arg in args {
                        check(d)?;
                        let s = arg
                            .as_str()
                            .ok_or_else(|| anyhow::anyhow!("postgres_catalog_input_invalid"))?;
                        input(
                            s.len() <= 10
                                && !s.is_empty()
                                && s.bytes().all(|b| b.is_ascii_digit())
                                && s.parse::<u32>().is_ok_and(|n| n != 0),
                        )?;
                    }
                }
                "foreignKey" => {
                    fields(
                        &v,
                        &[
                            "section",
                            "oid",
                            "name",
                            "sourceOid",
                            "targetOid",
                            "sourceSchema",
                            "sourceName",
                            "targetSchema",
                            "targetName",
                            "columns",
                            "targetColumns",
                            "deferrable",
                            "deferred",
                            "validated",
                        ],
                    )?;
                    input(meta.is_some() && stage <= 3)?;
                    stage = 3;
                    input(key_ids.insert(oid(&v, "oid")?))?;
                    capacity(key_ids.len() <= l.max_foreign_keys)?;
                    oid(&v, "sourceOid")?;
                    oid(&v, "targetOid")?;
                    name(text(&v, "name")?)?;
                    ns(text(&v, "sourceSchema")?)?;
                    name(text(&v, "targetSchema")?)?;
                    name(text(&v, "sourceName")?)?;
                    name(text(&v, "targetName")?)?;
                    let a = rows(&v, "columns")?;
                    let b = rows(&v, "targetColumns")?;
                    input(!a.is_empty() && a.len() == b.len())?;
                    capacity(a.len() <= 128)?;
                    for v in a.iter().chain(b) {
                        check(d)?;
                        name(
                            v.as_str()
                                .ok_or_else(|| anyhow::anyhow!("postgres_catalog_input_invalid"))?,
                        )?;
                    }
                    for k in ["deferrable", "deferred", "validated"] {
                        input(v[k].is_boolean())?;
                    }
                    input(v["deferred"] != true || v["deferrable"] == true)?;
                }
                "end" => {
                    fields(&v, &["section", "contract", "readOnly"])?;
                    input(
                        meta.is_some()
                            && text(&v, "contract")? == "native_postgres_catalog_rows_v1"
                            && v["readOnly"] == true,
                    )?;
                    ended = true;
                }
                _ => input(false)?,
            }
            records.push(json!({"ordinal":records.len(),"line":line_index+1,"startByte":start,"endByte":offset,"record":v}));
        }
        input(ended)?;
        let meta = meta.ok_or_else(|| anyhow::anyhow!("postgres_catalog_input_invalid"))?;
        let (nodes, observations) = graph(&records, &response_hash, l, d)?;
        let value = json!({"schemaVersion":1,"kind":"native_postgres_catalog_projection_v1","catalogId":response_hash,
            "source":{"kind":"received_psql_json_lines_utf8","responseHash":response_hash,"responseBytes":bytes.len(),"originalDatabaseSourceBytes":false},
            "meta":meta,"records":records,"nodes":nodes,"observations":observations,
            "columnsGraphed":false,"viewDefinitionFormat":"opaque_pg_node_tree_not_original_sql","routineDefinitionFormat":"server_stored_source_or_pg_node_tree_not_original_ddl",
            "serverAttestation":"unverified","bindingValidation":"received_client_representation_only","canonicalSourceSnapshot":false,
            "networkConnections":0,"networkConnectionCountScope":"this_received_bytes_projection_only","modelCalls":0,"projectCodeExecutionAuthorized":false,"complete":false,
            "limitations":["This is received client-reported catalog data and a replayed structural projection, not database truth, original SQL/physical source, or a canonical code snapshot.",
                "OID identities are local to this captured response. Names and overloads are not semantic identity. Foreign keys are declared directed constraints with ordered column metadata, not observed execution.",
                "Definitions can be hidden or outside the cell budget. No fake SQL, column nodes, default model, stored routine execution or view deparsing is used. Views retain opaque server parse trees where visible.",
                "Coverage is selected non-system metadata visible to the connecting role, with explicit quotas. Querying frozen data never contacts a server or grants network, credential, source or write authority."]});
        crate::json::encode_bounded(&value, l.max_archive_bytes, d)
            .map_err(|_| anyhow::anyhow!("postgres_catalog_capacity_exceeded"))?;
        check(d)?;
        Ok(Self {
            value,
            response: bytes.to_vec(),
            limits: l.clone(),
            capture_claims: None,
        })
    }
    pub fn as_value(&self) -> &Value {
        &self.value
    }
    pub fn response_bytes(&self) -> &[u8] {
        &self.response
    }
}
fn node_id(hash: &str, namespace: &str, id: u32) -> String {
    crate::model::identity(&("postgres-catalog-node-v1", hash, namespace, id))
}
fn graph(
    records: &[Value],
    hash: &str,
    l: &Limits,
    d: &Deadline,
) -> Result<(Vec<Value>, Vec<Value>)> {
    let mut nodes = Vec::new();
    let mut observations = Vec::new();
    let mut relations = BTreeMap::<u32, (String, String, String)>::new();
    for entry in records {
        check(d)?;
        let r = &entry["record"];
        let section = text(r, "section")?;
        if !matches!(section, "relation" | "routine") {
            continue;
        }
        let number = oid(r, "oid")?;
        let id = node_id(hash, section, number);
        let kind = match (section, text(r, "kind")?) {
            ("relation", "r") => "table",
            ("relation", "p") => "partitioned_table",
            ("relation", "f") => "foreign_table",
            ("relation", "v") => "view",
            ("relation", "m") => "materialized_view",
            ("routine", "f") => "function",
            ("routine", "p") => "procedure",
            ("routine", "w") => "window_function",
            _ => {
                input(false)?;
                unreachable!()
            }
        };
        capacity(nodes.len() < l.max_nodes)?;
        nodes.push(json!({"id":id,"oid":number,"kind":kind,"schema":r["schema"],"name":r["name"],"recordOrdinal":entry["ordinal"],"definitionStatus":r["definitionStatus"],"definitionKind":r["definitionKind"],"sourcePrecision":"received_catalog_record","executionObserved":false}));
        if section == "relation" {
            relations.insert(
                number,
                (text(r, "schema")?.into(), text(r, "name")?.into(), id),
            );
        }
    }
    for entry in records {
        check(d)?;
        let r = &entry["record"];
        if r["section"] != "foreignKey" {
            continue;
        }
        let mut endpoints = Vec::new();
        for (ok, sk, nk) in [
            ("sourceOid", "sourceSchema", "sourceName"),
            ("targetOid", "targetSchema", "targetName"),
        ] {
            check(d)?;
            let number = oid(r, ok)?;
            let schema = text(r, sk)?;
            let label = text(r, nk)?;
            if let Some((s, n, id)) = relations.get(&number) {
                input(s == schema && n == label)?;
                endpoints.push(id.clone());
            } else {
                let id = node_id(hash, "relation", number);
                capacity(nodes.len() < l.max_nodes)?;
                nodes.push(json!({"id":id,"oid":number,"kind":"unselected_relation_reference","schema":schema,"name":label,"recordOrdinal":entry["ordinal"],"sourcePrecision":"foreign_key_endpoint_only","definitionStatus":"not_selected","executionObserved":false}));
                relations.insert(number, (schema.into(), label.into(), id.clone()));
                endpoints.push(id);
            }
        }
        let pairs: Vec<_> = rows(r, "columns")?
            .iter()
            .zip(rows(r, "targetColumns")?)
            .enumerate()
            .map(|(i, (a, b))| json!({"position":i,"source":a,"target":b}))
            .collect();
        capacity(observations.len() < l.max_observations)?;
        observations.push(json!({"id":crate::model::identity(&("postgres-catalog-foreign-key-v1",hash,oid(r,"oid")?)),"constraintOid":r["oid"],"name":r["name"],"relation":"declared_foreign_key","source":endpoints[0],"target":endpoints[1],"columnPairs":pairs,"deferrable":r["deferrable"],"deferred":r["deferred"],"validated":r["validated"],"recordOrdinal":entry["ordinal"],"executionObserved":false}));
    }
    nodes.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    observations.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    check(d)?;
    Ok((nodes, observations))
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Archive {
    schema_version: u8,
    kind: String,
    response_hash: String,
    response_base64: String,
    capture_claims: Option<Value>,
}
impl Catalog {
    pub fn encode_archive(&self, d: &Deadline) -> Result<Vec<u8>> {
        check(d)?;
        let value = Archive {
            schema_version: 1,
            kind: "native_postgres_catalog_archive_v1".into(),
            response_hash: crate::model::hash(&self.response),
            response_base64: base64::prelude::BASE64_STANDARD.encode(&self.response),
            capture_claims: self.capture_claims.clone(),
        };
        let result = crate::json::encode_bounded(&value, self.limits.max_archive_bytes, d);
        check(d)?;
        result.map_err(|_| anyhow::anyhow!("postgres_catalog_capacity_exceeded"))
    }
    pub fn load_archive(bytes: &[u8], l: &Limits, d: &Deadline) -> Result<Self> {
        l.validate()?;
        check(d)?;
        capacity(bytes.len() <= l.max_archive_bytes)?;
        let value = crate::json::parse(bytes, l.max_archive_bytes)
            .and_then(crate::json::decode::<Archive>)
            .map_err(|_| anyhow::anyhow!("postgres_catalog_input_invalid"))?;
        input(
            value.schema_version == 1
                && value.kind == "native_postgres_catalog_archive_v1"
                && crate::model::valid_id(&value.response_hash),
        )?;
        capacity(value.response_base64.len() <= l.max_response_bytes.div_ceil(3) * 4)?;
        if let Some(claims) = &value.capture_claims {
            crate::json::encode_bounded(claims, 16384, d)
                .map_err(|_| anyhow::anyhow!("postgres_catalog_capacity_exceeded"))?;
        }
        let raw = base64::prelude::BASE64_STANDARD
            .decode(value.response_base64)
            .map_err(|_| anyhow::anyhow!("postgres_catalog_input_invalid"))?;
        input(crate::model::hash(&raw) == value.response_hash)?;
        check(d)?;
        let mut catalog = Self::from_response_with_limits(&raw, l, d)?;
        catalog.capture_claims = value.capture_claims;
        Ok(catalog)
    }
    pub fn capture_claims(&self) -> Option<&Value> {
        self.capture_claims.as_ref()
    }
    pub(crate) fn set_capture_claims(&mut self, claims: Value) {
        self.capture_claims = Some(claims);
    }
}
