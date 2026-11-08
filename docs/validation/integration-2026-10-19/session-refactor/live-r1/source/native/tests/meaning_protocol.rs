//! Owned loopback Ollama accounting fixtures; no external model or credential use.
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::Duration,
};
use yoke::{
    capture::Deadline,
    meaning_graph::{self, Binding, Source},
    model::{hash, identity},
    ollama::{self, OllamaSpec},
};

const MODEL: &str = "owned-meaning-fixture:local";
struct Fixture {
    spec: OllamaSpec,
    stop: Arc<AtomicBool>,
    calls: Arc<AtomicUsize>,
    worker: Option<thread::JoinHandle<()>>,
}

fn read_request(stream: &mut TcpStream) -> Option<(String, Value)> {
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut header = Vec::new();
    while !header.ends_with(b"\r\n\r\n") {
        let mut byte = [0];
        stream.read_exact(&mut byte).ok()?;
        header.push(byte[0]);
        assert!(header.len() <= 16000);
    }
    let header = String::from_utf8(header).unwrap();
    assert!(!header.to_ascii_lowercase().contains("authorization:"));
    let path = header
        .lines()
        .next()
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .to_string();
    let length = header
        .lines()
        .find_map(|line| {
            line.split_once(':')
                .filter(|(key, _)| key.eq_ignore_ascii_case("content-length"))
                .map(|(_, value)| value.trim().parse::<usize>().unwrap())
        })
        .unwrap_or(0);
    assert!(length <= 1_000_000);
    let mut body = vec![0; length];
    stream.read_exact(&mut body).ok()?;
    Some((
        path,
        if body.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&body).unwrap()
        },
    ))
}

impl Fixture {
    fn start(mode: &'static str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let spec = OllamaSpec {
            endpoint: format!("http://{}", listener.local_addr().unwrap()),
            model: MODEL.into(),
            model_digest: "1".repeat(64),
        };
        let stop = Arc::new(AtomicBool::new(false));
        let done = stop.clone();
        let calls = Arc::new(AtomicUsize::new(0));
        let chat_calls = calls.clone();
        let worker = thread::spawn(move || {
            let mut tags = 0;
            while !done.load(Ordering::Acquire) {
                let Ok((mut stream, _)) = listener.accept() else {
                    thread::sleep(Duration::from_millis(2));
                    continue;
                };
                let Some((path, body)) = read_request(&mut stream) else {
                    continue;
                };
                let response = match path.as_str() {
                    "/api/tags" => {
                        tags += 1;
                        json!({"models":[{"name":MODEL,"model":MODEL,
                            "digest":if mode=="postcall-digest-change" && tags>1 {"2".repeat(64)} else {"1".repeat(64)},
                            "size":2000000,"details":{"format":"gguf"}}]})
                    }
                    "/api/show" => {
                        json!({"capabilities":["completion"],"details":{"format":"gguf"},
                        "model_info":{"general.architecture":"fixture","fixture.context_length":32768}})
                    }
                    "/api/chat" => {
                        chat_calls.fetch_add(1, Ordering::AcqRel);
                        let input: Value =
                            serde_json::from_str(body["messages"][1]["content"].as_str().unwrap())
                                .unwrap();
                        assert_eq!(input["operation"], "extract_meaning_v1");
                        assert_eq!(input["extractionPolicy"]["resolution"], "inferred_concept");
                        let data = json!({"concepts":[],"relations":[],"hyperedges":[],
                            "coverage":[{"sourceId":input["sources"][0]["anchor"]["sourceId"],
                                "outcome":if mode=="invalid-meaning" {"claims"} else {"no_claims"}}]});
                        let content = if mode == "duplicate-json" {
                            "{\"concepts\":[],\"concepts\":[],\"relations\":[],\"hyperedges\":[],\"coverage\":[]}".to_string()
                        } else {
                            serde_json::to_string(&data).unwrap()
                        };
                        json!({"model":MODEL,"done":true,"done_reason":"stop",
                            "message":{"role":"assistant","content":content},
                            "prompt_eval_count":if mode=="invalid-usage" {json!("invalid")} else {json!(10)},
                            "prompt_eval_cached_count":2,"eval_count":3})
                    }
                    _ => panic!("unrequested fixture endpoint: {path}"),
                };
                let bytes = serde_json::to_vec(&response).unwrap();
                let header = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    bytes.len()
                );
                let _ = stream.write_all(header.as_bytes());
                let _ = stream.write_all(&bytes);
            }
        });
        Self {
            spec,
            stop,
            calls,
            worker: Some(worker),
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.worker.take().unwrap().join().unwrap();
    }
}

