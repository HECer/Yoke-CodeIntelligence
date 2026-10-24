//! All-captured-path effects of authored rule changes, without review authority.
use super::*;
use std::collections::BTreeMap;

pub const INPUT: &str = "git_owner_changes_input_invalid";
const CAPACITY: &str = "git_owner_changes_capacity_exceeded";
const SOURCE: &str = "git_owner_changes_source_refused";
const DEADLINE: &str = "git_owner_changes_deadline_exceeded";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Limits {
    pub max_paths: usize,
    pub max_manifest_entries: usize,
    pub max_rows: usize,
    pub max_file_bytes: usize,
    pub max_source_bytes: usize,
    pub max_rule_bytes: usize,
    pub max_rules: usize,
    pub max_owner_tokens: usize,
    pub max_matches: usize,
    pub max_match_work: usize,
    pub max_output_bytes: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_paths: 20_000,
            max_manifest_entries: 20_000,
            max_rows: 20_000,
            max_file_bytes: 4_000_000,
            max_source_bytes: 64_000_000,
            max_rule_bytes: 2_000_000,
            max_rules: 4096,
            max_owner_tokens: 16384,
            max_matches: 65536,
            max_match_work: 32_000_000,
            max_output_bytes: 8_000_000,
        }
    }
}
impl Limits {
    fn rules(&self) -> super::Limits {
        super::Limits {
            max_rule_bytes: self.max_rule_bytes,
            max_rules: self.max_rules,
            max_owner_tokens: self.max_owner_tokens,
            max_matches: self.max_matches,
            max_match_work: self.max_match_work,
            max_output_bytes: self.max_output_bytes,
        }
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Options {
    pub areas: areas::Options,
    pub limits: Limits,
    #[serde(default)]
    pub include_unchanged: bool,
}
impl Options {
    pub fn parse(raw: &[u8]) -> Result<Self> {
        crate::json::parse(raw, 8192)
            .and_then(crate::json::decode)
            .map_err(|_| anyhow::anyhow!(INPUT))
    }
    pub fn admit(&self, id: &str, d: &Deadline) -> Result<()> {
        d.check().map_err(|_| anyhow::anyhow!(DEADLINE))?;
        anyhow::ensure!(valid_id(id), INPUT);
        self.areas
            .admit(id, &[], d)
            .map_err(|_| anyhow::anyhow!(INPUT))?;
        let l = &self.limits;
        let h = Limits::default();
        for (value, maximum) in [
            (l.max_paths, h.max_paths),
            (l.max_manifest_entries, h.max_manifest_entries),
            (l.max_rows, h.max_rows),
            (l.max_file_bytes, h.max_file_bytes),
            (l.max_source_bytes, h.max_source_bytes),
            (l.max_rule_bytes, h.max_rule_bytes),
            (l.max_rules, h.max_rules),
            (l.max_owner_tokens, h.max_owner_tokens),
            (l.max_matches, h.max_matches),
            (l.max_match_work, h.max_match_work),
            (l.max_output_bytes, h.max_output_bytes),
        ] {
            anyhow::ensure!(value <= maximum, INPUT);
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct ChangeReport {
    value: Value,
    bytes: Vec<u8>,
}
impl ChangeReport {
    pub fn value(&self) -> &Value {
        &self.value
    }
    pub fn canonical_bytes(&self) -> &[u8] {
        &self.bytes
    }
}
impl Serialize for ChangeReport {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        self.value.serialize(s)
    }
}
#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct Consumed {
    manifest_entries: usize,
    paths: usize,
    source_bytes: usize,
    source_files: usize,
    rows: usize,
    unchanged: usize,
    indeterminate: usize,
}

fn interpretation(
    source: &Source,
    side: usize,
    snapshot: &Snapshot,
    path: &str,
    used: &mut Used,
    l: &super::Limits,
    d: &Deadline,
) -> Result<Value> {
    let e = super::evaluate(source, path, used, l, d)?;
    let winning = e.winner.map(|i| {
        let mut r = source.rules[i].record.clone();
        r["sourceOrdinal"] = json!(side);
        r["ruleOrdinal"] = json!(i);
        r["sourcePath"] = source.record["sourcePath"].clone();
        r["contentHash"] = source.record["contentHash"].clone();
        r["snapshotId"] = json!(snapshot.id);
        r
    });
    Ok(json!({"snapshotId":snapshot.id,"status":e.status,
        "ownerLabels":e.winner.map(|i|&source.rules[i].record["ownerLabels"]),
        "winningRule":winning,"matchingSupportedRuleOrdinals":e.matching,"sourceOrdinal":side,
        "uncapturedPrecedenceMayDiffer":!rows(&source.record,"uncapturedHigherPriorityPaths")?.is_empty(),
        "ownerIdentityVerified":false,"githubPermissionVerified":false}))
}
fn unknown(v: &Value) -> bool {
    matches!(
        v["status"].as_str(),
        Some("no_captured_rules" | "unsupported_rules_present")
    )
}
fn rule_key(v: &Value) -> Value {
    if v["winningRule"].is_null() {
        return Value::Null;
    }
    let r = &v["winningRule"];
    // Compare the original winning line/token positions, not the container hash
    // or snapshot ID, which also change when unrelated source/rules change.
    json!({"sourcePath":r["sourcePath"],"pattern":r["pattern"],"ownerLabels":r["ownerLabels"],"ownerTokens":r["ownerTokens"],
        "quote":r["quote"],"line":r["line"],"startByte":r["startByte"],"endByte":r["endByte"]})
}
fn change_kind(base: &Value, head: &Value) -> &'static str {
    if base.is_null() {
        "captured_path_added"
    } else if head.is_null() {
        "captured_path_removed"
    } else if unknown(base) || unknown(head) {
        "indeterminate_selected_rules"
    } else if base["status"] != head["status"] || base["ownerLabels"] != head["ownerLabels"] {
        "authored_candidates_changed"
    } else if rule_key(base) != rule_key(head) {
        "winning_rule_evidence_changed"
    } else {
        "unchanged_selected_assignment"
    }
}

pub fn derive(store: &Store, report: &str, d: &Deadline) -> Result<ChangeReport> {
    derive_with_options(store, report, &Options::default(), d)
}
pub fn derive_with_options(
    store: &Store,
    report: &str,
    o: &Options,
    d: &Deadline,
) -> Result<ChangeReport> {
    let run = || -> Result<ChangeReport> {
        o.admit(report, d)?;
        anyhow::ensure!(store.db.is_autocommit(), INPUT);
        anyhow::ensure!(o.limits.max_output_bytes > 0, CAPACITY);
        areas::derive_with(store, report, &[], &o.areas, d, |parent, snapshots| {
            let l = &o.limits;
            let limits = l.rules();
            let mut used = Consumed::default();
            let mut rule_used = Used::default();
            // All new manifest/body/rule byte admission precedes new CAS reads.
            // The original parent's bounded replay has already happened.
            let mut source_bytes = 0usize;
            let mut physical_rules = 0usize;
            let mut paths = BTreeMap::new();
            for (side, snapshot) in snapshots.iter().enumerate() {
                for file in &snapshot.files {
                    check(d)?;
                    charge(&mut used.manifest_entries, 1, l.max_manifest_entries)?;
                    cap(file.size <= l.max_file_bytes)?;
                    charge(&mut source_bytes, file.size, l.max_source_bytes)?;
                    if !paths.contains_key(file.path.as_str()) {
                        charge(&mut used.paths, 1, l.max_paths)?
                    }
                    let entry = paths.entry(file.path.as_str()).or_insert([None, None]);
                    entry[side] = Some(file);
                }
                if let Some(file) = LOCATIONS
                    .iter()
                    .find_map(|p| snapshot.files.iter().find(|f| f.path == *p))
                {
                    cap(file.size <= 1_000_000)?;
                    charge(&mut physical_rules, file.size, l.max_rule_bytes)?;
                }
            }
            charge(
                &mut rule_used.reserved_output_bytes,
                parent.canonical_bytes().len(),
                l.max_output_bytes,
            )?;
            for snapshot in snapshots {
                let scope = crate::capture::source_scope_policy(store, snapshot)?;
                for file in &snapshot.files {
                    check(d)?;
                    let bytes = scope.read_source(&file.path)?;
                    check(d)?;
                    anyhow::ensure!(
                        bytes.len() == file.size && crate::model::hash(&bytes) == file.hash,
                        SOURCE
                    );
                    charge(&mut used.source_bytes, bytes.len(), l.max_source_bytes)?;
                    charge(&mut used.source_files, 1, l.max_manifest_entries)?;
                }
            }
            let sources = [
                source(store, &snapshots[0], "base", &mut rule_used, &limits, d)?,
                source(store, &snapshots[1], "head", &mut rule_used, &limits, d)?,
            ];
            let rule_sources: Vec<_> = sources.iter().map(|s| &s.record).collect();
            reserve(&rule_sources, &mut rule_used, &limits, d)?;
            let mut changes = Vec::new();
            for (path, files) in paths {
                check(d)?;
                let mut sides = [Value::Null, Value::Null];
                for side in 0..2 {
                    if let Some(file) = files[side] {
                        sides[side] = interpretation(
                            &sources[side],
                            side,
                            &snapshots[side],
                            path,
                            &mut rule_used,
                            &limits,
                            d,
                        )?;
                        sides[side]["contentHash"] = json!(file.hash);
                        sides[side]["sourceBytes"] = json!(file.size);
                    }
                }
                let kind = change_kind(&sides[0], &sides[1]);
                let indeterminate = sides.iter().any(unknown);
                if indeterminate {
                    used.indeterminate += 1
                }
                if kind == "unchanged_selected_assignment" {
                    used.unchanged += 1;
                    if !o.include_unchanged {
                        continue;
                    }
                }
                charge(&mut used.rows, 1, l.max_rows)?;
                let row = json!({"path":path,"change":kind,"base":sides[0],"head":sides[1],
                    "sourceContentChanged":files[0].map(|f|&f.hash)!=files[1].map(|f|&f.hash),
                    "indeterminate":indeterminate,"ownerIdentityVerified":false,"reviewRequirementEstablished":false});
                reserve(&row, &mut rule_used, &limits, d)?;
                changes.push(row);
            }
            let mut value = json!({"schemaVersion":1,"kind":"native_git_captured_owner_changes_v1",
                "policy":"captured_codeowners_path_rules_v1","workspaceId":store.workspace_id,"reportArtifactId":report,
                "parentResultId":parent.value()["resultId"],"impactAreas":parent.value(),
                "ruleSources":rule_sources,"changes":changes,"consumed":used,"ruleInterpretationConsumed":rule_used,
                "options":o,"allCapturedPathsEvaluated":true,"capturedSourceBodiesVerified":true,
                "baseSnapshotId":snapshots[0].id,"headSnapshotId":snapshots[1].id,
                "baseSnapshotOmissionCount":snapshots[0].omissions.len(),"headSnapshotOmissionCount":snapshots[1].omissions.len(),
                "comparisonBasis":"ordered_candidate_labels_and_exact_winning_line_token_positions_not_snapshot_or_container_hash",
                "ownerIdentityVerified":false,"githubPermissionVerified":false,"reviewRequirementEstablished":false,
                "stateWritten":false,"sourceWritten":false,"latestChanged":false,"gitSubprocesses":0,"networkRequests":0,"modelCalls":0,"complete":false,
                "limitations":["Compares declared candidates under selected captured rules. Not authenticated ownership, current repository completeness, user/team existence, permission, branch protection or actual review requirements.",
                "GitHub review requests use base-branch rules. This base/head authored-rule comparison does not request reviews or attest review policy.",
                "Missing or unsupported selected rules remain indeterminate, with supported matching candidates and original rule sources preserved. Uncaptured higher-priority files and omitted paths may change actual repository behavior.",
                "Snapshot/container hashes alone do not mark an unchanged winning line as changed. Earlier matching alternatives remain in original rule sources and optional unchanged rows; the comparison concerns final selected assignments and winning-line evidence.",
                "New manifest/source/rule/match/output limits apply after inherited bounded parent replay. Repeated physical source selections are charged separately; this is not a total heap or hard scheduling quota. No output clipping or source/provider fallback occurs."]});
            let encoded = crate::json::encode_bounded(&value, l.max_output_bytes, d)?;
            check(d)?;
            value = crate::json::parse(&encoded, l.max_output_bytes)?;
            value["resultId"] = json!(crate::model::identity(&value));
            let bytes = crate::json::encode_bounded(&value, l.max_output_bytes, d)?;
            check(d)?;
            Ok(ChangeReport { value, bytes })
        })
    };
    run().map_err(|e| {
        if d.check().is_err() {
            return anyhow::anyhow!(DEADLINE);
        }
        match e.to_string().as_str() {
            INPUT => anyhow::anyhow!(INPUT),
            CAPACITY
            | "git_owners_capacity_exceeded"
            | "git_areas_capacity_exceeded"
            | "JSON output exceeds byte capacity" => anyhow::anyhow!(CAPACITY),
            _ => anyhow::anyhow!(SOURCE),
        }
    })
}
