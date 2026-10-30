//! One bounded HTTP/1.1 request; authentication precedes body acquisition.
use super::*;
use crate::github_app::ingress::socket::{self, Channel};
use std::{
    io::{Read, Write},
    net::TcpStream,
};
const HEADERS: usize = 16_384;
fn read(io: &mut Channel<'_>, bytes: &mut [u8], d: &Deadline) -> Result<usize> {
    loop {
        d.check()?;
        match io.read(bytes) {
            Ok(n) => return Ok(n),
            Err(e) if socket::retry(&e) => {}
            Err(_) => anyhow::bail!(INVALID),
        }
    }
}
fn write(io: &mut Channel<'_>, mut bytes: &[u8], d: &Deadline) -> Result<()> {
    while !bytes.is_empty() {
        d.check()?;
        match io.write(bytes) {
            Ok(0) => anyhow::bail!(INVALID),
            Ok(n) => bytes = &bytes[n..],
            Err(e) if socket::retry(&e) => {}
            Err(_) => anyhow::bail!(INVALID),
        }
    }
    loop {
        d.check()?;
        match io.flush() {
            Ok(()) => return Ok(()),
            Err(e) if socket::retry(&e) => {}
            Err(_) => anyhow::bail!(INVALID),
        }
    }
}
pub(super) fn packet(status: u16, value: &Value, cap: usize, d: &Deadline) -> Result<Vec<u8>> {
    let mut body = crate::json::encode_bounded(value, cap, d)?;
    body.push(b'\n');
    let mut bytes=format!("HTTP/1.1 {status} Yoke\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\n\r\n",body.len()).into_bytes();
    ensure!(bytes.len().saturating_add(body.len()) <= cap, CAPACITY);
    bytes.extend(body);
    Ok(bytes)
}
fn receive(
    io: &mut Channel<'_>,
    store: &Store,
    shared: &Shared,
    d: &Deadline,
) -> Result<(u16, Value)> {
    let mut raw = Zeroizing::new(Vec::new());
    let mut byte = [0_u8; 1];
    while !raw.ends_with(b"\r\n\r\n") {
        ensure!(raw.len() < HEADERS, CAPACITY);
        ensure!(read(io, &mut byte, d)? == 1, INVALID);
        raw.push(byte[0]);
    }
    ensure!(
        !raw.iter()
            .enumerate()
            .any(|(i, b)| (*b == b'\n' && (i == 0 || raw[i - 1] != b'\r'))
                || (*b == b'\r' && raw.get(i + 1) != Some(&b'\n'))),
        INVALID
    );
    let mut entries = [httparse::EMPTY_HEADER; 48];
    let mut parsed = httparse::Request::new(&mut entries);
    ensure!(
        matches!(parsed.parse(&raw),Ok(httparse::Status::Complete(n)) if n==raw.len())
            && parsed.version == Some(1),
        INVALID
    );
    let method = parsed.method.ok_or_else(|| anyhow::anyhow!(INVALID))?;
    ensure!(matches!(method, "PUT" | "GET"), INVALID);
    let id = parsed
        .path
        .and_then(|p| p.strip_prefix("/v1/digests/"))
        .filter(|id| valid_id(id))
        .ok_or_else(|| anyhow::anyhow!(INVALID))?;
    let mut fields = BTreeMap::new();
    for h in parsed.headers.iter() {
        ensure!(
            h.value
                .iter()
                .all(|b| b.is_ascii() && *b >= 32 && *b != 127),
            INVALID
        );
        let value = std::str::from_utf8(h.value)
            .map_err(|_| anyhow::anyhow!(INVALID))?
            .trim_matches(' ');
        ensure!(
            fields
                .insert(
                    h.name.to_ascii_lowercase(),
                    Zeroizing::new(value.to_owned())
                )
                .is_none(),
            INVALID
        );
    }
    let field = |name: &str| {
        fields
            .get(name)
            .map(|v: &Zeroizing<String>| v.as_str())
            .ok_or_else(|| anyhow::anyhow!(INVALID))
    };
    ensure!(
        ![
            "transfer-encoding",
            "content-encoding",
            "expect",
            "trailer",
            "origin"
        ]
        .iter()
        .any(|h| fields.contains_key(*h)),
        INVALID
    );
    let origin = shared.profile.origin()?;
    let authority = &origin[url::Position::BeforeHost..url::Position::AfterPort];
    ensure!(field("host")?.eq_ignore_ascii_case(authority), INVALID);
    if shared.tls.is_some() {
        ensure!(io.server_name() == origin.domain(), INVALID);
    }
    let authorization = fields
        .get("authorization")
        .map(|s| s.as_str())
        .ok_or_else(|| anyhow::anyhow!("github_app_digest_receiver_auth_required"))?;
    let token = authorization
        .strip_prefix("Bearer ")
        .filter(|v| valid_id(v))
        .ok_or_else(|| anyhow::anyhow!("github_app_digest_receiver_auth_required"))?;
    let token = Zeroizing::new(hex::decode(token).map_err(|_| anyhow::anyhow!(INVALID))?);
    let token_hash = hash(token.as_slice());
    let matches = shared
        .profile
        .grants
        .iter()
        .filter(|g| bool::from(g.token_hash.as_bytes().ct_eq(token_hash.as_bytes())))
        .collect::<Vec<_>>();
    ensure!(
        !matches.is_empty(),
        "github_app_digest_receiver_auth_required"
    );
    {
        let revoked = shared
            .revoked
            .read()
            .map_err(|_| anyhow::anyhow!(BINDING))?;
        ensure!(
            matches.iter().any(|a| !revoked.contains(&a.id)),
            "github_app_digest_receiver_auth_required"
        );
    }
    let length = fields
        .get("content-length")
        .map(|s| s.as_str())
        .unwrap_or("0");
    ensure!(
        !length.is_empty()
            && length.len() <= 10
            && length.bytes().all(|b| b.is_ascii_digit())
            && (length == "0" || !length.starts_with('0')),
        INVALID
    );
    let length = length
        .parse::<usize>()
        .map_err(|_| anyhow::anyhow!(CAPACITY))?;
    if method == "GET" {
        ensure!(
            length == 0 && !fields.contains_key("idempotency-key"),
            INVALID
        );
        let revoked = shared
            .revoked
            .read()
            .map_err(|_| anyhow::anyhow!(BINDING))?;
        let allowed = matches
            .into_iter()
            .filter(|a| !revoked.contains(&a.id))
            .collect::<Vec<_>>();
        ensure!(
            !allowed.is_empty(),
            "github_app_digest_receiver_auth_required"
        );
        return Ok(match storage::get(store, shared, &allowed, id, d)? {
            Some(value) => (200, value),
            None => (404, json!({"error":"digest_not_found"})),
        });
    }
    ensure!(
        length > 0 && length <= shared.profile.max_payload_bytes,
        CAPACITY
    );
    ensure!(
        field("idempotency-key")? == id
            && field("content-type")?.eq_ignore_ascii_case("application/json"),
        INVALID
    );
    let mut body = Zeroizing::new(vec![0_u8; length]);
    let mut offset = 0;
    while offset < length {
        let n = read(io, &mut body[offset..], d)?;
        ensure!(n > 0, INVALID);
        offset += n;
    }
    let submission = Submission::parse(&body, shared.profile.max_payload_bytes, d)?;
    let scope = submission.scope();
    drop(submission);
    let revoked = shared
        .revoked
        .read()
        .map_err(|_| anyhow::anyhow!(BINDING))?;
    let access = matches
        .into_iter()
        .find(|a| !revoked.contains(&a.id) && a.permits_scope(&scope))
        .ok_or_else(|| anyhow::anyhow!("github_app_digest_receiver_scope_denied"))?;
    storage::receive(store, shared, access, id, &body, d)
}
pub(super) fn serve(stream: TcpStream, store: &Store, shared: &Shared, d: &Deadline) -> Result<()> {
    let mut io = Channel::with_tls(stream, shared.tls.as_ref(), d)?;
    let (status, value) = match receive(&mut io, store, shared, d) {
        Ok(value) => value,
        Err(error) => {
            d.check()?;
            let marker = error.to_string();
            let status = if marker.contains("auth_required") {
                401
            } else if marker.contains("scope_denied") {
                403
            } else if marker.contains("capacity") {
                507
            } else if marker == INVALID || marker == BINDING {
                400
            } else {
                500
            };
            (status, json!({"error":"digest_receiver_refused"}))
        }
    };
    let bytes = packet(status, &value, shared.config.max_response_bytes, d)?;
    write(&mut io, &bytes, d)?;
    io.discard_pending(d);
    Ok(())
}
