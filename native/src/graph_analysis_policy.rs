//! Explicit derived-analysis categories. Original observations remain unchanged.
use crate::model::{Entity, Observation};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Extracted,
    Inferred,
    Ambiguous,
    Unmapped,
}

/// Host-selected projection settings, never source, compiler or execution grants.
/// Classifications describe this report policy, not probabilities or attestation.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceCategoryPolicy {
    pub schema_version: u8,
    pub resolution_categories: BTreeMap<String, Category>,
    pub relation_weights: BTreeMap<String, f64>,
    pub excluded_kinds: BTreeSet<String>,
    pub excluded_labels: BTreeSet<String>,
    /// Explicit display heuristic; source declarations are never removed.
    pub exclude_stub_labels: bool,
    /// Optional source-path heuristic, disabled for real extensionless files.
    pub extensionless_as_concepts: bool,
}
impl Default for SourceCategoryPolicy {
    fn default() -> Self {
        Self {
            schema_version: 1,
            resolution_categories: [
                ("syntactic", Category::Extracted),
                ("heuristic", Category::Inferred),
                ("inferred_concept", Category::Inferred),
                ("ambiguous", Category::Ambiguous),
            ]
            .into_iter()
            .map(|(k, v)| (k.into(), v))
            .collect(),
            relation_weights: BTreeMap::new(),
            excluded_kinds: ["file", "concept", "semantic_concept"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
            excluded_labels: BTreeSet::new(),
            exclude_stub_labels: false,
            extensionless_as_concepts: false,
        }
    }
}
fn labels<'a>(values: impl Iterator<Item = &'a String>) -> bool {
    values.into_iter().all(|s| {
        !s.is_empty()
            && s.len() <= 256
            && !s.chars().any(char::is_control)
            && !crate::paths::obvious_secret(s)
    })
}
impl SourceCategoryPolicy {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == 1,
            "Unsupported source-category analysis policy"
        );
        ensure!(
            self.resolution_categories.len() <= 32
                && self.relation_weights.len() <= 32
                && self.excluded_kinds.len() <= 32
                && self.excluded_labels.len() <= 128,
            "Analysis policy entry capacity exceeded"
        );
        ensure!(
            labels(self.resolution_categories.keys())
                && labels(self.relation_weights.keys())
                && labels(self.excluded_kinds.iter())
                && labels(self.excluded_labels.iter()),
            "Invalid analysis policy label"
        );
        ensure!(
            self.relation_weights
                .values()
                .all(|v| v.is_finite() && (0.0..=1000.0).contains(v)),
            "Invalid analysis policy weight"
        );
        ensure!(
            serde_json::to_vec(self)?.len() <= 16_384,
            "Analysis policy byte capacity exceeded"
        );
        Ok(())
    }
    pub fn category(&self, edge: &Observation) -> Category {
        self.resolution_categories
            .get(&edge.resolution)
            .copied()
            .unwrap_or(Category::Unmapped)
    }
    pub fn weight(&self, edge: &Observation, ambiguous_scale: f64) -> f64 {
        self.relation_weights
            .get(&edge.relation)
            .copied()
            .unwrap_or(1.0)
            * if self.category(edge) == Category::Ambiguous {
                ambiguous_scale
            } else {
                1.0
            }
    }
    pub fn eligible(&self, node: &Entity, degree: usize) -> bool {
        if self.excluded_kinds.contains(&node.kind) || self.excluded_labels.contains(&node.name) {
            return false;
        }
        if self.extensionless_as_concepts
            && !node.path.rsplit('/').next().unwrap_or("").contains('.')
        {
            return false;
        }
        if self.exclude_stub_labels
            && ((node.name.starts_with('.') && node.name.ends_with("()"))
                || (degree <= 1 && node.name.ends_with("()")))
        {
            return false;
        }
        true
    }
    pub fn structural_relation(relation: &str) -> bool {
        matches!(relation, "contains" | "imports" | "imports_from" | "method")
    }
    pub fn score(
        &self,
        edge: &Observation,
        from: &Entity,
        to: &Entity,
        separate_communities: bool,
        from_degree: usize,
        to_degree: usize,
    ) -> (u64, Vec<&'static str>) {
        let category = self.category(edge);
        let mut reasons = Vec::new();
        let mut score = match category {
            Category::Ambiguous => {
                reasons.push("policy_ambiguous");
                3
            }
            Category::Inferred => {
                reasons.push("policy_inferred");
                2
            }
            Category::Extracted => 1,
            Category::Unmapped => {
                reasons.push("unmapped_source_category");
                0
            }
        };
        let a = source_kind(&from.path);
        let b = source_kind(&to.path);
        let cross_runtime = runtime_family(&from.path)
            .zip(runtime_family(&to.path))
            .is_some_and(|(a, b)| a != b);
        let code_document = matches!(
            (a, b),
            (SourceKind::Code, SourceKind::Document) | (SourceKind::Document, SourceKind::Code)
        );
        let suppressed = category == Category::Inferred
            && matches!(edge.relation.as_str(), "calls" | "uses" | "references")
            && (cross_runtime || code_document);
        if suppressed {
            score = 0;
            reasons.push("inferred_structural_cross_runtime_or_document_suppressed");
        }
        if !suppressed {
            if a != b {
                score += 2;
                reasons.push("different_source_kinds");
            }
            if top_directory(&from.path) != top_directory(&to.path) {
                score += 2;
                reasons.push("different_top_level_directories");
            }
            if separate_communities {
                score += 1;
                reasons.push("separate_communities");
            }
        }
        if edge.relation == "semantically_similar_to" {
            score = score * 3 / 2;
            reasons.push("semantic_similarity_claim");
        }
        if from_degree.min(to_degree) <= 2 && from_degree.max(to_degree) >= 5 {
            score += 1;
            reasons.push("peripheral_to_hub");
        }
        (score, reasons)
    }
}
fn top_directory(path: &str) -> &str {
    path.split_once('/').map_or(".", |(dir, _)| dir)
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum SourceKind {
    Code,
    Document,
    Paper,
    Image,
}
fn extension(path: &str) -> String {
    path.rsplit('/')
        .next()
        .unwrap_or("")
        .rsplit_once('.')
        .map_or("", |(_, ext)| ext)
        .to_ascii_lowercase()
}
fn source_kind(path: &str) -> SourceKind {
    if runtime_family(path).is_some() {
        return SourceKind::Code;
    }
    match extension(path).as_str() {
        "pdf" => SourceKind::Paper,
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp" | "tiff" => SourceKind::Image,
        _ => SourceKind::Document,
    }
}
fn runtime_family(path: &str) -> Option<&'static str> {
    Some(match extension(path).as_str() {
        "js" | "jsx" | "mjs" | "cjs" | "ejs" | "ts" | "tsx" | "mts" | "cts" | "vue" | "svelte" => {
            "javascript"
        }
        "py" | "pyw" => "python",
        "rs" => "rust",
        "go" => "go",
        "java" | "kt" | "kts" | "scala" => "jvm",
        "c" | "h" | "cpp" | "cc" | "cxx" | "hpp" => "c",
        "cs" | "vb" => "dotnet",
        "rb" | "rake" => "ruby",
        "swift" => "swift",
        "php" => "php",
        "r" => "r",
        "cbl" | "cob" | "cobol" | "cpy" => "cobol",
        "sol" => "solidity",
        "erl" | "hrl" | "escript" => "erlang",
        "ps1" | "psm1" => "powershell",
        _ => return None,
    })
}