fn request() -> Value {
    let workspace = "a".repeat(64);
    let text = "pub fn fixture() {}\n";
    let mut anchor = Binding {
        source_id: String::new(),
        path: "fixture.rs".into(),
        content_hash: hash(text),
        start_byte: 0,
        end_byte: text.len(),
    };
    anchor.source_id = meaning_graph::unit_id(&workspace, &anchor, text);
    let sources = serde_json::to_value(vec![Source {
        anchor,
        text: text.into(),
        omitted_bytes: 0,
    }])
    .unwrap();
    let payload = json!({"workspaceId":workspace,"unitSetHash":identity(&sources),
        "contractHash":"b".repeat(64),"sources":sources});
    json!({"schema":1,"operation":"extract_meaning_v1","model":MODEL,"modelVersion":"1".repeat(64),
        "requestId":"c".repeat(64),"inputHash":identity(&payload),"taskId":"owned-meaning-accounting",
        "currency":"EUR","costLimitMicros":10,"payload":payload})
}

fn assert_accounted_failure(mode: &'static str) {
    let fixture = Fixture::start(mode);
    let input = request();
    let reply = ollama::provider_reply(
        &fixture.spec,
        &serde_json::to_vec(&input).unwrap(),
        &Deadline::new(10000),
    )
    .expect("a completed meaning call must return a bound error envelope retaining reported usage");
    assert!(reply.get("data").is_none());
    assert!(!reply["error"].as_str().unwrap().is_empty());
    assert_eq!(reply["schema"], 1);
    assert_eq!(reply["operation"], "extract_meaning_v1");
    assert_eq!(reply["requestId"], "c".repeat(64));
    assert_eq!(reply["inputHash"], input["inputHash"]);
    assert_eq!(reply["model"], MODEL);
    assert_eq!(reply["modelVersion"], "1".repeat(64));
    assert_eq!(reply["currency"], "EUR");
    assert_eq!(
        reply["sourceBindings"],
        json!([input["payload"]["sources"][0]["anchor"]])
    );
    assert_eq!(
        reply["usage"],
        json!({"inputTokens":10,"outputTokens":3,"cachedInputTokens":2})
    );
    assert!(reply["actualCostMicros"].is_null());
    assert_eq!(fixture.calls.load(Ordering::Acquire), 1);
}

#[test]
fn meaning_ollama_accounting_preserves_usage_after_invalid_shape_and_duplicate_json() {
    for mode in ["invalid-meaning", "duplicate-json"] {
        assert_accounted_failure(mode);
    }
}

#[test]
fn meaning_ollama_accounting_preserves_usage_after_postcall_digest_change() {
    assert_accounted_failure("postcall-digest-change");
}

#[test]
fn meaning_ollama_accounting_keeps_invalid_usage_unknown_in_bound_error() {
    let fixture = Fixture::start("invalid-usage");
    let input = request();
    let reply = ollama::provider_reply(
        &fixture.spec,
        &serde_json::to_vec(&input).unwrap(),
        &Deadline::new(10000),
    )
    .expect("invalid reported usage must return a bound unknown-billing failure");
    assert!(reply.get("data").is_none());
    assert!(reply["error"].as_str().is_some_and(|text| !text.is_empty()));
    assert!(reply["usage"].is_null());
    assert_eq!(reply["requestId"], input["requestId"]);
    assert_eq!(reply["inputHash"], input["inputHash"]);
    assert_eq!(fixture.calls.load(Ordering::Acquire), 1);
}

#[test]
fn meaning_ollama_success_keeps_exact_no_claims_and_reported_usage() {
    let fixture = Fixture::start("ok");
    let input = request();
    let reply = ollama::provider_reply(
        &fixture.spec,
        &serde_json::to_vec(&input).unwrap(),
        &Deadline::new(10000),
    )
    .unwrap();
    assert!(reply.get("error").is_none());
    assert_eq!(
        reply["data"],
        json!({"concepts":[],"relations":[],"hyperedges":[],
        "coverage":[{"sourceId":input["payload"]["sources"][0]["anchor"]["sourceId"],"outcome":"no_claims"}]})
    );
    assert_eq!(
        reply["usage"],
        json!({"inputTokens":10,"outputTokens":3,"cachedInputTokens":2})
    );
    assert_eq!(fixture.calls.load(Ordering::Acquire), 1);
}
