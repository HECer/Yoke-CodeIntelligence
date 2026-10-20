//! Closed event decoding and atomic output-before-write publication.
use super::*;

pub(super) fn load(context: &mut Context<'_>) -> Result<Vec<Event>> {
    let mut events = Vec::new();
    for id in context
        .source
        .ids(EVENT_KIND, context.source.options.max_events)?
    {
        let raw = context
            .source
            .raw(EVENT_KIND, &id)?
            .context("feedback_binding_invalid")?;
        let event: Event =
            crate::json::decode(crate::json::parse(raw.as_bytes(), MAX_RECORD_BYTES)?)?;
        validate_draft(&event.draft, context.source.deadline)?;
        ensure!(
            event.schema_version == 1
                && event.id == id
                && event.workspace_id == context.source.store.workspace_id
                && event.draft.target.origin.workspace_id == context.source.store.workspace_id
                && event.draft == canonical(event.draft.clone())
                && event.id == event_id(&event.workspace_id, &event.draft),
            "feedback_binding_invalid"
        );
        events.push(event);
    }
    Ok(events)
}
pub(super) fn sequences(events: &[Event], d: &Deadline) -> Result<()> {
    let mut slots: BTreeMap<(&str, &str, &str), BTreeMap<u32, u64>> = BTreeMap::new();
    for event in events {
        d.check()?;
        let draft = &event.draft;
        let times = slots
            .entry((&draft.target.plan_id, &draft.actor, &draft.session))
            .or_default();
        ensure!(
            times.insert(draft.sequence, draft.occurred_at_ms).is_none(),
            "feedback_sequence_conflict"
        );
    }
    for times in slots.values() {
        d.check()?;
        ensure!(
            times
                .values()
                .zip(times.values().skip(1))
                .all(|(a, b)| a <= b),
            "feedback_sequence_conflict"
        );
    }
    Ok(())
}
pub(super) fn record(
    store: &Store,
    draft: &Draft,
    options: &Options,
    d: &Deadline,
) -> Result<Value> {
    validate_options(options)?;
    validate_draft(draft, d)?;
    ensure!(store.db.is_autocommit(), "feedback_transaction_unavailable");
    store.set_deadline(d)?;
    let transaction = Transaction::new_unchecked(&store.db, TransactionBehavior::Immediate)?;
    let draft = canonical(draft.clone());
    let event = Event {
        schema_version: 1,
        id: event_id(&store.workspace_id, &draft),
        workspace_id: store.workspace_id.clone(),
        draft,
    };
    let mut context = Context::new(store, options, d)?;
    let mut events = load(&mut context)?;
    let existing = events.iter().find(|e| e.id == event.id);
    let inserted = existing.is_none();
    if let Some(prior) = existing {
        ensure!(*prior == event, "feedback_binding_invalid");
    }
    if inserted {
        ensure!(
            events.len() < options.max_events,
            "feedback_capacity_exceeded"
        );
        events.push(event.clone());
    }
    sequences(&events, d)?;
    context.check_event(&event)?;
    let bytes = crate::json::encode_bounded(&event, MAX_RECORD_BYTES, d)?;
    let total: usize = store.db.query_row(
        "SELECT coalesce(sum(length(CAST(value AS BLOB))),0) FROM records WHERE kind=?1",
        [EVENT_KIND],
        |r| r.get(0),
    )?;
    ensure!(
        total
            .checked_add(if inserted { bytes.len() } else { 0 })
            .is_some_and(|n| n <= MAX_STORE_BYTES),
        "feedback_capacity_exceeded"
    );
    let result = provenance::output_with_proofs(
        &json!({
            "id":event.id,"event":event,"inserted":inserted,
            "plan":context.plans[&event.draft.target.plan_id],
            "evaluatedRepresentsPlannedResult":true,"executionObserved":false,
            "workspaceApplicationObserved":false,"independentCorroborationEstablished":false,
        "currentApplicabilityEstablished":false,
            "sourceAuthority":false,"sourceWritten":false,"modelCalls":0,"complete":false,
            "trust":"authored_historical_plan_outcome",
            "inputBytes":context.source.input_bytes,"sourceBytes":context.source.source_bytes
        }),
        &context.source,
        d,
    )?;
    d.check()?;
    if inserted {
        transaction.execute(
            "INSERT INTO records VALUES(?1,?2,?3)",
            params![EVENT_KIND, event.id, String::from_utf8(bytes)?],
        )?;
    }
    d.check()?;
    transaction.commit()?;
    Ok(result)
}
