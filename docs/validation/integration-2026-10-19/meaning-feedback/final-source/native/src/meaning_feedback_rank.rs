//! Historical votes over one original packet-qualified inference. Every counted
//! event keeps its own original evaluated View, including across later selections.
use super::*;

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
    target: &'a Target,
    subject_id: String,
    subject: Subject<'a>,
    score: i64,
    voting_sessions: usize,
    events: Vec<Vote<'a>>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Ranked<'a> {
    #[serde(flatten)]
    base: Value,
    items: Vec<Item<'a>>,
}
fn sign(o: &Outcome) -> i64 {
    match o {
        Outcome::Success | Outcome::Corroboration => 1,
        Outcome::Failure | Outcome::Contradiction => -1,
        Outcome::Inconclusive => 0,
    }
}
pub(super) fn rank(store: &Store, input: &RankInput, o: &Options, d: &Deadline) -> Result<Value> {
    validate_options(o)?;
    validate_rank(input, d)?;
    ensure!(
        input.targets.len() <= o.max_candidates,
        "feedback_capacity_exceeded"
    );
    store.set_deadline(d)?;
    let _read = storage::read_transaction(store)?;
    let mut c = Context::new(store, o, d)?;
    let all = events::load(&mut c)?;
    events::sequences(&all, d)?;
    let selected: BTreeSet<_> = input.targets.iter().map(subject_id).collect();
    for t in &input.targets {
        c.target(t)?;
    }
    let mut grouped = BTreeMap::<String, Vec<&Event>>::new();
    let mut relevant = 0usize;
    for event in &all {
        d.check()?;
        let id = subject_id(&event.draft.target);
        if selected.contains(&id) {
            c.check_event(event)?;
            grouped.entry(id).or_default().push(event);
            relevant += 1;
        }
    }
    let window = u64::from(input.decay_window_days) * DAY_MS;
    let mut items = Vec::new();
    for t in &input.targets {
        d.check()?;
        let id = subject_id(t);
        let observations = grouped.get(&id).map_or(&[][..], Vec::as_slice);
        let mut latest = BTreeMap::<(&str, &str), &Event>::new();
        for event in observations
            .iter()
            .copied()
            .filter(|e| e.draft.occurred_at_ms <= input.as_of_ms)
        {
            d.check()?;
            let slot = (event.draft.actor.as_str(), event.draft.session.as_str());
            if latest
                .get(&slot)
                .is_none_or(|p| p.draft.sequence < event.draft.sequence)
            {
                latest.insert(slot, event);
            }
        }
        let mut votes = Vec::new();
        let mut score = 0i64;
        for event in observations.iter().copied() {
            d.check()?;
            let age = input.as_of_ms.checked_sub(event.draft.occurred_at_ms);
            let slot = (event.draft.actor.as_str(), event.draft.session.as_str());
            let disposition = if age.is_none() {
                "future"
            } else if latest.get(&slot).is_none_or(|p| p.id != event.id) {
                "superseded_in_session"
            } else {
                "counted"
            };
            let weight = age.map_or(0, |a| {
                (1000u128 * u128::from(window) / (u128::from(window) + u128::from(a))) as i64
            });
            let contribution = if disposition == "counted" {
                weight * sign(&event.draft.outcome)
            } else {
                0
            };
            score += contribution;
            votes.push(Vote {
                event,
                disposition,
                age_ms: age,
                decayed_weight: weight,
                contribution,
            });
        }
        items.push(Item {
            target: t,
            subject_id: id,
            subject: c.subject(t)?,
            score,
            voting_sessions: latest.len(),
            events: votes,
        });
    }
    items.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| a.subject_id.cmp(&b.subject_id))
    });
    let total = items.len();
    items.truncate(input.limit);
    let mut base = flags();
    base["schemaVersion"] = json!(1);
    base["asOfMs"] = json!(input.as_of_ms);
    base["decayWindowDays"] = json!(input.decay_window_days);
    base["selectedTargets"] = json!(total);
    base["notReturnedByLimit"] = json!(total.saturating_sub(input.limit));
    base["eventsOutsideSelectedSubjects"] = json!(all.len() - relevant);
    base["inputBytes"] = json!(c.source.input_bytes);
    base["sourceBytes"] = json!(c.source.source_bytes);
    provenance::output_with_proofs(&Ranked { base, items }, &c.source, d)
}
