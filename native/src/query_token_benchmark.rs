use crate::{
    capture::{Deadline, source_scope_policy},
    json::encode_bounded,
    model::{Snapshot, hash},
    retrieval_saved_graph,
    store::Store,
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};

const DEFAULT_QUESTIONS: [&str; 5] = [
    "how does authentication work",
    "what is the main entry point",
    "how are errors handled",
    "what connects the data layer to the api",
    "what are the core abstractions",
];

fn captured_words(store: &Store, snapshot: &Snapshot, deadline: &Deadline) -> Result<u64> {
    let scope = source_scope_policy(store, snapshot)?;
    let mut total = 0;
    for file in &snapshot.files {
        let source = scope.source_view(&file.path)?;
        total += source.text().split_whitespace().count() as u64;
        deadline.check()?;
    }
    Ok(total)
}

fn ratio(corpus: u64, query: u64) -> f64 {
    (corpus as f64 / query as f64 * 10.0).round_ties_even() / 10.0
}

/// Compare estimated corpus tokens with the exact serialized native context data.
pub fn token_benchmark(
    store: &Store,
    snapshot_id: &str,
    questions: &[String],
    corpus_words: Option<u64>,
    limit: u64,
    deadline: &Deadline,
) -> Result<Value> {
    ensure!(
        questions.len() <= 20
            && questions
                .iter()
                .all(|question| question.len() <= 1000 && !question.trim().is_empty())
            && (1..=20).contains(&limit),
        "Graph benchmark needs up to 20 nonempty questions of at most 1000 bytes and a limit of 1 to 20"
    );
    store.set_deadline(deadline)?;
    let snapshot = store.snapshot(snapshot_id)?;
    let graph = retrieval_saved_graph::read_graph(store, snapshot_id, deadline)?
        .context("Graph benchmark needs a saved graph for the selected snapshot")?;
    let (words, baseline) = match corpus_words {
        Some(words) => (words, "host_supplied_word_count"),
        None => (
            captured_words(store, &snapshot, deadline)?,
            "captured_text_word_count",
        ),
    };
    let corpus = words
        .checked_add(words / 3)
        .context("Corpus token estimate overflow")?;

    let questions: Vec<&str> = if questions.is_empty() {
        DEFAULT_QUESTIONS.to_vec()
    } else {
        questions.iter().map(String::as_str).collect()
    };
    let mut results = Vec::with_capacity(questions.len());
    let mut matched = 0;
    let mut total = 0;
    for question in questions {
        let data = super::context(
            store,
            &snapshot,
            &graph,
            &json!({"query": question, "limit": limit}),
            deadline,
        )?;
        let returned = data["items"]
            .as_array()
            .context("Context result is missing its items")?
            .len();
        let payload = encode_bounded(&data, 4_000_000, deadline)?;
        let chars = std::str::from_utf8(&payload)?.chars().count();
        let estimate = if returned == 0 {
            0
        } else {
            (chars as u64 / 4).max(1)
        };
        matched += u64::from(returned > 0);
        total += estimate;
        results.push(json!({
            "question": question,
            "returnedItems": returned,
            "contextCharacters": chars,
            "contextSha256": hash(&payload),
            "estimatedQueryTokens": estimate,
            "reductionRatio": (estimate > 0).then(|| ratio(corpus, estimate)),
        }));
    }

    let average = total.checked_div(matched);
    Ok(json!({
        "schemaVersion": 1,
        "snapshotId": snapshot.id,
        "capturedFiles": snapshot.files.len(),
        "omittedFiles": snapshot.omissions.len(),
        "graphNodes": graph.entities.len(),
        "graphObservations": graph.observations.len(),
        "corpusBaseline": baseline,
        "corpusWords": words,
        "estimatedCorpusTokens": corpus,
        "contextFormat": "compact_json_context_data",
        "contextLimit": limit,
        "matchedQuestions": matched,
        "averageEstimatedQueryTokens": average,
        "reductionRatio": average.map(|average| ratio(corpus, average)),
        "questions": results,
    }))
}
