//! Frozen read admission and immutable transactional outcome publication.
use super::*;
use anyhow::Context as _;
use rusqlite::{Connection, limits::Limit};

pub(super) fn limited<T>(store: &Store, f: impl FnOnce() -> Result<T>) -> Result<T> {
    let prior = store.db.limit(Limit::SQLITE_LIMIT_LENGTH)?;
    store.db.set_limit(
        Limit::SQLITE_LIMIT_LENGTH,
        prior.min((MAX_RECORD_BYTES + 65_536) as i32),
    )?;
    let result = f();
    store.db.set_limit(Limit::SQLITE_LIMIT_LENGTH, prior)?;
    result
}
pub(super) fn read_transaction(store: &Store) -> Result<Option<Transaction<'_>>> {
    if store.db.is_autocommit() {
        Ok(Some(Transaction::new_unchecked(
            &store.db,
            TransactionBehavior::Deferred,
        )?))
    } else {
        Ok(None)
    }
}
pub(super) struct Context<'a> {
    pub store: &'a Store,
    pub options: &'a Options,
    pub deadline: &'a Deadline,
    pub input_bytes: usize,
    pub source_bytes: usize,
    admitted: BTreeMap<(String, String), (usize, String)>,
    pub pairs: BTreeMap<String, CheckedComparisonPair<'static>>,
    pub memories: BTreeMap<String, Memory>,
    pub transcripts: BTreeMap<String, transcript::CheckedTranscript>,
    targets: BTreeSet<(String, String)>,
}
impl<'a> Context<'a> {
    pub fn new(store: &'a Store, options: &'a Options, d: &'a Deadline) -> Result<Self> {
        validate_options(options)?;
        d.check()?;
        store.set_deadline(d)?;
        Ok(Self {
            store,
            options,
            deadline: d,
            input_bytes: 0,
            source_bytes: 0,
            admitted: BTreeMap::new(),
            pairs: BTreeMap::new(),
            memories: BTreeMap::new(),
            transcripts: BTreeMap::new(),
            targets: BTreeSet::new(),
        })
    }
    pub fn ids(&self, kind: &str, maximum: usize) -> Result<Vec<String>> {
        self.deadline.check()?;
        limited(self.store, || {
            let (count,bytes,bad):(usize,usize,usize)=self.store.db.query_row(
                "SELECT count(*),coalesce(sum(length(CAST(value AS BLOB))),0),coalesce(sum(CASE WHEN length(CAST(id AS BLOB))!=64 OR length(CAST(value AS BLOB))>?2 THEN 1 ELSE 0 END),0) FROM records WHERE kind=?1",
                params![kind,MAX_RECORD_BYTES],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
            ensure!(
                count <= maximum && bytes <= MAX_STORE_BYTES && bad == 0,
                "feedback_capacity_exceeded"
            );
            let mut stmt = self
                .store
                .db
                .prepare("SELECT id FROM records WHERE kind=?1 ORDER BY id")?;
            let result = stmt
                .query_map([kind], |r| r.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            ensure!(
                result.len() == count && result.iter().all(|id| valid_id(id)),
                "feedback_binding_invalid"
            );
            Ok(result)
        })
    }
    pub fn raw(&mut self, kind: &str, id: &str) -> Result<Option<String>> {
        self.deadline.check()?;
        ensure!(valid_id(id), "feedback_input_invalid");
        let raw = limited(self.store, || {
            let length: Option<usize> = self
                .store
                .db
                .query_row(
                    "SELECT length(CAST(value AS BLOB)) FROM records WHERE kind=?1 AND id=?2",
                    params![kind, id],
                    |r| r.get(0),
                )
                .optional()?;
            if let Some(length) = length {
                ensure!(length <= MAX_RECORD_BYTES, "feedback_capacity_exceeded");
                if !self
                    .admitted
                    .contains_key(&(kind.to_owned(), id.to_owned()))
                {
                    ensure!(
                        length
                            <= self
                                .options
                                .max_input_bytes
                                .saturating_sub(self.input_bytes),
                        "feedback_capacity_exceeded"
                    );
                }
            }
            let raw:Option<String>=self.store.db.query_row(
                "SELECT value FROM records WHERE kind=?1 AND id=?2 AND length(CAST(value AS BLOB))<=?3",
                params![kind,id,MAX_RECORD_BYTES],|r|r.get(0)).optional()?;
            ensure!(
                raw.as_ref().map(String::len) == length,
                "feedback_binding_invalid"
            );
            Ok(raw)
        })?;
        if let Some(raw) = &raw {
            let key = (kind.to_owned(), id.to_owned());
            let proof = (raw.len(), hash(raw.as_bytes()));
            if let Some(prior) = self.admitted.get(&key) {
                ensure!(*prior == proof, "feedback_binding_invalid");
            } else {
                self.input_bytes += raw.len();
                self.admitted.insert(key, proof);
            }
        }
        Ok(raw)
    }
    pub fn pair(&mut self, origin: &ViewPin, current: &ViewPin) -> Result<String> {
        self.deadline.check()?;
        origin.validate()?;
        current.validate()?;
        ensure!(
            origin.workspace_id == self.store.workspace_id
                && current.workspace_id == self.store.workspace_id,
            "feedback_binding_invalid"
        );
        let key = identity(&(origin, current));
        if !self.pairs.contains_key(&key) {
            ensure!(
                self.pairs.len() < self.options.max_pairs,
                "feedback_capacity_exceeded"
            );
            let limits = ComparisonOptions {
                max_input_bytes: self
                    .options
                    .max_input_bytes
                    .saturating_sub(self.input_bytes),
                max_source_bytes: self
                    .options
                    .max_source_bytes
                    .saturating_sub(self.source_bytes),
                ..Default::default()
            };
            let pair = comparison_input::prepare_published_pair(
                self.store,
                &origin.native(),
                &current.native(),
                &limits,
                self.deadline,
            )?;
            self.input_bytes += pair.accounting().total_proof_bytes();
            self.source_bytes += pair.accounting().distinct_source_bytes();
            ensure!(
                self.input_bytes <= self.options.max_input_bytes
                    && self.source_bytes <= self.options.max_source_bytes,
                "feedback_capacity_exceeded"
            );
            self.pairs.insert(key.clone(), pair);
        }
        Ok(key)
    }
    pub fn memory(&mut self, id: &str) -> Result<()> {
        if self.memories.contains_key(id) {
            return Ok(());
        }
        let raw = self
            .raw(MEMORY_KIND, id)?
            .context("feedback_binding_invalid")?;
        ensure!(raw.len() <= 64_000, "feedback_capacity_exceeded");
        let mut value: Memory = crate::json::decode(crate::json::parse(raw.as_bytes(), 64_000)?)?;
        ensure!(
            value.id == id
                && value.workspace_id == self.store.workspace_id
                && valid_id(&value.snapshot_id)
                && (1..=100).contains(&value.anchors.len())
                && value.supersedes.as_ref().is_none_or(|id| valid_id(id))
                && !value.created_at_nonce.is_empty()
                && value.created_at_nonce.len() <= 40
                && value.created_at_nonce.bytes().all(|v| v.is_ascii_digit()),
            "feedback_binding_invalid"
        );
        bounded_text(&value.text, 20_000, false)?;
        label(&value.actor)?;
        let mut paths = BTreeSet::new();
        for anchor in &value.anchors {
            ensure!(
                crate::paths::normalized(&anchor.path)? == anchor.path
                    && valid_id(&anchor.content_hash)
                    && paths.insert(&anchor.path),
                "feedback_binding_invalid"
            );
        }
        value.id.clear();
        ensure!(identity(&value) == id, "feedback_binding_invalid");
        value.id = id.to_owned();
        self.memories.insert(id.to_owned(), value);
        Ok(())
    }
    pub fn target(&mut self, id: &str, pair_key: &str) -> Result<()> {
        self.memory(id)?;
        let key = (pair_key.to_owned(), id.to_owned());
        if self.targets.contains(&key) {
            return Ok(());
        }
        let pair = &self.pairs[pair_key];
        let snapshot = pair.before().primary_snapshot();
        let memory = &self.memories[id];
        ensure!(
            memory.snapshot_id == snapshot.id,
            "feedback_binding_invalid"
        );
        let bound = pair
            .before()
            .bind_named_scope(&snapshot.id, self.deadline)?;
        for anchor in &memory.anchors {
            self.deadline.check()?;
            let source = bound.source_named(&anchor.path, self.deadline)?;
            ensure!(
                source.member().file().hash == anchor.content_hash,
                "feedback_binding_invalid"
            );
        }
        self.targets.insert(key);
        Ok(())
    }
    pub fn supersession(&self, id: &str, pair_key: &str) -> Result<()> {
        let memory = &self.memories[id];
        let pair = &self.pairs[pair_key];
        for side in [pair.before(), pair.after()] {
            self.deadline.check()?;
            if let Some(snapshot) = side
                .source_scopes()
                .find(|scope| scope.id == memory.snapshot_id)
            {
                let bound = side.bind_named_scope(&snapshot.id, self.deadline)?;
                for anchor in &memory.anchors {
                    self.deadline.check()?;
                    let source = bound.source_named(&anchor.path, self.deadline)?;
                    ensure!(
                        source.member().file().hash == anchor.content_hash,
                        "feedback_binding_invalid"
                    );
                }
                return Ok(());
            }
        }
        anyhow::bail!("feedback_binding_invalid")
    }
    pub fn events(&mut self) -> Result<Vec<Event>> {
        let ids = self.ids(EVENT_KIND, self.options.max_events)?;
        let mut result = Vec::with_capacity(ids.len());
        for id in ids {
            let raw = self
                .raw(EVENT_KIND, &id)?
                .context("feedback_binding_invalid")?;
            let event: Event =
                crate::json::decode(crate::json::parse(raw.as_bytes(), MAX_RECORD_BYTES)?)?;
            validate_draft(&event.draft, self.deadline)?;
            ensure!(
                event.schema_version == 1
                    && event.id == id
                    && event.workspace_id == self.store.workspace_id
                    && event.draft.origin.workspace_id == self.store.workspace_id
                    && event.draft == canonical(event.draft.clone())
                    && event.id == event_id(&event.workspace_id, &event.draft),
                "feedback_binding_invalid"
            );
            result.push(event);
        }
        Ok(result)
    }
    pub fn check_event(&mut self, event: &Event) -> Result<()> {
        let key = self.pair(&event.draft.origin, &event.draft.evaluated)?;
        self.target(&event.draft.knowledge_id, &key)?;
        let memory = &self.memories[&event.draft.knowledge_id];
        let pair = &self.pairs[&key];
        let snapshot = pair.after().primary_snapshot();
        let bound = pair.after().bind_named_scope(&snapshot.id, self.deadline)?;
        for quote in &event.draft.source_evidence {
            self.deadline.check()?;
            ensure!(
                memory
                    .anchors
                    .iter()
                    .any(|anchor| anchor.path == quote.path),
                "feedback_binding_invalid"
            );
            let source = bound.source_named(&quote.path, self.deadline)?;
            ensure!(
                source.member().file().hash == quote.content_hash,
                "feedback_binding_invalid"
            );
            ensure!(
                source.text().get(quote.start_byte..quote.end_byte) == Some(quote.quote.as_str()),
                "feedback_binding_invalid"
            );
        }
        for quote in &event.draft.transcript_evidence {
            self.transcript(&quote.transcript_id)?;
            let transcript = &self.transcripts[&quote.transcript_id];
            ensure!(
                transcript.header.source_hash == quote.content_hash
                    && transcript.text.get(quote.start_byte..quote.end_byte)
                        == Some(quote.quote.as_str()),
                "feedback_binding_invalid"
            );
        }
        Ok(())
    }
    pub fn transcript(&mut self, id: &str) -> Result<()> {
        if self.transcripts.contains_key(id) {
            return Ok(());
        }
        let raw = self
            .raw(TRANSCRIPT_KIND, id)?
            .context("feedback_binding_invalid")?;
        let header: transcript::Header =
            crate::json::decode(crate::json::parse(raw.as_bytes(), MAX_RECORD_BYTES)?)?;
        transcript::validate_header(&header, self.store)?;
        ensure!(
            header.id == id
                && header.source_bytes
                    <= self
                        .options
                        .max_source_bytes
                        .saturating_sub(self.source_bytes),
            "feedback_capacity_exceeded"
        );
        self.source_bytes += header.source_bytes;
        let bytes = self.store.read_blob_exact(
            &header.source_hash,
            header.source_bytes,
            MAX_TRANSCRIPT_BYTES,
        )?;
        let text = String::from_utf8(bytes)?;
        bounded_text(&text, MAX_TRANSCRIPT_BYTES, false)?;
        self.transcripts.insert(
            id.to_owned(),
            transcript::CheckedTranscript { header, text },
        );
        Ok(())
    }
}
fn same_slot(a: &Draft, b: &Draft) -> bool {
    a.origin == b.origin
        && a.knowledge_id == b.knowledge_id
        && a.actor == b.actor
        && a.session == b.session
}
pub(super) fn check_sequences(events: &[Event], d: &Deadline) -> Result<()> {
    let mut sessions: BTreeMap<(String, &str, &str, &str), BTreeMap<u32, u64>> = BTreeMap::new();
    for event in events {
        d.check()?;
        let draft = &event.draft;
        let session = sessions
            .entry((
                identity(&draft.origin),
                &draft.knowledge_id,
                &draft.actor,
                &draft.session,
            ))
            .or_default();
        ensure!(
            session
                .insert(draft.sequence, draft.occurred_at_ms)
                .is_none(),
            "feedback_sequence_conflict"
        );
    }
    for sequence in sessions.values() {
        d.check()?;
        ensure!(
            sequence
                .values()
                .zip(sequence.values().skip(1))
                .all(|(a, b)| a <= b),
            "feedback_sequence_conflict"
        );
    }
    Ok(())
}
pub(super) fn quota(
    db: &Connection,
    event_count: usize,
    transcript_count: usize,
    add_bytes: usize,
) -> Result<()> {
    let (count,bytes,transcripts):(usize,usize,usize)=db.query_row(
        "SELECT count(*),coalesce(sum(length(CAST(value AS BLOB))),0),coalesce(sum(CASE WHEN kind=?2 THEN 1 ELSE 0 END),0) FROM records WHERE kind IN (?1,?2)",
        params![EVENT_KIND,TRANSCRIPT_KIND],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?)))?;
    ensure!(
        count - transcripts + event_count <= 10_000
            && transcripts + transcript_count <= 1000
            && bytes
                .checked_add(add_bytes)
                .is_some_and(|size| size <= MAX_STORE_BYTES),
        "feedback_capacity_exceeded"
    );
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
    let mut events = context.events()?;
    let existing = events.iter().find(|value| value.id == event.id);
    let inserted = existing.is_none();
    if let Some(existing) = existing {
        ensure!(*existing == event, "feedback_binding_invalid");
    }
    if inserted {
        ensure!(
            events.len() < options.max_events,
            "feedback_capacity_exceeded"
        );
        ensure!(
            !events
                .iter()
                .any(|value| same_slot(&value.draft, &event.draft)
                    && value.draft.sequence == event.draft.sequence),
            "feedback_sequence_conflict"
        );
        events.push(event.clone());
    }
    check_sequences(&events, d)?;
    context.check_event(&event)?;
    let bytes = crate::json::encode_bounded(&event, MAX_RECORD_BYTES, d)?;
    quota(
        &store.db,
        usize::from(inserted),
        0,
        if inserted { bytes.len() } else { 0 },
    )?;
    let result = super::provenance::output_with_proofs(
        &json!({"id":event.id,"event":event,"inserted":inserted,"executionVerified":false,
        "independentCorroborationEstablished":false,"modelCalls":0,"sourceWritten":false,"complete":false,
        "trust":"authored_data_not_execution_authority","inputBytes":context.input_bytes,"sourceBytes":context.source_bytes}),
        &context,
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
pub(super) fn retained_roots(store: &Store, d: &Deadline) -> Result<RetainedRoots> {
    store.set_deadline(d)?;
    let _transaction = read_transaction(store)?;
    let options = Options::default();
    let mut admission = Context::new(store, &options, d)?;
    let mut events = admission.events()?;
    check_sequences(&events, d)?;
    let transcript_ids = admission.ids(TRANSCRIPT_KIND, 1000)?;
    let mut roots = RetainedRoots::default();
    // Retention streams independently bounded source closures. It does not
    // require the entire historical Store to fit one query's simultaneous cache.
    for id in transcript_ids {
        d.check()?;
        let mut context = Context::new(store, &options, d)?;
        context.transcript(&id)?;
        roots
            .blob_ids
            .insert(context.transcripts[&id].header.source_hash.clone());
    }
    events.sort_by_key(|event| identity(&(&event.draft.origin, &event.draft.evaluated)));
    let mut context = Context::new(store, &options, d)?;
    let mut prior_pair = None;
    for event in &events {
        d.check()?;
        let key = identity(&(&event.draft.origin, &event.draft.evaluated));
        let new_pair = prior_pair.as_ref() != Some(&key);
        if new_pair {
            context = Context::new(store, &options, d)?;
            prior_pair = Some(key);
        } else {
            // Reuse the checked native pair; release each event's scratch data.
            context.admitted.clear();
            context.memories.clear();
            context.transcripts.clear();
            context.targets.clear();
            context.input_bytes = context
                .pairs
                .values()
                .map(|p| p.accounting().total_proof_bytes())
                .sum();
            context.source_bytes = context
                .pairs
                .values()
                .map(|p| p.accounting().distinct_source_bytes())
                .sum();
        }
        context.check_event(event)?;
        if !new_pair {
            continue;
        }
        for pair in context.pairs.values() {
            for side in [pair.before(), pair.after()] {
                roots.blob_ids.insert(side.view().artifact_id.clone());
                for scope in side.source_scopes() {
                    d.check()?;
                    roots.snapshot_ids.insert(scope.id.clone());
                    for file in &scope.files {
                        d.check()?;
                        roots.blob_ids.insert(file.hash.clone());
                    }
                }
            }
        }
    }
    Ok(roots)
}
