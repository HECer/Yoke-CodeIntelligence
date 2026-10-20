//! Separate immutable namespace; exact sources and complete output precede SQL.
use super::*;

pub(super) fn load(c: &mut Context<'_>) -> Result<Vec<Event>> {
    let mut result = Vec::new();
    for id in c.source.ids(EVENT_KIND, c.source.options.max_events)? {
        let raw = c
            .source
            .raw(EVENT_KIND, &id)?
            .context("feedback_binding_invalid")?;
        let event: Event =
            crate::json::decode(crate::json::parse(raw.as_bytes(), MAX_RECORD_BYTES)?)?;
        validate_draft(&event.draft, c.source.deadline)?;
        ensure!(
            event.schema_version == 1
                && event.id == id
                && event.workspace_id == c.source.store.workspace_id
                && event.draft.target.origin.workspace_id == c.source.store.workspace_id
                && event.draft == canonical(event.draft.clone())
                && event.id == event_id(&event.workspace_id, &event.draft),
            "feedback_binding_invalid"
        );
        result.push(event);
    }
    Ok(result)
}
pub(super) fn sequences(events: &[Event], d: &Deadline) -> Result<()> {
    let mut slots: BTreeMap<(String, &str, &str), BTreeMap<u32, u64>> = BTreeMap::new();
    for event in events {
        d.check()?;
        let v = &event.draft;
        let times = slots
            .entry((subject_id(&v.target), &v.actor, &v.session))
            .or_default();
        ensure!(
            times.insert(v.sequence, v.occurred_at_ms).is_none(),
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
pub(super) fn record(store: &Store, draft: &Draft, o: &Options, d: &Deadline) -> Result<Value> {
    validate_options(o)?;
    validate_draft(draft, d)?;
    ensure!(store.db.is_autocommit(), "feedback_transaction_unavailable");
    store.set_deadline(d)?;
    let tx = Transaction::new_unchecked(&store.db, TransactionBehavior::Immediate)?;
    let draft = canonical(draft.clone());
    let event = Event {
        schema_version: 1,
        id: event_id(&store.workspace_id, &draft),
        workspace_id: store.workspace_id.clone(),
        draft,
    };
    let mut c = Context::new(store, o, d)?;
    let mut all = load(&mut c)?;
    let previous = all.iter().find(|e| e.id == event.id);
    let inserted = previous.is_none();
    if let Some(previous) = previous {
        ensure!(*previous == event, "feedback_binding_invalid");
    }
    if inserted {
        ensure!(all.len() < o.max_events, "feedback_capacity_exceeded");
        all.push(event.clone());
    }
    sequences(&all, d)?;
    c.check_event(&event)?;
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
    let mut base = flags();
    base["id"] = json!(event.id);
    base["event"] = json!(event);
    base["inserted"] = json!(inserted);
    base["subjectId"] = json!(subject_id(&event.draft.target));
    base["inputBytes"] = json!(c.source.input_bytes);
    base["sourceBytes"] = json!(c.source.source_bytes);
    let result = provenance::output_with_proofs(
        &WithSubject {
            base,
            subject: c.subject(&event.draft.target)?,
        },
        &c.source,
        d,
    )?;
    d.check()?;
    if inserted {
        tx.execute(
            "INSERT INTO records VALUES(?1,?2,?3)",
            params![EVENT_KIND, event.id, String::from_utf8(bytes)?],
        )?;
    }
    d.check()?;
    tx.commit()?;
    Ok(result)
}
