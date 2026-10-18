//! Strict one-request framing; response byte admission precedes queue mutation.
use super::{socket::Channel, *};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    net::TcpStream,
};
use zeroize::Zeroizing;
const HEADER_LIMIT: usize = 16_384;
struct Fault(u16, &'static str);
type HttpResult<T> = std::result::Result<T, Fault>;
fn invalid() -> Fault {
    Fault(400, "github_app_http_invalid")
}
fn read(io: &mut Channel<'_>, buffer: &mut [u8], d: &Deadline) -> HttpResult<usize> {
    loop {
        d.check()
            .map_err(|_| Fault(408, "github_app_request_expired"))?;
        match io.read(buffer) {
            Ok(count) => return Ok(count),
            Err(error) if socket::retry(&error) => continue,
            Err(_) => return Err(Fault(400, "github_app_transport_refused")),
        }
    }
}
fn write(io: &mut Channel<'_>, packet: &[u8], d: &Deadline) -> HttpResult<()> {
    let mut sent = 0;
    while sent < packet.len() {
        d.check()
            .map_err(|_| Fault(408, "github_app_request_expired"))?;
        match io.write(&packet[sent..]) {
            Ok(0) => return Err(Fault(400, "github_app_transport_refused")),
            Ok(n) => sent += n,
            Err(e) if socket::retry(&e) => continue,
            Err(_) => return Err(Fault(400, "github_app_transport_refused")),
        }
    }
    loop {
        d.check()
            .map_err(|_| Fault(408, "github_app_request_expired"))?;
        match io.flush() {
            Ok(()) => return Ok(()),
            Err(e) if socket::retry(&e) => continue,
            Err(_) => return Err(Fault(400, "github_app_transport_refused")),
        }
    }
}
fn wire(status: u16, value: &Value, cap: usize, d: &Deadline) -> HttpResult<Vec<u8>> {
    let mut body =
        crate::json::encode_bounded(value, cap, d).map_err(|_| Fault(507, INGRESS_CAPACITY))?;
    body.push(b'\n');
    let mut packet=format!("HTTP/1.1 {status} Yoke\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\n\r\n",body.len()).into_bytes();
    if packet.len().saturating_add(body.len()) > cap {
        return Err(Fault(507, INGRESS_CAPACITY));
    }
    packet.extend_from_slice(&body);
    Ok(packet)
}
fn authority(name: &str, port: u16) -> String {
    if name.contains(':') {
        format!("[{name}]:{port}")
    } else {
        format!("{name}:{port}")
    }
}
fn tls_authority_allowed(host: &str, name: &str, port: u16) -> bool {
    host.eq_ignore_ascii_case(&authority(name, port))
        || (port == 443
            && host.eq_ignore_ascii_case(&if name.contains(':') {
                format!("[{name}]")
            } else {
                name.to_owned()
            }))
}
struct Delivery {
    headers: Headers,
    event: String,
    raw: Zeroizing<Vec<u8>>,
}
enum Incoming {
    Delivery(Delivery),
    Health,
    Page {
        id: String,
        token: Zeroizing<String>,
    },
}
fn receive(io: &mut Channel<'_>, shared: &Shared, d: &Deadline) -> HttpResult<Incoming> {
    let mut raw = Zeroizing::new(Vec::new());
    let mut buffer = Zeroizing::new([0; 8192]);
    let (end, length, event, headers) = loop {
        d.check()
            .map_err(|_| Fault(408, "github_app_request_expired"))?;
        let mut entries = [httparse::EMPTY_HEADER; 48];
        let mut parsed = httparse::Request::new(&mut entries);
        match parsed.parse(&raw) {
            Ok(httparse::Status::Complete(end)) => {
                if end > HEADER_LIMIT
                    || parsed.version != Some(1)
                    || raw[..end].iter().enumerate().any(|(i, b)| {
                        (*b == b'\n' && (i == 0 || raw[i - 1] != b'\r'))
                            || (*b == b'\r' && raw.get(i + 1) != Some(&b'\n'))
                    })
                {
                    return Err(invalid());
                }
                let path = parsed.path.ok_or_else(invalid)?;
                let page = shared.pages.is_some() && path.starts_with("/p/");
                let health = shared.pages.is_some() && path == "/healthz";
                if page || health {
                    if parsed.method != Some("GET") {
                        return Err(Fault(405, "github_app_method_refused"));
                    }
                } else {
                    if parsed.method != Some("POST") {
                        return Err(Fault(405, "github_app_method_refused"));
                    }
                    if path != "/github/webhook" {
                        return Err(Fault(404, "github_app_path_refused"));
                    }
                    if !shared.config.allow_webhooks {
                        return Err(Fault(403, "github_app_permission_required"));
                    }
                }
                let mut fields = BTreeMap::new();
                for entry in parsed.headers.iter() {
                    if entry
                        .value
                        .iter()
                        .any(|b| !b.is_ascii() || *b < 32 || *b == 127)
                    {
                        return Err(invalid());
                    }
                    let value = std::str::from_utf8(entry.value)
                        .map_err(|_| invalid())?
                        .trim_matches(' ');
                    if fields
                        .insert(
                            entry.name.to_ascii_lowercase(),
                            Zeroizing::new(value.to_owned()),
                        )
                        .is_some()
                    {
                        return Err(invalid());
                    }
                }
                let get = |key: &str| {
                    fields
                        .get(key)
                        .map(|s: &Zeroizing<String>| s.as_str())
                        .ok_or_else(invalid)
                };
                let host = get("host")?;
                let host_allowed = if shared.tls.is_some() {
                    shared
                        .config
                        .server_names
                        .iter()
                        .any(|name| tls_authority_allowed(host, name, shared.address.port()))
                } else {
                    host == shared.address.to_string()
                };
                if !host_allowed
                    || io.server_name().is_some_and(|name| {
                        !shared
                            .config
                            .server_names
                            .iter()
                            .any(|allowed| name.eq_ignore_ascii_case(allowed))
                    })
                {
                    return Err(Fault(403, "github_app_host_refused"));
                }
                if [
                    "transfer-encoding",
                    "content-encoding",
                    "expect",
                    "upgrade",
                    "trailer",
                ]
                .iter()
                .any(|key| fields.contains_key(*key))
                {
                    return Err(invalid());
                }
                if fields.get("connection").is_some_and(|s| {
                    !s.eq_ignore_ascii_case("close") && !s.eq_ignore_ascii_case("keep-alive")
                }) {
                    return Err(invalid());
                }
                if page || health {
                    if let Some(count) = fields.get("content-length")
                        && (count.is_empty()
                            || !count.bytes().all(|b| b.is_ascii_digit())
                            || count.parse::<usize>().ok() != Some(0))
                    {
                        return Err(invalid());
                    }
                    if raw.len() != end {
                        return Err(invalid());
                    }
                    if health {
                        d.check()
                            .map_err(|_| Fault(408, "github_app_request_expired"))?;
                        return Ok(Incoming::Health);
                    }
                    let (id, token) = path
                        .strip_prefix("/p/")
                        .and_then(|s| s.split_once("?t="))
                        .filter(|(id, token)| {
                            crate::model::valid_id(id) && crate::model::valid_id(token)
                        })
                        .ok_or(Fault(404, "github_app_page_not_found"))?;
                    d.check()
                        .map_err(|_| Fault(408, "github_app_request_expired"))?;
                    return Ok(Incoming::Page {
                        id: id.into(),
                        token: Zeroizing::new(token.into()),
                    });
                }
                if !matches!(
                    get("content-type")?.to_ascii_lowercase().as_str(),
                    "application/json"
                        | "application/json; charset=utf-8"
                        | "application/json;charset=utf-8"
                ) {
                    return Err(Fault(415, "github_app_content_type_refused"));
                }
                let count = get("content-length")?;
                if count.is_empty() || !count.bytes().all(|b| b.is_ascii_digit()) {
                    return Err(invalid());
                }
                let length = count.parse::<usize>().map_err(|_| invalid())?;
                if length > shared.options.max_payload_bytes {
                    return Err(Fault(413, "github_app_payload_capacity_exceeded"));
                }
                let event = get("x-github-event")?.to_owned();
                if !matches!(event.as_str(), "pull_request" | "ping") {
                    return Err(Fault(422, "github_app_event_unsupported"));
                }
                let headers = Headers {
                    schema_version: 1,
                    event: "pull_request".into(),
                    delivery_id: get("x-github-delivery")?.to_owned(),
                    signature256: get("x-hub-signature-256")?.to_owned(),
                };
                headers.validate().map_err(|_| invalid())?;
                break (end, length, event, headers);
            }
            Ok(httparse::Status::Partial) => {}
            Err(_) => return Err(invalid()),
        }
        let cap = (HEADER_LIMIT + 1)
            .saturating_sub(raw.len())
            .min(buffer.len());
        if cap == 0 {
            return Err(Fault(431, "github_app_header_capacity_exceeded"));
        }
        let count = read(io, &mut buffer[..cap], d)?;
        if count == 0 {
            return Err(invalid());
        }
        raw.extend_from_slice(&buffer[..count]);
    };
    let mut body = Zeroizing::new(raw.split_off(end));
    if body.len() > length {
        return Err(invalid());
    }
    while body.len() < length {
        let cap = (length - body.len() + 1).min(buffer.len());
        let count = read(io, &mut buffer[..cap], d)?;
        if count == 0 || body.len().saturating_add(count) > length {
            return Err(invalid());
        }
        body.extend_from_slice(&buffer[..count]);
    }
    d.check()
        .map_err(|_| Fault(408, "github_app_request_expired"))?;
    Ok(Incoming::Delivery(Delivery {
        headers,
        event,
        raw: body,
    }))
}
fn ping(raw: &[u8], cap: usize) -> HttpResult<()> {
    let value = crate::json::parse(raw, cap).map_err(|_| invalid())?;
    let object = value.as_object().ok_or_else(invalid)?;
    if !object.keys().all(|key| {
        matches!(
            key.as_str(),
            "zen" | "hook_id" | "hook" | "organization" | "repository" | "sender"
        )
    }) || !value["zen"]
        .as_str()
        .is_some_and(|s| !s.is_empty() && s.len() <= 4096)
        || !value["hook_id"].as_u64().is_some_and(numeric)
        || ["hook", "organization", "repository", "sender"]
            .iter()
            .any(|key| object.get(*key).is_some_and(|v| !v.is_object()))
        || object
            .get("hook")
            .is_some_and(|hook| hook["id"].as_u64() != value["hook_id"].as_u64())
    {
        return Err(invalid());
    }
    Ok(())
}
fn domain_error(error: anyhow::Error) -> Fault {
    let message = error.to_string();
    if message.contains("signature") {
        Fault(401, "github_app_signature_refused")
    } else if message.contains("conflict") {
        Fault(409, "github_app_delivery_conflict")
    } else if message.contains("grant") {
        Fault(403, "github_app_grant_refused")
    } else if message.contains("capacity") {
        Fault(413, INGRESS_CAPACITY)
    } else if message.contains("input") || message.contains("binding") {
        invalid()
    } else {
        Fault(500, "github_app_delivery_refused")
    }
}
fn page_packet(shared: &Shared, id: &str, token: &str, d: &Deadline) -> HttpResult<Vec<u8>> {
    let missing = || Fault(404, "github_app_page_not_found");
    let service = shared.pages.as_ref().ok_or_else(missing)?;
    if !super::super::pages::valid_read_token(&shared.host, &shared.workspace_id, id, token)
        .map_err(|_| Fault(503, "github_app_page_unavailable"))?
    {
        return Err(missing());
    }
    let engine = shared
        .engine
        .try_lock()
        .map_err(|_| Fault(503, "github_app_pages_busy"))?;
    let context = super::super::pages::Context {
        store: &engine.store,
        host: &shared.host,
        options: &service.options,
        clock: service.clock.as_ref(),
    };
    let page = super::super::pages::read_page(&context, id, token, d)
        .map_err(|_| {
            if d.check().is_err() {
                Fault(408, "github_app_request_expired")
            } else {
                Fault(503, "github_app_page_unavailable")
            }
        })?
        .ok_or_else(missing)?;
    // Safe packet and policy derive from the exact approved, freshly checked HTML.
    let policy = page
        .content_security_policy()
        .map_err(|_| Fault(503, "github_app_page_unavailable"))?;
    let html = page.html().as_bytes();
    let header = format!(
        "HTTP/1.1 200 Yoke\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\nReferrer-Policy: no-referrer\r\nX-Content-Type-Options: nosniff\r\nContent-Security-Policy: {policy}\r\n\r\n",
        html.len()
    );
    let size = header
        .len()
        .checked_add(html.len())
        .filter(|size| *size <= shared.config.max_response_bytes)
        .ok_or(Fault(507, INGRESS_CAPACITY))?;
    d.check()
        .map_err(|_| Fault(408, "github_app_request_expired"))?;
    let mut packet = Vec::with_capacity(size);
    packet.extend_from_slice(header.as_bytes());
    packet.extend_from_slice(html);
    if service
        .clock
        .now()
        .map_err(|_| Fault(503, "github_app_page_unavailable"))?
        >= page.expires_at_unix()
    {
        return Err(missing());
    }
    d.check()
        .map_err(|_| Fault(408, "github_app_request_expired"))?;
    Ok(packet)
}
fn process(io: &mut Channel<'_>, shared: &Shared, d: &Deadline) -> HttpResult<Vec<u8>> {
    let input = match receive(io, shared, d)? {
        Incoming::Health => {
            return wire(
                200,
                &json!({"ok":true,"pageServing":true,
            "webhookServing":shared.config.allow_webhooks,"complete":false}),
                shared.config.max_response_bytes,
                d,
            );
        }
        Incoming::Page { id, token } => return page_packet(shared, &id, &token, d),
        Incoming::Delivery(input) => input,
    };
    if input.event == "ping" {
        shared
            .host
            .verify(&input.raw, &input.headers.signature256)
            .map_err(domain_error)?;
        ping(&input.raw, shared.options.max_payload_bytes)?;
        let packet = wire(
            200,
            &json!({"authenticated":true,"enqueued":false,"event":"ping",
            "installationTokenVerified":false,"sourceWritten":false,"published":false,"complete":false}),
            shared.config.max_response_bytes,
            d,
        )?;
        shared.counts.pings.fetch_add(1, Ordering::SeqCst);
        return Ok(packet);
    }
    shared
        .host
        .preflight_delivery(&input.headers, &input.raw, &shared.options, d)
        .map_err(domain_error)?;
    let packet = wire(
        202,
        &json!({"accepted":true,"authenticated":true,"durable":true,
        "installationTokenVerified":false,"currentHeadVerified":false,"sourceWritten":false,"published":false,"complete":false}),
        shared.config.max_response_bytes,
        d,
    )?;
    let engine = shared
        .engine
        .try_lock()
        .map_err(|_| Fault(503, "github_app_queue_busy"))?;
    let result = super::super::state::ingest_http(
        &engine.store,
        &shared.host,
        &input.headers,
        &input.raw,
        &shared.options,
        d,
    );
    result.map_err(domain_error)?;
    shared.counts.durable.fetch_add(1, Ordering::SeqCst);
    Ok(packet)
}
pub(super) fn handle(stream: TcpStream, shared: &Shared, d: &Deadline, busy: bool) {
    let Ok(mut io) = Channel::new(stream, shared, d) else {
        shared.counts.refused.fetch_add(1, Ordering::SeqCst);
        return;
    };
    let result = if busy {
        Err(Fault(503, "github_app_connection_capacity_exceeded"))
    } else {
        process(&mut io, shared, d)
    };
    let packet = match result {
        Ok(packet) => Some(packet),
        Err(Fault(status, code)) => {
            shared.counts.refused.fetch_add(1, Ordering::SeqCst);
            wire(
                status,
                &json!({"accepted":false,"error":code,"complete":false}),
                shared.config.max_response_bytes,
                d,
            )
            .ok()
        }
    };
    if let Some(packet) = packet
        && write(&mut io, &packet, d).is_ok()
    {
        shared.counts.replies.fetch_add(1, Ordering::SeqCst);
    }
    io.discard_pending(d);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tls_default_port_accepts_standard_authority_without_weakening_other_ports() {
        assert!(tls_authority_allowed(
            "allowed.example",
            "allowed.example",
            443
        ));
        assert!(tls_authority_allowed(
            "ALLOWED.example:443",
            "allowed.example",
            443
        ));
        assert!(tls_authority_allowed("[::1]", "::1", 443));
        assert!(tls_authority_allowed("[::1]:443", "::1", 443));
        assert!(!tls_authority_allowed(
            "allowed.example",
            "allowed.example",
            8443
        ));
        assert!(tls_authority_allowed(
            "allowed.example:8443",
            "allowed.example",
            8443
        ));
        for value in [
            "other.example",
            "allowed.example:80",
            "allowed.example.",
            "allowed.example@other.example",
            "allowed.example:443:443",
        ] {
            assert!(!tls_authority_allowed(value, "allowed.example", 443));
        }
    }
}
