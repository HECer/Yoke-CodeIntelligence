//! Historical authored outcomes; ranking grants no current applicability.
use super::*;

fn sign(outcome: &Outcome) -> i64 {
    match outcome {
        Outcome::Success | Outcome::Corroboration => 1,
        Outcome::Failure | Outcome::Contradiction => -1,
        Outcome::Inconclusive => 0,
    }
}
pub(super) fn rank(
    store: &Store,
    input: &RankInput,
    options: &Options,
    d: &Deadline,
) -> Result<Value> {
    validate_options(options)?;
    validate_rank(input, d)?;
    ensure!(
        input.targets.len() <= options.max_candidates,
        "feedback_capacity_exceeded"
    );
    store.set_deadline(d)?;
    let _transaction = storage::read_transaction(store)?;
    let mut context = Context::new(store, options, d)?;
    let events = events::load(&mut context)?;
    events::sequences(&events, d)?;
    let selected: BTreeSet<_> = input.targets.iter().map(identity).collect();
    for target in &input.targets {
        context.target(target)?;
    }
    let mut relevant = Vec::new();
    for event in &events {
        d.check()?;
        if selected.contains(&identity(&event.draft.target)) {
            context.check_event(event)?;
            relevant.push(event);
        }
    }
    let window = u64::from(input.decay_window_days) * DAY_MS;
    let mut items = Vec::new();
    for target in &input.targets {
        d.check()?;
        let observations: Vec<_> = relevant
            .iter()
            .copied()
            .filter(|e| e.draft.target == *target)
            .collect();
        let mut latest: BTreeMap<(&str, &str), &Event> = BTreeMap::new();
        for event in observations
            .iter()
            .copied()
            .filter(|e| e.draft.occurred_at_ms <= input.as_of_ms)
        {
            d.check()?;
            let slot = (event.draft.actor.as_str(), event.draft.session.as_str());
            if latest
                .get(&slot)
                .is_none_or(|prior| prior.draft.sequence < event.draft.sequence)
            {
                latest.insert(slot, event);
            }
        }
        let mut votes = Vec::new();
        let mut score = 0i64;
        for event in observations {
            d.check()?;
            let age = input.as_of_ms.checked_sub(event.draft.occurred_at_ms);
            let slot = (event.draft.actor.as_str(), event.draft.session.as_str());
            let disposition = if age.is_none() {
                "future"
            } else if latest.get(&slot).is_none_or(|prior| prior.id != event.id) {
                "superseded_in_session"
            } else {
                "counted"
            };
            let weight = age.map_or(0, |age| {
                (1000u128 * u128::from(window) / (u128::from(window) + u128::from(age))) as i64
            });
            let contribution = if disposition == "counted" {
                weight * sign(&event.draft.outcome)
            } else {
                0
            };
            score += contribution;
            votes.push(json!({"event":event,"disposition":disposition,"ageMs":age,
                "decayedWeight":weight,"contribution":contribution}));
        }
        items.push(
            json!({"target":target,"plan":context.plans[&target.plan_id],
            "score":score,"votingSessions":latest.len(),"events":votes}),
        );
    }
    items.sort_by(|a, b| {
        b["score"].as_i64().cmp(&a["score"].as_i64()).then_with(|| {
            a["target"]["planId"]
                .as_str()
                .cmp(&b["target"]["planId"].as_str())
        })
    });
    let total = items.len();
    items.truncate(input.limit);
    provenance::output_with_proofs(
        &json!({
            "schemaVersion":1,"asOfMs":input.as_of_ms,"decayWindowDays":input.decay_window_days,
            "items":items,"selectedTargets":total,"notReturnedByLimit":total.saturating_sub(input.limit),
            "eventsOutsideSelectedTargets":events.len()-relevant.len(),
            "evaluatedRepresentsPlannedResult":true,"executionObserved":false,
            "workspaceApplicationObserved":false,"currentApplicabilityEstablished":false,
            "independentCorroborationEstablished":false,"sourceAuthority":false,
            "sourceWritten":false,"modelCalls":0,"complete":false,"trust":"authored_historical_plan_outcome",
            "inputBytes":context.source.input_bytes,"sourceBytes":context.source.source_bytes
        }),
        &context.source,
        d,
    )
}
