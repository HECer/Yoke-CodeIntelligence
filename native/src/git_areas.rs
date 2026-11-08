//! Saved concept/file memberships and directory fallbacks over a replayed Git report.
//! Labels are inferences or path spellings, never semantic identity or ownership.
use super::impact_details::{self, DetailOptions};
use crate::{
    capture::{Deadline, DeadlineFailure},
    meaning_build,
    model::{Snapshot, identity, valid_id},
    store::Store,
};
use anyhow::{Context, Result, ensure};
use rusqlite::{Transaction, TransactionBehavior, limits::Limit};
use serde::{Deserialize, Serialize, Serializer};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Limits {
    pub max_paths: usize,
    pub max_occurrences: usize,
    pub max_snapshot_files: usize,
    pub max_concepts: usize,
    pub max_evidence_rows: usize,
    pub max_support_references: usize,
    pub max_memberships: usize,
    pub max_assignments: usize,
    pub max_meaning_bytes: usize,
    pub max_metadata_bytes: usize,
    pub max_output_bytes: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_paths: 20_000,
            max_occurrences: 200_000,
            max_snapshot_files: 20_000,
            max_concepts: 2048,
            max_evidence_rows: 16_384,
            max_support_references: 32_768,
            max_memberships: 32_768,
            max_assignments: 65_536,
            max_meaning_bytes: 8_000_000,
            max_metadata_bytes: 16_000_000,
            max_output_bytes: 4_000_000,
        }
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Options {
    pub details: DetailOptions,
    pub limits: Limits,
}
impl Options {
    pub fn parse(raw: &[u8]) -> Result<Self> {
        crate::json::parse(raw, 8192)
            .and_then(crate::json::decode)
            .map_err(|_| anyhow::anyhow!(Failure::Input))
    }
    pub fn admit(&self, report: &str, meanings: &[String], d: &Deadline) -> Result<()> {
        d.check()?;
        ensure!(
            valid_id(report)
                && meanings.len() <= 4
                && meanings.iter().all(|id| valid_id(id))
                && meanings.iter().collect::<BTreeSet<_>>().len() == meanings.len(),
            Failure::Input
        );
        let hard = Limits::default();
        let l = &self.limits;
        for (n, maximum) in [
            (l.max_paths, hard.max_paths),
            (l.max_occurrences, hard.max_occurrences),
            (l.max_snapshot_files, hard.max_snapshot_files),
            (l.max_concepts, hard.max_concepts),
            (l.max_evidence_rows, hard.max_evidence_rows),
            (l.max_support_references, hard.max_support_references),
            (l.max_memberships, hard.max_memberships),
            (l.max_assignments, hard.max_assignments),
            (l.max_meaning_bytes, hard.max_meaning_bytes),
            (l.max_metadata_bytes, hard.max_metadata_bytes),
            (l.max_output_bytes, hard.max_output_bytes),
        ] {
            ensure!(n <= maximum, Failure::Capacity);
        }
        // The legacy producer's schema/IDs stay unchanged. This new consumer has
        // its own fixed ceilings even when the direct legacy API permits more.
        let hard = DetailOptions::default();
        let a = &self.details;
        for (n, maximum) in [
            (a.max_candidate_pairs, hard.max_candidate_pairs),
            (a.max_source_bytes, hard.max_source_bytes),
            (a.max_similarity_lines, hard.max_similarity_lines),
            (a.max_similarity_work, hard.max_similarity_work),
            (a.max_rows, hard.max_rows),
            (a.max_output_bytes, hard.max_output_bytes),
            (a.max_quote_bytes, hard.max_quote_bytes),
            (a.max_total_quote_bytes, hard.max_total_quote_bytes),
        ] {
            ensure!(n <= maximum, Failure::Capacity);
        }
        Ok(())
    }
}
#[derive(Debug)]
enum Failure {
    Input,
    Capacity,
    Source,
}
impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Input => "git_areas_input_invalid",
            Self::Capacity => "git_areas_capacity_exceeded",
            Self::Source => "git_areas_source_unavailable",
        })
    }
}
impl std::error::Error for Failure {}
fn public(error: anyhow::Error, d: &Deadline) -> anyhow::Error {
    if let Some(f) = d.failure() {
        return anyhow::anyhow!(match f {
            DeadlineFailure::Cancelled => "git_areas_cancelled",
            DeadlineFailure::Expired => "git_areas_deadline_exceeded",
        });
    }
    if let Some(f) = error.downcast_ref::<Failure>() {
        return anyhow::anyhow!(f.to_string());
    }
    if impact_details::is_checked_input_capacity(&error)
        || error.to_string() == "git_impact_details_capacity_exceeded"
    {
        anyhow::anyhow!(Failure::Capacity)
    } else {
        anyhow::anyhow!(Failure::Source)
    }
}
#[derive(Clone, Debug)]
pub struct AreaReport {
    value: Value,
    bytes: Vec<u8>,
}
impl AreaReport {
    pub fn value(&self) -> &Value {
        &self.value
    }
    pub fn canonical_bytes(&self) -> &[u8] {
        &self.bytes
    }
}
impl Serialize for AreaReport {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        self.value.serialize(s)
    }
}
#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct Consumed {
    paths: usize,
    occurrences: usize,
    snapshot_files: usize,
    meaning_graphs: usize,
    concepts: usize,
    evidence_rows: usize,
    support_references: usize,
    memberships: usize,
    assignments: usize,
    preflight_metadata_bytes: usize,
}
struct Budget<'a> {
    limits: &'a Limits,
    d: &'a Deadline,
    used: Consumed,
    reserved_output: usize,
}
impl Budget<'_> {
    fn reserve(&mut self, value: &Value) -> Result<()> {
        let remaining = self.limits.max_output_bytes - self.reserved_output;
        let bytes = crate::json::encode_bounded(value, remaining, self.d)
            .map_err(|_| anyhow::anyhow!(Failure::Capacity))?;
        self.reserved_output += bytes.len();
        Ok(())
    }
    fn clone_value(&mut self, value: &Value) -> Result<Value> {
        self.reserve(value)?;
        Ok(value.clone())
    }
}
fn charge(n: &mut usize, add: usize, maximum: usize) -> Result<()> {
    *n = n.checked_add(add).context(Failure::Capacity)?;
    ensure!(*n <= maximum, Failure::Capacity);
    Ok(())
}
fn text(value: &Value) -> Result<&str> {
    value.as_str().context(Failure::Source)
}
fn array(value: &Value) -> Result<&Vec<Value>> {
    value.as_array().context(Failure::Source)
}
fn metadata(store: &Store, kind: Option<&str>, id: &str, b: &mut Budget<'_>) -> Result<Value> {
    b.d.check()?;
    ensure!(valid_id(id), Failure::Source);
    let remaining = b.limits.max_metadata_bytes - b.used.preflight_metadata_bytes;
    let raw: Vec<u8> = if let Some(kind) = kind {
        store.db.query_row(
            "SELECT substr(CAST(value AS BLOB),1,?3) FROM records WHERE kind=?1 AND id=?2",
            rusqlite::params![kind, id, remaining + 1],
            |r| r.get(0),
        )?
    } else {
        store.db.query_row(
            "SELECT substr(CAST(value AS BLOB),1,?2) FROM snapshots WHERE id=?1",
            rusqlite::params![id, remaining + 1],
            |r| r.get(0),
        )?
    };
    charge(
        &mut b.used.preflight_metadata_bytes,
        raw.len(),
        b.limits.max_metadata_bytes,
    )?;
    crate::json::parse(&raw, raw.len()).map_err(|_| anyhow::anyhow!(Failure::Source))
}
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Side {
    Base,
    Head,
}
impl Side {
    fn parse(value: &Value) -> Result<Self> {
        match value.as_str() {
            Some("base") => Ok(Self::Base),
            Some("head") => Ok(Self::Head),
            _ => Err(anyhow::anyhow!(Failure::Source)),
        }
    }
    fn label(self) -> &'static str {
        match self {
            Self::Base => "base",
            Self::Head => "head",
        }
    }
}
struct Sources {
    snapshots: [Snapshot; 2],
    files: BTreeMap<(Side, String), String>,
    graph_hashes: [String; 2],
}
impl Sources {
    fn snapshot(&self, side: Side) -> &Snapshot {
        &self.snapshots[usize::from(side == Side::Head)]
    }
    fn matches(&self, snapshot: &Value, graph_hash: Option<&Value>) -> Vec<Side> {
        [Side::Base, Side::Head]
            .into_iter()
            .filter(|s| {
                snapshot.as_str() == Some(self.snapshot(*s).id.as_str())
                    && graph_hash.is_none_or(|g| {
                        g.as_str()
                            == Some(self.graph_hashes[usize::from(*s == Side::Head)].as_str())
                    })
            })
            .collect()
    }
}
fn sources(store: &Store, parent: &Value, b: &mut Budget<'_>) -> Result<Sources> {
    let mut snapshots = vec![];
    let mut files = BTreeMap::new();
    for (side, key) in [
        (Side::Base, "baseSnapshotId"),
        (Side::Head, "headSnapshotId"),
    ] {
        let id = text(&parent[key])?;
        let s: Snapshot = crate::json::decode(metadata(store, None, id, b)?)?;
        ensure!(
            s.id == id && s.workspace_id == store.workspace_id,
            Failure::Source
        );
        charge(
            &mut b.used.snapshot_files,
            s.files.len(),
            b.limits.max_snapshot_files,
        )?;
        for file in &s.files {
            b.d.check()?;
            ensure!(
                files
                    .insert((side, file.path.clone()), file.hash.clone())
                    .is_none(),
                Failure::Source
            );
        }
        snapshots.push(s);
    }
    Ok(Sources {
        snapshots: snapshots
            .try_into()
            .map_err(|_| anyhow::anyhow!(Failure::Source))?,
        files,
        graph_hashes: [
            text(&parent["baseGraphHash"])?.into(),
            text(&parent["headGraphHash"])?.into(),
        ],
    })
}
struct PathClass {
    side: Side,
    path: String,
    occurrences: Vec<usize>,
}
struct Paths {
    classes: Vec<PathClass>,
    lookup: BTreeMap<(Side, String), usize>,
    occurrences: Vec<Value>,
}
impl Paths {
    fn add(
        &mut self,
        side: Side,
        path: &str,
        pointers: (String, Option<String>),
        category: &str,
        sources: &Sources,
        b: &mut Budget<'_>,
    ) -> Result<()> {
        b.d.check()?;
        ensure!(
            sources.files.contains_key(&(side, path.into())),
            Failure::Source
        );
        charge(&mut b.used.occurrences, 1, b.limits.max_occurrences)?;
        let classification = if let Some(i) = self.lookup.get(&(side, path.into())) {
            *i
        } else {
            charge(&mut b.used.paths, 1, b.limits.max_paths)?;
            let i = self.classes.len();
            self.classes.push(PathClass {
                side,
                path: path.into(),
                occurrences: vec![],
            });
            self.lookup.insert((side, path.into()), i);
            i
        };
        let ordinal = self.occurrences.len();
        let row = json!({"ordinal":ordinal,"classificationOrdinal":classification,
            "category":category,"parentPointer":pointers.0,"quotePointer":pointers.1});
        b.reserve(&row)?;
        self.classes[classification].occurrences.push(ordinal);
        self.occurrences.push(row);
        Ok(())
    }
}
fn paths(parent: &Value, sources: &Sources, b: &mut Budget<'_>) -> Result<Paths> {
    let mut out = Paths {
        classes: vec![],
        lookup: BTreeMap::new(),
        occurrences: vec![],
    };
    for (i, row) in array(&parent["changes"])?.iter().enumerate() {
        let original = &row["original"];
        for (side, key) in [(Side::Base, "basePath"), (Side::Head, "headPath")] {
            if !original[key].is_null() {
                out.add(
                    side,
                    text(&original[key])?,
                    (format!("/changes/{i}/original/{key}"), None),
                    "changed_path",
                    sources,
                    b,
                )?;
            }
        }
        for (j, hunk) in array(&original["hunks"])?.iter().enumerate() {
            for (side, key) in [(Side::Base, "base"), (Side::Head, "head")] {
                if !hunk[key].is_null() {
                    out.add(
                        side,
                        text(&hunk[key]["path"])?,
                        (
                            format!("/changes/{i}/original/hunks/{j}/{key}"),
                            Some(format!("/changes/{i}/hunks/{j}/{key}")),
                        ),
                        "hunk",
                        sources,
                        b,
                    )?;
                }
            }
        }
    }
    for key in ["changedSymbols", "impact"] {
        for (i, row) in array(&parent[key])?.iter().enumerate() {
            let original = &row["original"];
            out.add(
                Side::parse(&original["side"])?,
                text(&original["path"])?,
                (
                    format!("/{key}/{i}/original"),
                    Some(format!("/{key}/{i}/source")),
                ),
                key,
                sources,
                b,
            )?;
        }
    }
    for (i, row) in array(&parent["impactObservations"])?.iter().enumerate() {
        let original = &row["original"];
        let side = Side::parse(&original["side"])?;
        for (j, e) in array(&original["evidence"])?.iter().enumerate() {
            out.add(
                side,
                text(&e["path"])?,
                (
                    format!("/impactObservations/{i}/original/evidence/{j}"),
                    Some(format!("/impactObservations/{i}/evidence/{j}/source")),
                ),
                "observation_evidence",
                sources,
                b,
            )?;
        }
    }
    Ok(out)
}
struct MeaningPlan {
    id: String,
    sides: Vec<Side>,
}
fn preflight_meanings(
    store: &Store,
    ids: &[String],
    sources: &Sources,
    b: &mut Budget<'_>,
) -> Result<Vec<MeaningPlan>> {
    let mut plans = vec![];
    for id in ids {
        let record = metadata(store, Some(meaning_build::GRAPH_KIND), id, b)?;
        let sides = sources.matches(&record["snapshotId"], None);
        ensure!(
            record["graphId"] == *id
                && record["workspaceId"] == store.workspace_id
                && !sides.is_empty(),
            Failure::Source
        );
        let size = record["totalBytes"].as_u64().context(Failure::Source)?;
        ensure!(size <= b.limits.max_meaning_bytes as u64, Failure::Capacity);
        let artifact = text(&record["artifactId"])?;
        ensure!(valid_id(artifact), Failure::Source);
        ensure!(
            store.artifact_size(artifact)? as u64 == size,
            Failure::Source
        );
        let job = metadata(
            store,
            Some(meaning_build::JOB_KIND),
            text(&record["jobId"])?,
            b,
        )?;
        ensure!(
            job["graphId"] == *id
                && job["state"] == "complete"
                && job["workspaceId"] == store.workspace_id
                && job["snapshotId"] == record["snapshotId"]
                && !sources
                    .matches(&job["snapshotId"], Some(&job["graphHash"]))
                    .is_empty(),
            Failure::Source
        );
        plans.push(MeaningPlan {
            id: id.clone(),
            sides,
        });
    }
    Ok(plans)
}
struct Concepts {
    rows: Vec<Value>,
    memberships: BTreeMap<(Side, String), Vec<usize>>,
    selections: Vec<Value>,
}
fn concepts(
    store: &Store,
    plans: &[MeaningPlan],
    sources: &Sources,
    b: &mut Budget<'_>,
) -> Result<Concepts> {
    let mut out = Concepts {
        rows: vec![],
        memberships: BTreeMap::new(),
        selections: vec![],
    };
    for (graph_index, plan) in plans.iter().enumerate() {
        b.d.check()?;
        let graph = meaning_build::load(store, &plan.id, b.d)?;
        let sides = sources.matches(&graph["snapshotId"], Some(&graph["graphHash"]));
        ensure!(
            sides == plan.sides && graph["workspaceId"] == store.workspace_id,
            Failure::Source
        );
        b.used.meaning_graphs += 1;
        let selection = json!({"ordinal":graph_index,"graphId":plan.id,
            "snapshotId":graph["snapshotId"],"graphHash":graph["graphHash"],
            "bindingValidation":"exact_captured_source_replayed","providerAttestation":"unverified"});
        b.reserve(&selection)?;
        out.selections.push(selection);
        for (packet_index, packet) in array(&graph["packets"])?.iter().enumerate() {
            let meaning = &packet["meaning"];
            let evidence = array(&meaning["evidence"])?;
            charge(
                &mut b.used.evidence_rows,
                evidence.len(),
                b.limits.max_evidence_rows,
            )?;
            let mut by_id = BTreeMap::new();
            for row in evidence {
                b.d.check()?;
                ensure!(
                    by_id.insert(text(&row["id"])?, row).is_none(),
                    Failure::Source
                );
                for side in &sides {
                    ensure!(
                        sources
                            .files
                            .get(&(*side, text(&row["path"])?.into()))
                            .is_some_and(|hash| row["contentHash"] == *hash),
                        Failure::Source
                    );
                }
            }
            for (concept_index, concept) in array(&meaning["concepts"])?.iter().enumerate() {
                let support = array(&concept["evidenceIds"])?;
                ensure!(!support.is_empty(), Failure::Source);
                for side in &sides {
                    b.d.check()?;
                    charge(&mut b.used.concepts, 1, b.limits.max_concepts)?;
                    charge(
                        &mut b.used.support_references,
                        support.len(),
                        b.limits.max_support_references,
                    )?;
                    let mut files = BTreeSet::new();
                    let original = b.clone_value(concept)?;
                    let mut proofs = vec![];
                    for id in support {
                        let e = *by_id.get(text(id)?).context(Failure::Source)?;
                        files.insert(text(&e["path"])?.to_owned());
                        proofs.push(b.clone_value(e)?);
                    }
                    charge(
                        &mut b.used.memberships,
                        files.len(),
                        b.limits.max_memberships,
                    )?;
                    let ordinal = out.rows.len();
                    for file in &files {
                        out.memberships
                            .entry((*side, file.clone()))
                            .or_default()
                            .push(ordinal);
                    }
                    out.rows.push(json!({"ordinal":ordinal,"side":side.label(),
                        "snapshotId":sources.snapshot(*side).id,
                        "reference":{"meaningGraphId":plan.id,"graphIndex":graph_index,
                            "packetIndex":packet_index,"conceptIndex":concept_index,"conceptId":concept["id"]},
                        "distinctSupportedFiles":files.len(),"original":original,"supportEvidence":proofs,
                        "semanticIdentityEstablished":false}));
                }
            }
        }
    }
    Ok(out)
}
fn project(
    paths: &Paths,
    concepts: &Concepts,
    sources: &Sources,
    b: &mut Budget<'_>,
) -> Result<(Vec<Value>, Vec<Value>)> {
    let mut classes = vec![];
    let mut groups = vec![];
    let mut concept_groups = BTreeMap::new();
    let mut directory_groups = BTreeMap::new();
    for (ordinal, path) in paths.classes.iter().enumerate() {
        b.d.check()?;
        let mut candidates = concepts
            .memberships
            .get(&(path.side, path.path.clone()))
            .cloned()
            .unwrap_or_default();
        charge(
            &mut b.used.assignments,
            candidates.len().max(1),
            b.limits.max_assignments,
        )?;
        let count = |i: usize| {
            concepts.rows[i]["distinctSupportedFiles"]
                .as_u64()
                .unwrap_or(u64::MAX)
        };
        candidates.sort_by_key(|i| (count(*i), *i));
        let preferred: Vec<usize> = candidates
            .iter()
            .copied()
            .take_while(|i| {
                candidates
                    .first()
                    .is_some_and(|first| count(*i) == count(*first))
            })
            .collect();
        let directory = path
            .path
            .rsplit_once('/')
            .map_or("(root)".into(), |(dir, _)| format!("{dir}/"));
        let label = if preferred.len() == 1 {
            concepts.rows[preferred[0]]["original"]["label"].clone()
        } else if preferred.is_empty() {
            json!(directory)
        } else {
            Value::Null
        };
        classes.push(json!({"ordinal":ordinal,"side":path.side.label(),
            "snapshotId":sources.snapshot(path.side).id,"path":path.path,
            "contentHash":sources.files[&(path.side,path.path.clone())],
            "occurrenceOrdinals":path.occurrences,"directoryLabel":directory,
            "conceptCandidateOrdinals":candidates,"preferredConceptOrdinals":preferred,
            "preferredAmbiguous":preferred.len()>1,"label":label,
            "basis":if preferred.is_empty() {"directory_spelling_only"} else {"saved_inferred_concept_membership"},
            "semanticIdentityEstablished":false}));
        b.reserve(classes.last().unwrap())?;
        for candidate in &candidates {
            let i = *concept_groups.entry(*candidate).or_insert_with(|| {
                let i = groups.len();
                groups.push(json!({"kind":"concept","conceptOrdinal":candidate,
                    "side":path.side.label(),"label":concepts.rows[*candidate]["original"]["label"],
                    "classificationOrdinals":[],"preferredClassificationOrdinals":[]}));
                i
            });
            groups[i]["classificationOrdinals"]
                .as_array_mut()
                .unwrap()
                .push(json!(ordinal));
            if preferred.contains(candidate) {
                groups[i]["preferredClassificationOrdinals"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!(ordinal));
            }
        }
        if preferred.is_empty() {
            let i = *directory_groups
                .entry((path.side, directory.clone()))
                .or_insert_with(|| {
                    let i = groups.len();
                    groups.push(
                        json!({"kind":"directory","side":path.side.label(),"label":directory,
                    "classificationOrdinals":[],"preferredClassificationOrdinals":[]}),
                    );
                    i
                });
            for key in ["classificationOrdinals", "preferredClassificationOrdinals"] {
                groups[i][key].as_array_mut().unwrap().push(json!(ordinal));
            }
        }
    }
    for row in &groups {
        b.reserve(row)?;
    }
    Ok((classes, groups))
}
pub fn derive(
    store: &Store,
    report_id: &str,
    meaning_graph_ids: &[String],
    options: &Options,
    d: &Deadline,
) -> Result<AreaReport> {
    derive_with(
        store,
        report_id,
        meaning_graph_ids,
        options,
        d,
        |report, _| Ok(report),
    )
}
/// Keep additional native source interpretations within this checked read view.
pub(crate) fn derive_with<T>(
    store: &Store,
    report_id: &str,
    meaning_graph_ids: &[String],
    options: &Options,
    d: &Deadline,
    finish: impl FnOnce(AreaReport, &[Snapshot; 2]) -> Result<T>,
) -> Result<T> {
    let run = || -> Result<Result<T>> {
        options.admit(report_id, meaning_graph_ids, d)?;
        ensure!(store.db.is_autocommit(), Failure::Input);
        crate::github_app::worker::publication_policy(store, d, |d| {
            let prior = store.db.limit(Limit::SQLITE_LIMIT_LENGTH)?;
            store
                .db
                .set_limit(Limit::SQLITE_LIMIT_LENGTH, prior.min(16_004_096))?;
            let tx = Transaction::new_unchecked(&store.db, TransactionBehavior::Deferred)?;
            let parent = impact_details::derive(store, report_id, &options.details, d)?;
            let mut b = Budget {
                limits: &options.limits,
                d,
                used: Consumed::default(),
                reserved_output: 0,
            };
            let original = b.clone_value(parent.value())?;
            let sources = sources(store, parent.value(), &mut b)?;
            let plans = preflight_meanings(store, meaning_graph_ids, &sources, &mut b)?;
            let paths = paths(parent.value(), &sources, &mut b)?;
            let concepts = concepts(store, &plans, &sources, &mut b)?;
            let (classes, groups) = project(&paths, &concepts, &sources, &mut b)?;
            let mut value = json!({"schemaVersion":1,"kind":"native_git_impact_areas",
            "policy":"saved_concept_smallest_distinct_file_set_with_visible_ties_and_directory_fallback_v1",
            "workspaceId":store.workspace_id,"reportArtifactId":report_id,
            "parentResultId":parent.value()["resultId"],"impactDetails":original,
            "meaningSelections":concepts.selections,"concepts":concepts.rows,
            "occurrences":paths.occurrences,"pathClassifications":classes,"groups":groups,
            "consumed":b.used,"options":options,"stateWritten":false,"sourceWritten":false,
            "latestChanged":false,"gitSubprocesses":0,"networkRequests":0,"modelCalls":0,
            "classificationCompleteForSelectedPaths":true,"semanticIdentityEstablished":false,"complete":false,
            "limitations":[
                "Existing captured Git report and selected saved Meaning graphs are replayed; no live files, Git refs, provider or source capture supplies a fallback.",
                "Every original impact row, parallel observation and exact quote remains in impactDetails. Occurrence pointers refer to that unchanged parent. Areas do not rewrite structural or Meaning relationships.",
                "Saved concept labels are unverified inferences. Smaller distinct supported-file sets rank first; all equal minima and all other claims remain visible. Equal labels never merge concept identities.",
                "Directory fallback is the exact captured parent-directory spelling, not a semantic module or ownership fact. Scope is the selected impact paths, not an exhaustive repository module catalog.",
                "New metadata counters cover selected snapshot/Meaning registration/job preflight, not inherited replay IO, total memory or a hard scheduler quota. Existing replay bounds, fixed SQLite cell ceiling and original host deadline/cancellation remain."
            ]});
            value["resultId"] = json!(identity(&value));
            let bytes = crate::json::encode_bounded(&value, options.limits.max_output_bytes, d)
                .map_err(|_| anyhow::anyhow!(Failure::Capacity))?;
            d.check()?;
            let result = finish(AreaReport { value, bytes }, &sources.snapshots);
            d.check()?;
            tx.commit()?;
            Ok(result)
        })
    };
    run().map_err(|e| public(e, d))?
}
