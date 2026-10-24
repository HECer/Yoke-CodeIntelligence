//! Host-selected, authenticated loopback Explorer. Captured source is never executed.
use crate::{
    Engine,
    capture::Deadline,
    export::{self, SourceExplorerOptions},
    model::valid_id,
};
use anyhow::{Result, ensure};
use serde::Deserialize;
use serde_json::json;
use std::{
    collections::BTreeMap,
    io::{self, Read, Write},
    net::{Ipv4Addr, Shutdown, TcpListener, TcpStream},
};
use subtle::ConstantTimeEq;

pub struct Config {
    pub port: u16,
    pub request_timeout_ms: u64,
    pub max_requests: Option<usize>,
    pub allow_rebuild: bool,
    pub snapshot_id: Option<String>,
    pub export_options: SourceExplorerOptions,
}
struct View {
    snapshot: String,
    revision: u64,
    html: Vec<u8>,
}
struct Failure(u16, &'static str);
type HttpResult<T> = std::result::Result<T, Failure>;
impl From<io::Error> for Failure {
    fn from(error: io::Error) -> Self {
        if matches!(
            error.kind(),
            io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
        ) {
            Self(408, "Request deadline exceeded")
        } else {
            Self(400, "Incomplete or invalid HTTP request")
        }
    }
}
fn remaining(deadline: &Deadline) -> HttpResult<std::time::Duration> {
    deadline
        .check()
        .map_err(|_| Failure(408, "Request deadline exceeded"))?;
    let left = deadline.remaining();
    if left.is_zero() {
        return Err(Failure(408, "Request deadline exceeded"));
    }
    Ok(left)
}
fn read_exact(stream: &mut TcpStream, bytes: &mut [u8], deadline: &Deadline) -> HttpResult<()> {
    let mut at = 0;
    while at < bytes.len() {
        stream.set_read_timeout(Some(remaining(deadline)?))?;
        let n = stream.read(&mut bytes[at..])?;
        if n == 0 {
            return Err(Failure(400, "Truncated HTTP request"));
        }
        at += n;
    }
    Ok(())
}
enum Request {
    Bootstrap,
    View,
    Rebuild(Vec<u8>),
}
fn request(
    stream: &mut TcpStream,
    host: &str,
    token: &str,
    deadline: &Deadline,
) -> HttpResult<Request> {
    let mut bytes = Vec::with_capacity(1024);
    while !bytes.ends_with(b"\r\n\r\n") {
        if bytes.len() >= 16 * 1024 {
            return Err(Failure(431, "HTTP header budget exceeded"));
        }
        let mut byte = [0];
        read_exact(stream, &mut byte, deadline)?;
        bytes.push(byte[0]);
        if byte[0] == b'\n' && (bytes.len() < 2 || bytes[bytes.len() - 2] != b'\r') {
            return Err(Failure(400, "HTTP requires CRLF framing"));
        }
    }
    let mut fields = [httparse::EMPTY_HEADER; 64];
    let mut parsed = httparse::Request::new(&mut fields);
    if !matches!(parsed.parse(&bytes),Ok(httparse::Status::Complete(end)) if end==bytes.len())
        || parsed.version != Some(1)
    {
        return Err(Failure(400, "Invalid HTTP/1.1 request"));
    }
    let mut headers = BTreeMap::new();
    for header in parsed.headers {
        let value =
            std::str::from_utf8(header.value).map_err(|_| Failure(400, "Invalid header value"))?;
        if headers
            .insert(header.name.to_ascii_lowercase(), value)
            .is_some()
        {
            return Err(Failure(400, "Duplicate HTTP header"));
        }
    }
    if headers.get("host").copied() != Some(host)
        || headers
            .get("origin")
            .is_some_and(|origin| **origin != format!("http://{host}"))
    {
        return Err(Failure(403, "Host or Origin denied"));
    }
    let bootstrap = parsed.method == Some("GET") && parsed.path == Some("/");
    if !bootstrap {
        let expected = format!("Bearer {token}");
        let supplied = headers.get("authorization").copied().unwrap_or("");
        if !bool::from(supplied.as_bytes().ct_eq(expected.as_bytes())) {
            return Err(Failure(401, "Authorization required"));
        }
    }
    if headers.contains_key("transfer-encoding") || headers.contains_key("expect") {
        return Err(Failure(400, "Transfer encoding and Expect are unsupported"));
    }
    let length = match headers.get("content-length") {
        Some(value) if !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()) => value
            .parse::<usize>()
            .map_err(|_| Failure(413, "Request body budget exceeded"))?,
        Some(_) => return Err(Failure(400, "Invalid Content-Length")),
        None if parsed.method == Some("GET") => 0,
        None => return Err(Failure(411, "Content-Length required")),
    };
    if length > 1024 {
        return Err(Failure(413, "Request body budget exceeded"));
    }
    let result = match (parsed.method, parsed.path) {
        (Some("GET"), Some("/")) if length == 0 => Request::Bootstrap,
        (Some("GET"), Some("/api/view")) if length == 0 => Request::View,
        (Some("POST"), Some("/api/rebuild")) => {
            if headers
                .get("content-type")
                .map(|v| v.split(';').next().unwrap_or("").trim())
                != Some("application/json")
            {
                return Err(Failure(415, "JSON content type required"));
            }
            let mut body = vec![0; length];
            read_exact(stream, &mut body, deadline)?;
            Request::Rebuild(body)
        }
        (Some("GET"), _) if length > 0 => return Err(Failure(400, "GET body refused")),
        _ => return Err(Failure(404, "Endpoint not found")),
    };
    stream.set_nonblocking(true)?;
    let extra = stream.peek(&mut [0]);
    stream.set_nonblocking(false)?;
    if extra.is_ok_and(|n| n > 0) {
        return Err(Failure(400, "HTTP pipelining refused"));
    }
    remaining(deadline)?;
    Ok(result)
}
fn respond(
    stream: &mut TcpStream,
    status: u16,
    body: &[u8],
    media: &str,
    view: Option<&View>,
    allow_rebuild: bool,
    deadline: &Deadline,
) -> HttpResult<()> {
    let binding = view.map_or_else(String::new, |v| {
        format!(
            "X-Yoke-Snapshot: {}\r\nX-Yoke-Revision: {}\r\nX-Yoke-Rebuild: {allow_rebuild}\r\n",
            v.snapshot, v.revision
        )
    });
    let header = format!(
        "HTTP/1.1 {status} {}\r\nContent-Type: {media}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\nReferrer-Policy: no-referrer\r\nX-Content-Type-Options: nosniff\r\nX-Frame-Options: DENY\r\nContent-Security-Policy: default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; connect-src 'self'; frame-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'\r\n{binding}\r\n",
        if status == 200 {
            "OK"
        } else {
            "Request Refused"
        },
        body.len()
    );
    if status == 408 {
        stream.set_nonblocking(true)?;
        let _ = stream.write(header.as_bytes());
        let _ = stream.write(body);
        return Ok(());
    }
    for bytes in [header.as_bytes(), body] {
        let mut at = 0;
        while at < bytes.len() {
            stream.set_write_timeout(Some(remaining(deadline)?))?;
            let n = stream.write(&bytes[at..])?;
            if n == 0 {
                return Err(Failure(400, "Client stopped reading"));
            }
            at += n;
        }
    }
    Ok(())
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Rebuild {
    expected_snapshot_id: String,
    expected_revision: u64,
}
fn rebuild(
    engine: &mut Engine,
    view: &mut View,
    body: &[u8],
    config: &Config,
    deadline: &Deadline,
) -> HttpResult<Vec<u8>> {
    if !config.allow_rebuild {
        return Err(Failure(403, "Host did not grant rebuild permission"));
    }
    let input: Rebuild =
        serde_json::from_slice(body).map_err(|_| Failure(400, "Invalid rebuild request"))?;
    if !valid_id(&input.expected_snapshot_id) {
        return Err(Failure(400, "Invalid expected snapshot"));
    }
    if input.expected_snapshot_id != view.snapshot || input.expected_revision != view.revision {
        return Err(Failure(
            409,
            "View changed; reload the saved view before rebuilding",
        ));
    }
    let revision = view
        .revision
        .checked_add(1)
        .ok_or(Failure(409, "Revision exhausted"))?;
    let (snapshot, graph, _, _) = engine.index(deadline).map_err(|_| {
        Failure(
            400,
            "Capture failed; previous view retained. A capture may have been recorded",
        )
    })?;
    let html = export::serialize_source_explorer(
        &engine.store,
        &snapshot,
        &graph,
        &config.export_options,
        deadline,
    )
    .map_err(|_| {
        Failure(
            400,
            "Export failed; previous view retained. A capture may have been recorded",
        )
    })?;
    remaining(deadline)?;
    let changed = snapshot.id != view.snapshot;
    *view = View {
        snapshot: snapshot.id,
        revision,
        html,
    };
    serde_json::to_vec(&json!({"snapshotId":view.snapshot,"revision":revision,"changed":changed}))
        .map_err(|_| Failure(500, "Cannot serialize rebuild result"))
}
pub fn serve(engine: &mut Engine, config: Config, mut output: impl Write) -> Result<()> {
    ensure!(
        (10..=600000).contains(&config.request_timeout_ms),
        "Explorer timeout must be 10..600000 milliseconds"
    );
    ensure!(
        config
            .max_requests
            .is_none_or(|n| (1..=100000).contains(&n)),
        "Explorer request count must be 1..100000"
    );
    ensure!(
        config.export_options.max_source_bytes <= 32000000
            && (1024..=128000000).contains(&config.export_options.max_output_bytes),
        "Invalid Explorer export limits"
    );
    ensure!(
        config.snapshot_id.as_ref().is_none_or(|id| valid_id(id)),
        "Invalid initial Explorer snapshot"
    );
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, config.port))?;
    let host = format!("127.0.0.1:{}", listener.local_addr()?.port());
    let prior_writes = engine.allow_writes;
    engine.allow_writes = false;
    let result = (|| -> Result<()> {
        let deadline = Deadline::new(config.request_timeout_ms);
        let (snapshot, graph) = engine.select(
            &config
                .snapshot_id
                .as_ref()
                .map_or_else(|| json!({}), |id| json!({"snapshotId":id})),
            &deadline,
        )?;
        let mut view = View {
            snapshot: snapshot.id.clone(),
            revision: 0,
            html: export::serialize_source_explorer(
                &engine.store,
                &snapshot,
                &graph,
                &config.export_options,
                &deadline,
            )?,
        };
        engine.store.clear_deadline();
        let mut bytes = [0; 32];
        getrandom::fill(&mut bytes)
            .map_err(|_| anyhow::anyhow!("System randomness unavailable"))?;
        let token = hex::encode(bytes);
        writeln!(
            output,
            "{}",
            json!({"url":format!("http://{host}/#{token}"),"snapshotId":view.snapshot,"revision":0,"allowRebuild":config.allow_rebuild})
        )?;
        output.flush()?;
        let mut count = 0;
        while config.max_requests.is_none_or(|max| count < max) {
            let (mut stream, peer) = listener.accept()?;
            count += 1;
            if !peer.ip().is_loopback() {
                continue;
            }
            let deadline = Deadline::new(config.request_timeout_ms);
            // Browsers may open an idle speculative socket before the actual
            // fetch. Header/body admission has its own short bound, independent
            // of the much longer source-capture and response deadline.
            let admission_deadline = Deadline::new(config.request_timeout_ms.min(1000));
            let response = match request(&mut stream, &host, &token, &admission_deadline) {
                Ok(Request::Bootstrap) => {
                    let _ = respond(
                        &mut stream,
                        200,
                        include_bytes!("explorer_live.html"),
                        "text/html; charset=utf-8",
                        None,
                        config.allow_rebuild,
                        &deadline,
                    );
                    Ok(())
                }
                Ok(Request::View) => {
                    let _ = respond(
                        &mut stream,
                        200,
                        &view.html,
                        "text/html; charset=utf-8",
                        Some(&view),
                        config.allow_rebuild,
                        &deadline,
                    );
                    Ok(())
                }
                Ok(Request::Rebuild(body)) => {
                    match rebuild(engine, &mut view, &body, &config, &deadline) {
                        Ok(bytes) => {
                            let _ = respond(
                                &mut stream,
                                200,
                                &bytes,
                                "application/json",
                                None,
                                config.allow_rebuild,
                                &deadline,
                            );
                            Ok(())
                        }
                        Err(failure) => Err(failure),
                    }
                }
                Err(failure) => Err(failure),
            };
            engine.store.clear_deadline();
            if let Err(Failure(status, message)) = response {
                let body = serde_json::to_vec(&json!({"error":message}))?;
                let _ = respond(
                    &mut stream,
                    status,
                    &body,
                    "application/json",
                    None,
                    config.allow_rebuild,
                    &deadline,
                );
            }
            let _ = stream.shutdown(Shutdown::Write);
            let _ = stream.set_nonblocking(true);
            let mut chunk = [0; 4096];
            let mut discarded = 0;
            while discarded < 65536 && deadline.check().is_ok() {
                match stream.read(&mut chunk) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => discarded += n,
                }
            }
        }
        Ok(())
    })();
    engine.store.clear_deadline();
    engine.allow_writes = prior_writes;
    result
}
