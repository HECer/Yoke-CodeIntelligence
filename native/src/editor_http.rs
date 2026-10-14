//! Authenticated loopback transport for immutable editor overlays.
use crate::{Engine, capture::Deadline, editor};
use anyhow::{Result, ensure};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::{self, Read, Write},
    net::{Ipv4Addr, Shutdown, TcpListener, TcpStream},
};
use subtle::ConstantTimeEq;

const HEADER_LIMIT: usize = 16 * 1024;
// A four-million-byte buffer containing only escaped control characters fits,
// including the surrounding editor JSON fields.
const BODY_LIMIT: usize = 24 * 1024 * 1024;
const RESPONSE_LIMIT: usize = 32 * 1024 * 1024;

pub struct Config {
    pub port: u16,
    pub request_timeout_ms: u64,
    pub max_requests: Option<usize>,
}
pub fn serve(engine: &mut Engine, config: Config, mut output: impl Write) -> Result<()> {
    ensure!(
        (10..=600_000).contains(&config.request_timeout_ms),
        "Editor request timeout must be between 10 and 600000 milliseconds"
    );
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, config.port))?;
    let host = format!("127.0.0.1:{}", listener.local_addr()?.port());
    let mut random = [0u8; 32];
    getrandom::fill(&mut random).map_err(|_| anyhow::anyhow!("System randomness unavailable"))?;
    let token = hex::encode(random);
    writeln!(
        output,
        "{}",
        json!({"endpoint":format!("http://{host}/editor"),"token":token})
    )?;
    output.flush()?;
    let prior_writes = engine.allow_writes;
    engine.allow_writes = false;
    let result = (|| -> Result<()> {
        let mut accepted = 0usize;
        while config.max_requests.is_none_or(|max| accepted < max) {
            let (mut stream, peer) = listener.accept()?;
            accepted += 1;
            if !peer.ip().is_loopback() {
                continue;
            }
            let deadline = Deadline::new(config.request_timeout_ms);
            let response = handle(engine, &mut stream, &host, &token, &deadline);
            engine.store.clear_deadline();
            let response = match response {
                Ok(data) => Response {
                    status: 200,
                    body: json!({"ok":true,"data":data}),
                },
                Err(error) => Response {
                    status: error.status,
                    body: json!({"ok":false,"error":{"code":error.code,"message":redacted(&error.message,&token)}}),
                },
            };
            // Broken or stalled clients do not terminate the listener.
            let _ = respond(&mut stream, response, &deadline);
            let _ = stream.shutdown(Shutdown::Write);
            discard_pending(&mut stream, &deadline);
        }
        Ok(())
    })();
    engine.allow_writes = prior_writes;
    result
}

fn discard_pending(stream: &mut TcpStream, deadline: &Deadline) {
    // Closing a Windows socket with unread rejected bytes can reset the peer
    // before it receives the error response. Discard only already-available
    // bytes after responding; never wait for, allocate, parse or accept a body.
    if stream.set_nonblocking(true).is_err() {
        return;
    }
    let mut chunk = [0u8; 4096];
    let mut total = 0usize;
    while total < 64 * 1024 && deadline.check().is_ok() {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(count) => total += count,
        }
    }
}

