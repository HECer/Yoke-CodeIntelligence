//! Integer, source-current ranking of explicitly attributed outcome assertions.
use super::*;
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AnchorState<'a> {
    path: &'a str,
    content_hash: &'a str,
    current_hash: Option<&'a str>,
    state: &'static str,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Vote<'a> {
    event: &'a Event,
    disposition: &'static str,
    age_ms: Option<u64>,
    decayed_weight: i64,
    contribution: i64,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Item<'a> {
    knowledge_id: &'a str,
    text: &'a str,
    origin_snapshot_id: &'a str,
    anchors: Vec<AnchorState<'a>>,
    stale: bool,
    authored_supersession_observed: bool,
    authored_supersession_ids: Vec<&'a str>,
    eligible: bool,
    raw_score: i64,
    effective_score: i64,
    voting_sessions: usize,
    events: Vec<Vote<'a>>,
}
fn anchors<'a>(
    anchors: &'a [Anchor],
    files: &BTreeMap<&str, &'a crate::model::SourceFile>,
    omissions: &BTreeSet<&str>,
    d: &Deadline,
) -> Result<Vec<AnchorState<'a>>> {
    anchors
        .iter()
        .map(|anchor| {
            d.check()?;
            let file = files.get(anchor.path.as_str()).copied();
            let state = match file {
                Some(file) if file.hash == anchor.content_hash => "current",
                Some(_) => "changed",
                None if omissions.contains(anchor.path.as_str()) => "excluded",
                None => "deleted",
            };
            Ok(AnchorState {
                path: &anchor.path,
                content_hash: &anchor.content_hash,
                current_hash: file.map(|f| f.hash.as_str()),
                state,
            })
        })
        .collect()
}
pub(super) fn rank(
    store: &Store,
    input: &RankInput,
    options: &Options,
    d: &Deadline,
) -> Result<Value> {
    validate_options(options)?;
    validate_rank(input, d)?;
    store.set_deadline(d)?;
    let _transaction = storage::read_transaction(store)?;
    let mut context = storage::Context::new(store, options, d)?;
    let events = context.events()?;
    storage::check_sequences(&events, d)?;
    let selected: BTreeSet<_> = input.knowledge_ids.iter().cloned().collect();
    let all_memory_ids = context.ids(MEMORY_KIND, 10_000)?;
    for id in &all_memory_ids {
        context.memory(id)?;
    }
    let current_key = context.pair(&input.origin, &input.current)?;
    for id in &selected {
        context.target(id, &current_key)?;
    }
    let mut superseded: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for memory in context.memories.values() {
        d.check()?;
        if let Some(parent) = &memory.supersedes {
            if !selected.contains(parent) {
                continue;
            }
            let old = context
                .memories
                .get(parent)
                .context("feedback_binding_invalid")?;
            ensure!(
                memory.id != old.id
                    && memory
                        .anchors
                        .iter()
                        .any(|a| old.anchors.iter().any(|b| a.path == b.path)),
                "feedback_binding_invalid"
            );
            context.supersession(&memory.id, &current_key)?;
            let successors = superseded.entry(parent.clone()).or_default();
            ensure!(successors.is_empty(), "feedback_binding_invalid");
            successors.push(memory.id.clone());
        }
    }
    let mut relevant = Vec::new();
    let mut outside_origin = 0usize;
    let mut outside_knowledge = 0usize;
    for event in &events {
        d.check()?;
        if event.draft.origin != input.origin {
            outside_origin += 1;
            continue;
        }
        if !selected.contains(&event.draft.knowledge_id) {
            outside_knowledge += 1;
            continue;
        }
        context.check_event(event)?;
        relevant.push(event);
    }
    let window = u64::from(input.decay_window_days) * DAY_MS;
    let current = context.pairs[&current_key].after().primary_snapshot();
    let mut files = BTreeMap::new();
    for file in &current.files {
        d.check()?;
        files.insert(file.path.as_str(), file);
    }
    let mut omissions = BTreeSet::new();
    for omission in &current.omissions {
        d.check()?;
        omissions.insert(omission.path.as_str());
    }
    let mut items = Vec::new();
    for id in &selected {
        d.check()?;
        let memory = &context.memories[id];
        let states = anchors(&memory.anchors, &files, &omissions, d)?;
        let stale = states.iter().any(|anchor| anchor.state != "current");
        let successors: Vec<&str> = superseded
            .get(id)
            .map(|ids| ids.iter().map(String::as_str).collect())
            .unwrap_or_default();
        let replaced = !successors.is_empty();
        let mut latest: BTreeMap<(&str, &str), &Event> = BTreeMap::new();
        for event in relevant
            .iter()
            .copied()
            .filter(|e| e.draft.knowledge_id == *id && e.draft.occurred_at_ms <= input.as_of_ms)
        {
            d.check()?;
            let key = (event.draft.actor.as_str(), event.draft.session.as_str());
            if latest
                .get(&key)
                .is_none_or(|prior| prior.draft.sequence < event.draft.sequence)
            {
                latest.insert(key, event);
            }
        }
        let mut votes = Vec::new();
        let mut score = 0i64;
        for event in relevant
            .iter()
            .copied()
            .filter(|e| e.draft.knowledge_id == *id)
        {
            d.check()?;
            let age = input.as_of_ms.checked_sub(event.draft.occurred_at_ms);
            let selected_vote = latest
                .get(&(event.draft.actor.as_str(), event.draft.session.as_str()))
                .is_some_and(|v| v.id == event.id);
            let disposition = if age.is_none() {
                "future_dated"
            } else if selected_vote {
                "latest_session_sequence"
            } else {
                "older_session_sequence"
            };
            let weight = age
                .map(|age| {
                    ((1000u128 * u128::from(window)) / (u128::from(window) + u128::from(age)))
                        as i64
                })
                .unwrap_or(0);
            let contribution = if !selected_vote {
                0
            } else {
                match event.draft.outcome {
                    Outcome::Success | Outcome::Corroboration => weight,
                    Outcome::Failure | Outcome::Contradiction => -weight,
                    Outcome::Inconclusive => 0,
                }
            };
            score = score
                .checked_add(contribution)
                .context("feedback_capacity_exceeded")?;
            votes.push(Vote {
                event,
                disposition,
                age_ms: age,
                decayed_weight: weight,
                contribution,
            });
        }
        let eligible = !stale && !replaced;
        items.push(Item {
            knowledge_id: id,
            text: &memory.text,
            origin_snapshot_id: &memory.snapshot_id,
            anchors: states,
            stale,
            authored_supersession_observed: replaced,
            authored_supersession_ids: successors,
            eligible,
            raw_score: score,
            effective_score: if eligible { score } else { 0 },
            voting_sessions: latest.len(),
            events: votes,
        });
    }
    items.sort_by(|a, b| {
        b.eligible
            .cmp(&a.eligible)
            .then_with(|| b.effective_score.cmp(&a.effective_score))
            .then_with(|| a.knowledge_id.cmp(b.knowledge_id))
    });
    let total = items.len();
    items.truncate(input.limit);
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Ranked<'a> {
        kind: &'static str,
        origin: &'a ViewPin,
        current: &'a ViewPin,
        as_of_ms: u64,
        policy: Value,
        items: Vec<Item<'a>>,
        total: usize,
        truncated: bool,
        outside_origin_events: usize,
        outside_knowledge_events: usize,
        input_bytes: usize,
        source_bytes: usize,
        independent_corroboration_established: bool,
        execution_verified: bool,
        model_calls: usize,
        source_written: bool,
        complete: bool,
        trust: &'static str,
    }
    let result = Ranked {
        kind: "source_bound_authored_feedback_rank",
        origin: &input.origin,
        current: &input.current,
        as_of_ms: input.as_of_ms,
        policy: json!({"method":"hyperbolic_integer_v1","baseWeight":1000,"decayWindowDays":input.decay_window_days,
            "windowMs":window,"formula":"floor(1000*windowMs/(windowMs+ageMs)); failure and contradiction negate; inconclusive zero",
            "actorSessionAuthority":"unverified_host_labels","latestSequenceOnly":true}),
        items,
        total,
        truncated: total > input.limit,
        outside_origin_events: outside_origin,
        outside_knowledge_events: outside_knowledge,
        input_bytes: context.input_bytes,
        source_bytes: context.source_bytes,
        independent_corroboration_established: false,
        execution_verified: false,
        model_calls: 0,
        source_written: false,
        complete: false,
        trust: "ranking_not_evidence_or_execution_authority",
    };
    super::provenance::output_with_proofs(&result, &context, d)
}
