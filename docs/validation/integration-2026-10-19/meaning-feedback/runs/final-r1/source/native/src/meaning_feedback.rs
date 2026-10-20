//! Authored outcomes for packet-qualified local Meaning inferences. Source and
//! receipt replay establish membership/citations, never interpretation truth.
pub use super::plans::{Evidence, Side};
use super::*;
use crate::meaning_view::{
    MeaningConcept, MeaningEvidence, MeaningHyperedge, MeaningPacket, MeaningRelation,
    MeaningSelectionRef,
};
use anyhow::Context as _;

#[path = "meaning_feedback_store.rs"]
mod events;
#[path = "meaning_feedback_rank.rs"]
mod ranking;

pub const EVENT_KIND: &str = "native_meaning_feedback_event_v1";
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Concept,
    Relation,
    Hyperedge,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Reference {
    pub kind: Kind,
    pub graph_id: String,
    pub packet_index: usize,
    /// Original normalized claim ID, qualified by graph and packet; never a label.
    pub claim_id: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Target {
    pub origin: ViewPin,
    pub evaluated: ViewPin,
    pub reference: Reference,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Draft {
    pub schema_version: u32,
    pub target: Target,
    pub actor: String,
    pub session: String,
    pub sequence: u32,
    pub occurred_at_ms: u64,
    pub outcome: Outcome,
    pub rationale: String,
    pub source_evidence: Vec<Evidence>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Event {
    schema_version: u32,
    id: String,
    workspace_id: String,
    draft: Draft,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RankInput {
    pub schema_version: u32,
    pub targets: Vec<Target>,
    pub as_of_ms: u64,
    pub decay_window_days: u32,
    pub limit: usize,
}
fn validate_target(t: &Target) -> Result<()> {
    t.origin.validate()?;
    t.evaluated.validate()?;
    ensure!(
        t.origin.workspace_id == t.evaluated.workspace_id
            && valid_id(&t.reference.graph_id)
            && valid_id(&t.reference.claim_id)
            && t.reference.packet_index < 64,
        "feedback_input_invalid"
    );
    Ok(())
}
pub fn validate_draft(draft: &Draft, d: &Deadline) -> Result<()> {
    d.check()?;
    crate::json::encode_bounded(draft, MAX_DRAFT_BYTES, d)?;
    validate_target(&draft.target)?;
    ensure!(
        draft.schema_version == 1
            && (1..=1_000_000).contains(&draft.sequence)
            && (1..=32).contains(&draft.source_evidence.len()),
        "feedback_input_invalid"
    );
    label(&draft.actor)?;
    label(&draft.session)?;
    bounded_text(&draft.rationale, 2048, false)?;
    let mut seen = BTreeSet::new();
    let mut bytes = 0usize;
    for evidence in &draft.source_evidence {
        d.check()?;
        let q = &evidence.source;
        ensure!(
            crate::paths::normalized(&q.path)? == q.path
                && valid_id(&q.content_hash)
                && q.end_byte >= q.start_byte
                && q.end_byte - q.start_byte == q.quote.len()
                && seen.insert(evidence),
            "feedback_input_invalid"
        );
        bounded_text(&q.quote, 16_000, false)?;
        bytes += q.quote.len();
    }
    ensure!(bytes <= 32_000, "feedback_capacity_exceeded");
    d.check()
}
pub fn validate_rank(input: &RankInput, d: &Deadline) -> Result<()> {
    d.check()?;
    crate::json::encode_bounded(input, MAX_DRAFT_BYTES, d)?;
    ensure!(
        input.schema_version == 1
            && (1..=128).contains(&input.targets.len())
            && (1..=128).contains(&input.limit)
            && (1..=3650).contains(&input.decay_window_days),
        "feedback_input_invalid"
    );
    let mut subjects = BTreeSet::new();
    for t in &input.targets {
        d.check()?;
        validate_target(t)?;
        ensure!(subjects.insert(subject_id(t)), "feedback_input_invalid");
    }
    d.check()
}
fn canonical(mut draft: Draft) -> Draft {
    draft.source_evidence.sort();
    draft
}
fn subject_id(t: &Target) -> String {
    identity(&(
        "native_meaning_feedback_subject_v1",
        &t.origin,
        &t.reference,
    ))
}
fn event_id(workspace: &str, draft: &Draft) -> String {
    identity(&(EVENT_KIND, workspace, draft))
}

#[derive(Serialize)]
#[serde(untagged)]
enum Claim<'a> {
    Concept(&'a MeaningConcept),
    Relation(&'a MeaningRelation),
    Hyperedge(&'a MeaningHyperedge),
}
impl Claim<'_> {
    fn evidence_ids(&self) -> &[String] {
        match self {
            Self::Concept(c) => &c.evidence_ids,
            Self::Relation(c) => &c.evidence_ids,
            Self::Hyperedge(c) => &c.evidence_ids,
        }
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Subject<'a> {
    selected_graph: &'a MeaningSelectionRef,
    packet: &'a MeaningPacket,
    claim: Claim<'a>,
    support: Vec<&'a MeaningEvidence>,
}
struct Context<'a> {
    source: storage::Context<'a>,
    targets: BTreeSet<String>,
}
impl<'a> Context<'a> {
    fn new(store: &'a Store, o: &'a Options, d: &'a Deadline) -> Result<Self> {
        Ok(Self {
            source: storage::Context::new(store, o, d)?,
            targets: BTreeSet::new(),
        })
    }
    fn target(&mut self, t: &Target) -> Result<()> {
        validate_target(t)?;
        self.source.deadline.check()?;
        let key = identity(t);
        if self.targets.contains(&key) {
            return Ok(());
        }
        self.source.pair(&t.origin, &t.evaluated)?;
        self.subject(t)?;
        self.targets.insert(key);
        Ok(())
    }
    fn subject<'b>(&'b self, t: &Target) -> Result<Subject<'b>> {
        let pair = self
            .source
            .pairs
            .get(&identity(&(&t.origin, &t.evaluated)))
            .context("feedback_binding_invalid")?;
        let layer = pair
            .before()
            .view()
            .meaning
            .as_ref()
            .context("feedback_binding_invalid")?;
        let packet = layer
            .packets
            .iter()
            .find(|p| {
                p.index == t.reference.packet_index
                    && layer
                        .selections
                        .get(p.selection_index)
                        .is_some_and(|s| s.graph_id == t.reference.graph_id)
            })
            .context("feedback_binding_invalid")?;
        let selected_graph = layer
            .selections
            .get(packet.selection_index)
            .context("feedback_binding_invalid")?;
        let claim = match t.reference.kind {
            Kind::Concept => Claim::Concept(
                packet
                    .concepts
                    .iter()
                    .find(|c| c.claim_id == t.reference.claim_id)
                    .context("feedback_binding_invalid")?,
            ),
            Kind::Relation => Claim::Relation(
                packet
                    .relations
                    .iter()
                    .find(|c| c.claim_id == t.reference.claim_id)
                    .context("feedback_binding_invalid")?,
            ),
            Kind::Hyperedge => Claim::Hyperedge(
                packet
                    .hyperedges
                    .iter()
                    .find(|c| c.claim_id == t.reference.claim_id)
                    .context("feedback_binding_invalid")?,
            ),
        };
        let mut support = Vec::new();
        for id in claim.evidence_ids() {
            self.source.deadline.check()?;
            support.push(
                packet
                    .evidence
                    .iter()
                    .find(|e| e.id == *id)
                    .context("feedback_binding_invalid")?,
            );
        }
        ensure!(!support.is_empty(), "feedback_binding_invalid");
        Ok(Subject {
            selected_graph,
            packet,
            claim,
            support,
        })
    }
    fn check_event(&mut self, event: &Event) -> Result<()> {
        self.target(&event.draft.target)?;
        let t = &event.draft.target;
        let subject = self.subject(t)?;
        let pair = &self.source.pairs[&identity(&(&t.origin, &t.evaluated))];
        for evidence in &event.draft.source_evidence {
            self.source.deadline.check()?;
            let q = &evidence.source;
            ensure!(
                subject.support.iter().any(|s| s.path == q.path
                    && match evidence.side {
                        Side::Origin =>
                            s.content_hash == q.content_hash
                                && s.start_byte <= q.start_byte
                                && q.end_byte <= s.end_byte,
                        Side::Evaluated => true,
                    }),
                "feedback_binding_invalid"
            );
            let side = match evidence.side {
                Side::Origin => pair.before(),
                Side::Evaluated => pair.after(),
            };
            let scope = side.bind_named_scope(&side.primary_snapshot().id, self.source.deadline)?;
            let source = scope.source_named(&q.path, self.source.deadline)?;
            ensure!(
                source.member().file().hash == q.content_hash
                    && source.text().get(q.start_byte..q.end_byte) == Some(q.quote.as_str()),
                "feedback_binding_invalid"
            );
        }
        Ok(())
    }
}
#[derive(Serialize)]
struct WithSubject<'a> {
    #[serde(flatten)]
    base: Value,
    subject: Subject<'a>,
}
fn flags() -> Value {
    json!({"executionObserved":false,"workspaceApplicationObserved":false,
    "currentApplicabilityEstablished":false,"independentCorroborationEstablished":false,"interpretationTruthEstablished":false,
    "semanticIdentityEstablished":false,"sourceAuthority":false,"sourceWritten":false,"modelCalls":0,"complete":false,
    "trust":"authored_historical_meaning_outcome","evaluatedQuoteCorrespondence":"same_claim_support_path_only"})
}
pub fn record(store: &Store, draft: &Draft, o: &Options, d: &Deadline) -> Result<Value> {
    events::record(store, draft, o, d).map_err(|e| sanitize(e, d))
}
pub fn rank(store: &Store, input: &RankInput, o: &Options, d: &Deadline) -> Result<Value> {
    ranking::rank(store, input, o, d).map_err(|e| sanitize(e, d))
}
pub(super) fn retained_roots(store: &Store, d: &Deadline) -> Result<RetainedRoots> {
    store.set_deadline(d)?;
    let _read = storage::read_transaction(store)?;
    let o = Options::default();
    let mut c = Context::new(store, &o, d)?;
    let mut all = events::load(&mut c)?;
    events::sequences(&all, d)?;
    all.sort_by_key(|e| identity(&e.draft.target));
    let mut roots = RetainedRoots::default();
    let mut previous = None;
    for event in &all {
        d.check()?;
        let key = identity(&event.draft.target);
        let fresh = previous.as_ref() != Some(&key);
        if fresh {
            c = Context::new(store, &o, d)?;
            previous = Some(key);
        }
        c.check_event(event)?;
        if !fresh {
            continue;
        }
        for pair in c.source.pairs.values() {
            for side in [pair.before(), pair.after()] {
                roots.blob_ids.insert(side.view().artifact_id.clone());
                for snapshot in side.source_scopes() {
                    roots.snapshot_ids.insert(snapshot.id.clone());
                    for f in &snapshot.files {
                        d.check()?;
                        roots.blob_ids.insert(f.hash.clone());
                    }
                }
            }
        }
    }
    Ok(roots)
}
