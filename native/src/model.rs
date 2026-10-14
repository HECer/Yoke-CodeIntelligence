use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub const ANALYZER: &str = "yoke-tree-sitter-4";
pub fn hash(bytes: impl AsRef<[u8]>) -> String {
    hex::encode(Sha256::digest(bytes.as_ref()))
}
pub fn identity(value: &impl Serialize) -> String {
    hash(serde_json::to_vec(value).expect("serializable identity"))
}
pub fn valid_id(id: &str) -> bool {
    id.len() == 64
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SourceFile {
    pub path: String,
    pub hash: String,
    pub size: usize,
    pub language: String,
    pub mode: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Omission {
    pub path: String,
    pub reason: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub id: String,
    pub workspace_id: String,
    pub kind: String,
    pub files: Vec<SourceFile>,
    pub omissions: Vec<Omission>,
    pub capture_consistency: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entity {
    pub id: String,
    pub key: String,
    pub snapshot_id: String,
    pub path: String,
    pub name: String,
    pub qualified_name: String,
    pub kind: String,
    pub language: String,
    pub signature: String,
    pub start_byte: usize,
    pub end_byte: usize,
    pub start_line: usize,
    pub end_line: usize,
    pub name_start_byte: Option<usize>,
    pub name_end_byte: Option<usize>,
    pub parent_key: Option<String>,
    pub producer: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Evidence {
    pub id: String,
    pub snapshot_id: String,
    pub path: String,
    pub content_hash: String,
    pub start_byte: usize,
    pub end_byte: usize,
    pub start_line: usize,
    pub end_line: usize,
    pub anchor: String,
    pub trust: String,
    pub producer: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Observation {
    pub id: String,
    pub snapshot_id: String,
    pub source: String,
    pub target: Option<String>,
    pub target_label: String,
    pub relation: String,
    pub resolution: String,
    pub evidence_ids: Vec<String>,
    pub producer: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    pub path: String,
    pub start_byte: usize,
    pub message: String,
    pub severity: String,
    pub producer: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Graph {
    pub snapshot_id: String,
    pub entities: Vec<Entity>,
    pub observations: Vec<Observation>,
    pub evidence: Vec<Evidence>,
    pub diagnostics: Vec<Diagnostic>,
    pub coverage: Coverage,
}
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Coverage {
    pub indexed_files: usize,
    pub source_files: usize,
    pub languages: BTreeMap<String, usize>,
    pub unresolved: usize,
    pub omissions: usize,
    pub exhaustive: bool,
    pub semantic: bool,
    pub limitations: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawEdge {
    pub source_key: String,
    pub target_label: String,
    pub relation: String,
    pub start_byte: usize,
    pub end_byte: usize,
    pub start_line: usize,
    pub end_line: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Extraction {
    pub entities: Vec<Entity>,
    pub edges: Vec<RawEdge>,
    pub diagnostics: Vec<Diagnostic>,
}
