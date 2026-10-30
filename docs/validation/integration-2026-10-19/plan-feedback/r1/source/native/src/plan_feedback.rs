//! Authored historical plan outcomes, bound to replayed planned source bytes.
//! A checked plan/result does not establish application, execution or identity.
use super::*;
use crate::changes::Plan;

#[path = "plan_feedback_store.rs"]
mod events;
#[path = "plan_feedback_rank.rs"]
mod ranking;

pub const EVENT_KIND: &str = "native_plan_feedback_event_v1";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Target {
    pub plan_id: String,
    pub origin: ViewPin,
    pub evaluated: ViewPin,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Origin,
    Evaluated,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Evidence {
    pub side: Side,
    pub source: SourceQuote,
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
fn validate_target(target: &Target) -> Result<()> {
    target.origin.validate()?;
    target.evaluated.validate()?;
    ensure!(
        valid_id(&target.plan_id) && target.origin.workspace_id == target.evaluated.workspace_id,
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
    let mut quotes = BTreeSet::new();
    let mut bytes = 0usize;
    for evidence in &draft.source_evidence {
        d.check()?;
        let q = &evidence.source;
        ensure!(
            crate::paths::normalized(&q.path)? == q.path
                && valid_id(&q.content_hash)
                && q.end_byte >= q.start_byte
                && q.end_byte - q.start_byte == q.quote.len()
                && quotes.insert(evidence),
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
    let mut ids = BTreeSet::new();
    for target in &input.targets {
        d.check()?;
        validate_target(target)?;
        ensure!(ids.insert(&target.plan_id), "feedback_input_invalid");
    }
    d.check()
}
fn canonical(mut draft: Draft) -> Draft {
    draft.source_evidence.sort();
    draft
}
fn event_id(workspace: &str, draft: &Draft) -> String {
    identity(&(EVENT_KIND, workspace, draft))
}

struct Context<'a> {
    source: storage::Context<'a>,
    plans: BTreeMap<String, Plan>,
    targets: BTreeSet<String>,
}
impl<'a> Context<'a> {
    fn new(store: &'a Store, options: &'a Options, d: &'a Deadline) -> Result<Self> {
        Ok(Self {
            source: storage::Context::new(store, options, d)?,
            plans: BTreeMap::new(),
            targets: BTreeSet::new(),
        })
    }
    fn target(&mut self, target: &Target) -> Result<String> {
        let d = self.source.deadline;
        d.check()?;
        validate_target(target)?;
        let key = identity(target);
        let pair_key = identity(&(&target.origin, &target.evaluated));
        if self.targets.contains(&key) {
            return Ok(pair_key);
        }
        // Read admission precedes the native replay. Large plans are explicitly
        // unsupported by this finite reader, never silently clipped.
        if !self.plans.contains_key(&target.plan_id) {
            let raw = self
                .source
                .raw("plan", &target.plan_id)?
                .context("feedback_binding_invalid")?;
            let plan: Plan =
                crate::json::decode(crate::json::parse(raw.as_bytes(), MAX_RECORD_BYTES)?)?;
            ensure!(
                plan.id == target.plan_id
                    && plan.workspace_id == self.source.store.workspace_id
                    && plan.base_snapshot_id == target.origin.snapshot_id
                    && plan.result_snapshot_id == target.evaluated.snapshot_id,
                "feedback_binding_invalid"
            );
        }
        self.source.pair(&target.origin, &target.evaluated)?;
        if !self.plans.contains_key(&target.plan_id) {
            let plan = crate::changes::load_checked_plan(self.source.store, &target.plan_id, d)?;
            self.plans.insert(target.plan_id.clone(), plan);
        }
        let plan = &self.plans[&target.plan_id];
        let pair = &self.source.pairs[&pair_key];
        ensure!(
            plan.base_snapshot_id == target.origin.snapshot_id
                && plan.result_snapshot_id == target.evaluated.snapshot_id
                && pair.after().primary_snapshot().kind == "derived",
            "feedback_binding_invalid"
        );
        self.targets.insert(key);
        Ok(pair_key)
    }
    fn check_event(&mut self, event: &Event) -> Result<()> {
        let key = self.target(&event.draft.target)?;
        let plan = &self.plans[&event.draft.target.plan_id];
        let pair = &self.source.pairs[&key];
        for evidence in &event.draft.source_evidence {
            self.source.deadline.check()?;
            let q = &evidence.source;
            let file = plan
                .files
                .iter()
                .find(|f| f.path == q.path)
                .context("feedback_binding_invalid")?;
            let (side, expected) = match evidence.side {
                Side::Origin => (pair.before(), &file.before_hash),
                Side::Evaluated => (pair.after(), &file.after_hash),
            };
            ensure!(
                expected.as_ref() == Some(&q.content_hash),
                "feedback_binding_invalid"
            );
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

pub fn record(store: &Store, draft: &Draft, options: &Options, d: &Deadline) -> Result<Value> {
    events::record(store, draft, options, d).map_err(|error| sanitize(error, d))
}
pub fn rank(store: &Store, input: &RankInput, options: &Options, d: &Deadline) -> Result<Value> {
    ranking::rank(store, input, options, d).map_err(|error| sanitize(error, d))
}
pub(super) fn retained_roots(store: &Store, d: &Deadline) -> Result<RetainedRoots> {
    store.set_deadline(d)?;
    let _transaction = storage::read_transaction(store)?;
    let options = Options::default();
    let mut context = Context::new(store, &options, d)?;
    let events = events::load(&mut context)?;
    events::sequences(&events, d)?;
    let mut roots = RetainedRoots::default();
    // Each original pair is independently bounded, like knowledge feedback.
    for event in &events {
        d.check()?;
        let mut context = Context::new(store, &options, d)?;
        context.check_event(event)?;
        for pair in context.source.pairs.values() {
            for side in [pair.before(), pair.after()] {
                roots.blob_ids.insert(side.view().artifact_id.clone());
                for snapshot in side.source_scopes() {
                    roots.snapshot_ids.insert(snapshot.id.clone());
                    for file in &snapshot.files {
                        d.check()?;
                        roots.blob_ids.insert(file.hash.clone());
                    }
                }
            }
        }
    }
    Ok(roots)
}
