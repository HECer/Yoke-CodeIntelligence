//! Only explicitly imported original UTF-8; literal candidates require host selection.
use super::*;
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Header {
    pub schema_version: u32,
    pub id: String,
    pub workspace_id: String,
    pub label: String,
    pub author: String,
    pub source_hash: String,
    pub source_bytes: usize,
    pub format: String,
    pub authorization_claim: String,
}
pub(super) struct CheckedTranscript {
    pub header: Header,
    pub text: String,
}
fn header_id(header: &Header) -> String {
    identity(&(
        TRANSCRIPT_KIND,
        &header.workspace_id,
        &header.label,
        &header.author,
        &header.source_hash,
        header.source_bytes,
        &header.format,
        &header.authorization_claim,
    ))
}
pub(super) fn validate_header(header: &Header, store: &Store) -> Result<()> {
    label(&header.label)?;
    label(&header.author)?;
    ensure!(
        header.schema_version == 1
            && header.workspace_id == store.workspace_id
            && valid_id(&header.source_hash)
            && header.id == header_id(header)
            && (1..=MAX_TRANSCRIPT_BYTES).contains(&header.source_bytes)
            && header.format == "text/plain; charset=utf-8"
            && header.authorization_claim == "explicit_host_selected_copy",
        "feedback_binding_invalid"
    );
    Ok(())
}
pub(super) fn validate(
    text: &[u8],
    name: &str,
    author: &str,
    options: &Options,
    d: &Deadline,
) -> Result<()> {
    validate_options(options)?;
    d.check()?;
    ensure!(
        !text.is_empty()
            && text.len() <= MAX_TRANSCRIPT_BYTES
            && text.len() <= options.max_source_bytes,
        "feedback_capacity_exceeded"
    );
    label(name)?;
    label(author)?;
    bounded_text(std::str::from_utf8(text)?, MAX_TRANSCRIPT_BYTES, false)?;
    d.check()
}
pub(super) fn import(
    store: &Store,
    text: &[u8],
    name: &str,
    author: &str,
    options: &Options,
    d: &Deadline,
) -> Result<Value> {
    validate(text, name, author, options, d)?;
    ensure!(store.db.is_autocommit(), "feedback_transaction_unavailable");
    store.set_deadline(d)?;
    let transaction = Transaction::new_unchecked(&store.db, TransactionBehavior::Immediate)?;
    let mut header = Header {
        schema_version: 1,
        id: String::new(),
        workspace_id: store.workspace_id.clone(),
        label: name.to_owned(),
        author: author.to_owned(),
        source_hash: hash(text),
        source_bytes: text.len(),
        format: "text/plain; charset=utf-8".into(),
        authorization_claim: "explicit_host_selected_copy".into(),
    };
    header.id = header_id(&header);
    validate_header(&header, store)?;
    let mut context = storage::Context::new(store, options, d)?;
    context.ids(TRANSCRIPT_KIND, 1000)?;
    let prior = context.raw(TRANSCRIPT_KIND, &header.id)?;
    if let Some(raw) = &prior {
        let old: Header =
            crate::json::decode(crate::json::parse(raw.as_bytes(), MAX_RECORD_BYTES)?)?;
        validate_header(&old, store)?;
        ensure!(old == header, "feedback_binding_invalid");
        ensure!(
            store.read_blob_exact(&header.source_hash, text.len(), MAX_TRANSCRIPT_BYTES)? == text,
            "feedback_binding_invalid"
        );
    }
    let inserted = prior.is_none();
    let encoded = crate::json::encode_bounded(&header, MAX_RECORD_BYTES, d)?;
    storage::quota(
        &store.db,
        0,
        usize::from(inserted),
        if inserted { encoded.len() } else { 0 },
    )?;
    let result = output(
        &json!({"id":header.id,"sourceHash":header.source_hash,"sourceBytes":header.source_bytes,
        "transcript":header,"inserted":inserted,"eventsCreated":0,"modelCalls":0,"sourceWritten":false,
        "ownershipAuthenticated":false,"trust":"explicit_untrusted_transcript_not_execution_authority"}),
        options,
        d,
    )?;
    d.check()?;
    if inserted {
        ensure!(
            store.blob(text)? == header.source_hash,
            "feedback_binding_invalid"
        );
        d.check()?;
        transaction.execute(
            "INSERT INTO records VALUES(?1,?2,?3)",
            params![TRANSCRIPT_KIND, header.id, String::from_utf8(encoded)?],
        )?;
    }
    d.check()?;
    transaction.commit()?;
    Ok(result)
}
#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
struct Signal {
    kind: &'static str,
    literal: &'static str,
}
const SIGNALS: [Signal; 12] = [
    Signal {
        kind: "success",
        literal: "tests passed",
    },
    Signal {
        kind: "success",
        literal: "test passed",
    },
    Signal {
        kind: "success",
        literal: "it worked",
    },
    Signal {
        kind: "success",
        literal: "funktioniert",
    },
    Signal {
        kind: "failure",
        literal: "tests failed",
    },
    Signal {
        kind: "failure",
        literal: "test failed",
    },
    Signal {
        kind: "failure",
        literal: "did not work",
    },
    Signal {
        kind: "failure",
        literal: "funktioniert nicht",
    },
    Signal {
        kind: "corroboration",
        literal: "confirmed",
    },
    Signal {
        kind: "corroboration",
        literal: "bestätigt",
    },
    Signal {
        kind: "contradiction",
        literal: "contradicted",
    },
    Signal {
        kind: "contradiction",
        literal: "widerlegt",
    },
];
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Citation<'a> {
    kind: &'static str,
    transcript_id: &'a str,
    content_hash: &'a str,
    start_byte: usize,
    end_byte: usize,
    quote: &'a str,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Candidate<'a> {
    candidate_kind: &'static str,
    signal: &'static str,
    source: Citation<'a>,
    requires_host_selection: bool,
}
pub(super) fn mine(store: &Store, id: &str, options: &Options, d: &Deadline) -> Result<Value> {
    validate_options(options)?;
    ensure!(valid_id(id), "feedback_input_invalid");
    store.set_deadline(d)?;
    let _transaction = storage::read_transaction(store)?;
    let mut context = storage::Context::new(store, options, d)?;
    context.transcript(id)?;
    let selected = &context.transcripts[id];
    let mut candidates = Vec::new();
    for signal in SIGNALS {
        d.check()?;
        let expression = regex::RegexBuilder::new(&regex::escape(signal.literal))
            .case_insensitive(true)
            .build()?;
        for matched in expression.find_iter(&selected.text) {
            d.check()?;
            ensure!(
                candidates.len() < options.max_candidates,
                "feedback_capacity_exceeded"
            );
            candidates.push(Candidate {
                candidate_kind: signal.kind,
                signal: signal.literal,
                requires_host_selection: true,
                source: Citation {
                    kind: "transcriptUtf8",
                    transcript_id: id,
                    content_hash: &selected.header.source_hash,
                    start_byte: matched.start(),
                    end_byte: matched.end(),
                    quote: matched.as_str(),
                },
            });
        }
    }
    candidates.sort_by(|a, b| {
        a.source
            .start_byte
            .cmp(&b.source.start_byte)
            .then_with(|| a.source.end_byte.cmp(&b.source.end_byte))
            .then_with(|| a.candidate_kind.cmp(b.candidate_kind))
            .then_with(|| a.signal.cmp(b.signal))
    });
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Mined<'a> {
        transcript: &'a Header,
        policy: Value,
        candidates: Vec<Candidate<'a>>,
        events_created: usize,
        semantic_understanding_established: bool,
        model_calls: usize,
        source_written: bool,
        complete: bool,
        limitation: &'static str,
    }
    output(
        &Mined {
            transcript: &selected.header,
            policy: json!({"id":"literal_feedback_signals_v1","signals":SIGNALS,
        "matching":"case_insensitive_original_utf8","candidateOnly":true}),
            candidates,
            events_created: 0,
            semantic_understanding_established: false,
            model_calls: 0,
            source_written: false,
            complete: false,
            limitation: "Literal signals may overlap or occur in negation or quoted speech; they are candidates for explicit host selection, not understood or authenticated outcomes.",
        },
        options,
        d,
    )
}