struct Failure {
    status: u16,
    code: &'static str,
    message: String,
}
type HttpResult<T> = std::result::Result<T, Failure>;
impl Failure {
    fn new(status: u16, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
        }
    }
    fn timeout() -> Self {
        Self::new(408, "DEADLINE_EXCEEDED", "Editor request deadline exceeded")
    }
    fn io(error: io::Error) -> Self {
        if matches!(
            error.kind(),
            io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
        ) {
            Self::timeout()
        } else {
            Self::new(400, "INVALID_REQUEST", "Incomplete or invalid HTTP request")
        }
    }
    fn operation(error: anyhow::Error) -> Self {
        let message = error.to_string();
        if message.contains("deadline") || message.contains("interrupted") {
            return Self::timeout();
        }
        let conflict = [
            "stale",
            "not newer",
            "newer editor buffer",
            "changed concurrently",
            "Disk changed independently",
        ]
        .iter()
        .any(|part| message.contains(part));
        Self::new(
            if conflict { 409 } else { 400 },
            if conflict {
                "EDITOR_CONFLICT"
            } else {
                "INVALID_REQUEST"
            },
            message,
        )
    }
}
fn redacted(message: &str, token: &str) -> String {
    message
        .replace(token, "[redacted]")
        .chars()
        .filter(|c| !c.is_control())
        .take(300)
        .collect()
}
fn remaining(deadline: &Deadline) -> HttpResult<std::time::Duration> {
    deadline.check().map_err(|_| Failure::timeout())?;
    let left = deadline.remaining();
    if left.is_zero() {
        return Err(Failure::timeout());
    }
    Ok(left)
}
fn read_exact(stream: &mut TcpStream, bytes: &mut [u8], deadline: &Deadline) -> HttpResult<()> {
    let mut at = 0;
    while at < bytes.len() {
        stream
            .set_read_timeout(Some(remaining(deadline)?))
            .map_err(Failure::io)?;
        let count = stream.read(&mut bytes[at..]).map_err(Failure::io)?;
        if count == 0 {
            return Err(Failure::new(
                400,
                "INVALID_REQUEST",
                "Truncated HTTP request",
            ));
        }
        at += count;
    }
    remaining(deadline)?;
    Ok(())
}
fn headers(stream: &mut TcpStream, deadline: &Deadline) -> HttpResult<Vec<u8>> {
    let mut bytes = Vec::with_capacity(1024);
    let mut byte = [0u8; 1];
    loop {
        if bytes.len() >= HEADER_LIMIT {
            return Err(Failure::new(
                431,
                "HEADERS_TOO_LARGE",
                "HTTP headers exceed the limit",
            ));
        }
        // Stop exactly at the header boundary: unauthenticated body bytes are
        // never read, allocated, parsed or passed to an editor operation.
        read_exact(stream, &mut byte, deadline)?;
        bytes.push(byte[0]);
        if byte[0] == b'\n' && (bytes.len() < 2 || bytes[bytes.len() - 2] != b'\r') {
            return Err(Failure::new(
                400,
                "INVALID_REQUEST",
                "HTTP headers require CRLF framing",
            ));
        }
        if bytes.ends_with(b"\r\n\r\n") {
            return Ok(bytes);
        }
    }
}
fn validate_headers(bytes: &[u8], host: &str, token: &str) -> HttpResult<usize> {
    let mut fields = [httparse::EMPTY_HEADER; 64];
    let mut request = httparse::Request::new(&mut fields);
    match request.parse(bytes) {
        Ok(httparse::Status::Complete(end)) if end == bytes.len() => {}
        _ => {
            return Err(Failure::new(
                400,
                "INVALID_REQUEST",
                "Malformed HTTP request headers",
            ));
        }
    }
    let mut headers = BTreeMap::new();
    for header in request.headers {
        let name = header.name.to_ascii_lowercase();
        let value = std::str::from_utf8(header.value)
            .map_err(|_| Failure::new(400, "INVALID_REQUEST", "Header value is not UTF-8"))?;
        if headers.insert(name, value).is_some() {
            return Err(Failure::new(
                400,
                "INVALID_REQUEST",
                "Duplicate HTTP request header",
            ));
        }
    }
    if headers.get("host").copied() != Some(host)
        || headers
            .get("origin")
            .is_some_and(|origin| **origin != format!("http://{host}"))
    {
        return Err(Failure::new(
            403,
            "ORIGIN_DENIED",
            "Invalid editor origin or host",
        ));
    }
    let expected = format!("Bearer {token}");
    let supplied = headers.get("authorization").copied().unwrap_or("");
    if !bool::from(supplied.as_bytes().ct_eq(expected.as_bytes())) {
        return Err(Failure::new(
            401,
            "AUTHORIZATION_REQUIRED",
            "Editor authorization required",
        ));
    }
    if request.method != Some("POST") || request.path != Some("/editor") {
        return Err(Failure::new(404, "NOT_FOUND", "Editor endpoint not found"));
    }
    if request.version != Some(1) {
        return Err(Failure::new(400, "INVALID_REQUEST", "HTTP/1.1 is required"));
    }
    if headers.contains_key("transfer-encoding") || headers.contains_key("expect") {
        return Err(Failure::new(
            400,
            "INVALID_REQUEST",
            "Transfer encoding and Expect are unsupported",
        ));
    }
    let Some(length) = headers.get("content-length") else {
        return Err(Failure::new(
            411,
            "LENGTH_REQUIRED",
            "Content-Length is required",
        ));
    };
    if length.is_empty() || !length.bytes().all(|b| b.is_ascii_digit()) {
        return Err(Failure::new(
            400,
            "INVALID_REQUEST",
            "Invalid Content-Length",
        ));
    }
    let length = length
        .parse::<usize>()
        .map_err(|_| Failure::new(413, "REQUEST_TOO_LARGE", "Editor request is too large"))?;
    if length > BODY_LIMIT {
        return Err(Failure::new(
            413,
            "REQUEST_TOO_LARGE",
            "Editor request is too large",
        ));
    }
    let media = headers
        .get("content-type")
        .map(|value| value.split(';').next().unwrap_or("").trim());
    if media != Some("application/json") {
        return Err(Failure::new(
            415,
            "UNSUPPORTED_MEDIA_TYPE",
            "Editor requests require application/json",
        ));
    }
    Ok(length)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BufferInput {
    path: String,
    version: u32,
    text: String,
    expected_version: Option<u32>,
}
#[derive(Deserialize)]
#[serde(
    tag = "operation",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
enum Request {
    Open {},
    Buffer {
        session_id: String,
        buffer: BufferInput,
    },
    CloseFile {
        session_id: String,
        path: String,
        version: Option<u32>,
    },
    Close {
        session_id: String,
    },
    Status {
        session_id: String,
    },
    Capture {
        session_id: String,
    },
    Tool {
        session_id: String,
        tool: String,
        #[serde(default = "empty_arguments")]
        arguments: Value,
    },
}
fn empty_arguments() -> Value {
    json!({})
}
fn handle(
    engine: &mut Engine,
    stream: &mut TcpStream,
    host: &str,
    token: &str,
    deadline: &Deadline,
) -> HttpResult<Value> {
    let header = headers(stream, deadline)?;
    let length = validate_headers(&header, host, token)?;
    let mut body = vec![0u8; length];
    read_exact(stream, &mut body, deadline)?;
    // A second request already on this connection is rejected. No second
    // request is ever dispatched, including one arriving after this check.
    stream.set_nonblocking(true).map_err(Failure::io)?;
    let extra = stream.peek(&mut [0u8; 1]);
    stream.set_nonblocking(false).map_err(Failure::io)?;
    if extra.is_ok_and(|count| count > 0) {
        return Err(Failure::new(
            400,
            "INVALID_REQUEST",
            "HTTP pipelining is unsupported",
        ));
    }
    let request: Request = serde_json::from_slice(&body)
        .map_err(|_| Failure::new(400, "INVALID_REQUEST", "Invalid editor JSON operation"))?;
    remaining(deadline)?;
    engine
        .store
        .set_deadline(deadline)
        .map_err(Failure::operation)?;
    let data=match request {
        Request::Open {}=>serde_json::to_value(editor::open(engine,deadline).map_err(Failure::operation)?).map_err(|_|Failure::new(500,"SERIALIZATION_FAILED","Cannot serialize editor session"))?,
        Request::Buffer{session_id,buffer}=>editor::update(engine,&session_id,&json!({"path":buffer.path,"version":buffer.version,"text":buffer.text,"expectedVersion":buffer.expected_version}),deadline).map_err(Failure::operation)?,
        Request::CloseFile{session_id,path,version}=>{editor::close_file(&engine.store,&session_id,&path,version,deadline).map_err(Failure::operation)?;json!({"closed":true})},
        Request::Close{session_id}=>{editor::close(&engine.store,&session_id,deadline).map_err(Failure::operation)?;json!({"closed":true})},
        Request::Status{session_id}=>serde_json::to_value(editor::session(&engine.store,&session_id).map_err(Failure::operation)?).map_err(|_|Failure::new(500,"SERIALIZATION_FAILED","Cannot serialize editor session"))?,
        Request::Capture{session_id}=>serde_json::to_value(editor::capture(engine,&session_id,deadline).map_err(Failure::operation)?).map_err(|_|Failure::new(500,"SERIALIZATION_FAILED","Cannot serialize editor snapshot"))?,
        Request::Tool{session_id,tool,mut arguments}=>{
            if !["ci_status","ci_find","ci_read","ci_trace","ci_context"].contains(&tool.as_str()) {return Err(Failure::new(403,"CAPABILITY_UNAVAILABLE","Editor API exposes read-only agent tools"));}
            crate::validate(&tool,&arguments).map_err(Failure::operation)?;
            if tool=="ci_status" && arguments["mode"]=="index" {return Err(Failure::new(403,"CAPABILITY_UNAVAILABLE","Use capture for source-bound editor indexing"));}
            let snapshot=editor::capture(engine,&session_id,deadline).map_err(Failure::operation)?;
            let left=remaining(deadline)?.as_millis().min(600_000) as u64;
            if left<10 {return Err(Failure::timeout());}
            let requested=arguments["timeoutMs"].as_u64().unwrap_or(left);
            arguments["snapshotId"]=json!(snapshot.id);arguments["timeoutMs"]=json!(left.min(requested));
            engine.invoke(&tool,arguments).map_err(Failure::operation)?
        },
    };
    remaining(deadline)?;
    Ok(data)
}
struct Response {
    status: u16,
    body: Value,
}
struct ResponseWriter<'a> {
    bytes: Vec<u8>,
    deadline: &'a Deadline,
}
impl Write for ResponseWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.deadline.check().is_err() {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Response deadline exceeded",
            ));
        }
        if bytes.len() > RESPONSE_LIMIT.saturating_sub(self.bytes.len()) {
            return Err(io::Error::other("Response byte limit exceeded"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn respond(stream: &mut TcpStream, response: Response, deadline: &Deadline) -> HttpResult<()> {
    let mut status = response.status;
    let mut writer = ResponseWriter {
        bytes: Vec::new(),
        deadline,
    };
    let serialized = serde_json::to_writer(&mut writer, &response.body);
    let body = if deadline.check().is_err() {
        status = 408;
        b"{\"ok\":false,\"error\":{\"code\":\"DEADLINE_EXCEEDED\",\"message\":\"Editor request deadline exceeded\"}}".to_vec()
    } else if serialized.is_err() {
        status = 413;
        b"{\"ok\":false,\"error\":{\"code\":\"RESPONSE_TOO_LARGE\",\"message\":\"Editor response is too large\"}}".to_vec()
    } else {
        writer.bytes
    };
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        408 => "Request Timeout",
        409 => "Conflict",
        411 => "Length Required",
        413 => "Content Too Large",
        415 => "Unsupported Media Type",
        431 => "Request Header Fields Too Large",
        _ => "Internal Server Error",
    };
    let headers = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\n\r\n",
        body.len()
    );
    if status == 408 {
        // Once expired, attempt one nonblocking timeout response rather than
        // extending the budget to wait for a stalled reader.
        stream.set_nonblocking(true).map_err(Failure::io)?;
        let mut bytes = headers.into_bytes();
        bytes.extend(body);
        let _ = stream.write(&bytes);
        return Ok(());
    }
    for bytes in [headers.as_bytes(), body.as_slice()] {
        let mut at = 0;
        while at < bytes.len() {
            stream
                .set_write_timeout(Some(remaining(deadline)?))
                .map_err(Failure::io)?;
            let written = stream.write(&bytes[at..]).map_err(Failure::io)?;
            if written == 0 {
                return Err(Failure::new(
                    400,
                    "INVALID_REQUEST",
                    "Client stopped reading editor response",
                ));
            }
            at += written;
        }
    }
    remaining(deadline)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{thread, time::Duration};

    #[cfg(windows)]
    fn socket_buffer(stream: &TcpStream, send: bool) {
        use std::os::windows::io::AsRawSocket;
        #[link(name = "Ws2_32")]
        unsafe extern "system" {
            fn setsockopt(
                socket: usize,
                level: i32,
                option: i32,
                value: *const i8,
                length: i32,
            ) -> i32;
        }
        let size = 8192i32;
        // These sockets are owned by this test; both kernel buffers are bounded
        // so ordinary Windows loopback autotuning cannot absorb the entire body.
        let result = unsafe {
            setsockopt(
                stream.as_raw_socket() as usize,
                0xffff,
                if send { 0x1001 } else { 0x1002 },
                (&size as *const i32).cast(),
                4,
            )
        };
        assert_eq!(result, 0, "Cannot constrain test socket buffer");
    }
    #[cfg(unix)]
    fn socket_buffer(stream: &TcpStream, send: bool) {
        use std::os::fd::AsRawFd;
        unsafe extern "C" {
            fn setsockopt(
                socket: i32,
                level: i32,
                option: i32,
                value: *const std::ffi::c_void,
                length: u32,
            ) -> i32;
        }
        #[cfg(target_os = "linux")]
        let (level, option) = (1, if send { 7 } else { 8 });
        #[cfg(not(target_os = "linux"))]
        let (level, option) = (0xffff, if send { 0x1001 } else { 0x1002 });
        let size = 8192i32;
        let result = unsafe {
            setsockopt(
                stream.as_raw_fd(),
                level,
                option,
                (&size as *const i32).cast(),
                4,
            )
        };
        assert_eq!(result, 0, "Cannot constrain test socket buffer");
    }

    #[test]
    fn backpressured_socket_writes_cannot_extend_the_response_deadline() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let (mut stream, _) = listener.accept().unwrap();
        socket_buffer(&client, false);
        socket_buffer(&stream, true);
        // Observe WouldBlock on an actual bounded outbound queue. Receive/send
        // buffer settings alone do not reliably stop Windows loopback buffering.
        stream.set_nonblocking(true).unwrap();
        let filler = [b'x'; 65536];
        let mut queued = 0usize;
        let mut blocked = false;
        while queued < 64 * 1024 * 1024 {
            match stream.write(&filler) {
                Ok(0) => break,
                Ok(count) => queued += count,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    blocked = true;
                    break;
                }
                Err(_) => break,
            }
        }
        assert!(
            blocked,
            "Fixture must fill the real socket's outbound queue"
        );
        stream.set_nonblocking(false).unwrap();
        let body = json!({"ok":true,"data":"pending response"});
        let (sender, receiver) = std::sync::mpsc::channel();
        let task = thread::spawn(move || {
            let deadline = Deadline::new(250);
            let started = std::time::Instant::now();
            let result = respond(&mut stream, Response { status: 200, body }, &deadline);
            let timeout = result.is_err_and(|error| error.status == 408);
            let _ = sender.send((timeout, started.elapsed()));
        });
        let result = receiver.recv_timeout(Duration::from_secs(3));
        if result.is_err() {
            let _ = client.shutdown(Shutdown::Both);
        }
        task.join().unwrap();
        let (timeout, elapsed) = result.expect("Writer must stop at its deadline");
        assert!(
            timeout,
            "Fixture must reach genuinely backpressured response writing"
        );
        assert!(elapsed < Duration::from_secs(3));
    }

    #[test]
    fn an_expired_response_never_claims_the_operation_succeeded() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let (mut stream, _) = listener.accept().unwrap();
        let deadline = Deadline::new(1);
        thread::sleep(Duration::from_millis(5));
        assert!(
            respond(
                &mut stream,
                Response {
                    status: 200,
                    body: json!({"ok":true,"data":{}})
                },
                &deadline
            )
            .is_ok()
        );
        drop(stream);
        let mut response = String::new();
        client.read_to_string(&mut response).unwrap();
        assert!(response.starts_with("HTTP/1.1 408 "));
        assert!(!response.contains("\"ok\":true"));
    }
}
