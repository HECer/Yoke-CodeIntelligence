use ed25519_dalek::SigningKey;
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    sync::Arc,
    time::{Duration, Instant},
};
use tempfile::TempDir;
use yoke::{
    Engine,
    capture::Deadline,
    federation::{self, ExportScope, ImportGrant, Scope},
    federation_http::{
        self, ClientConfig, Config, ExportAccess, ImportAccess, Principal, Role, TlsMaterial,
    },
};
use zeroize::Zeroizing;

fn workspace(name: &str) -> (TempDir, Engine, yoke::model::Snapshot) {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("main.rs"),
        format!("// Grüße 🌍\nfn {name}() {{}}\n"),
    )
    .unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    let snapshot = engine.index(&Deadline::new(10_000)).unwrap().0;
    (dir, engine, snapshot)
}
fn scope() -> ExportScope {
    ExportScope {
        scope: Scope {
            tenant_id: "tenant-one".into(),
            project_id: "demo".into(),
            sender_id: "peer-host".into(),
            key_id: "ephemeral-peer".into(),
        },
        ttl_ms: 60_000,
    }
}
fn peer(snapshot: &yoke::model::Snapshot) -> ImportGrant {
    ImportGrant {
        scope: scope().scope,
        public_key: hex::encode(SigningKey::from_bytes(&[9; 32]).verifying_key().to_bytes()),
        sender_workspace_id: snapshot.workspace_id.clone(),
        exclude: vec![],
    }
}
fn config() -> (Config, Vec<u8>) {
    // Actual ephemeral certificate/private key exist only in fixture RAM.
    let rcgen::CertifiedKey { cert, key_pair } =
        rcgen::generate_simple_self_signed(vec!["localhost".into(), "127.0.0.1".into()]).unwrap();
    let certificate = cert.der().to_vec();
    (
        Config {
            bind: "127.0.0.1:0".parse().unwrap(),
            tls: Some(TlsMaterial {
                certificates: vec![certificate.clone()],
                private_key: Zeroizing::new(key_pair.serialize_der()),
            }),
            allow_remote_bind: false,
            server_names: vec!["localhost".into()],
            request_timeout_ms: 3000,
            max_body_bytes: 32_000_000,
            max_response_bytes: 32_000_000,
            imports: vec![],
            exports: vec![],
        },
        certificate,
    )
}
fn principal(id: &str, byte: u8, roles: &[Role], imports: &[&str], exports: &[&str]) -> Principal {
    Principal::new(
        id,
        &[byte; 32],
        roles,
        &imports.iter().map(|id| id.to_string()).collect::<Vec<_>>(),
        &exports.iter().map(|id| id.to_string()).collect::<Vec<_>>(),
    )
    .unwrap()
}
fn tls_request(addr: SocketAddr, certificate: &[u8], request: &[u8]) -> (u16, Value) {
    let mut roots = rustls::RootCertStore::empty();
    roots
        .add(rustls::pki_types::CertificateDer::from(
            certificate.to_vec(),
        ))
        .unwrap();
    let client = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_protocol_versions(&[&rustls::version::TLS13])
    .unwrap()
    .with_root_certificates(roots)
    .with_no_client_auth();
    let connection = rustls::ClientConnection::new(
        Arc::new(client),
        rustls::pki_types::ServerName::try_from("localhost".to_owned()).unwrap(),
    )
    .unwrap();
    let tcp = TcpStream::connect_timeout(&addr, Duration::from_secs(5)).unwrap();
    tcp.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    tcp.set_write_timeout(Some(Duration::from_secs(5))).unwrap();
    let mut stream = rustls::StreamOwned::new(connection, tcp);
    stream.write_all(request).unwrap();
    stream.flush().unwrap();
    let mut response = Vec::new();
    stream.take(64_000_000).read_to_end(&mut response).unwrap();
    let end = response
        .windows(4)
        .position(|bytes| bytes == b"\r\n\r\n")
        .expect("Complete HTTP header");
    let header = std::str::from_utf8(&response[..end]).unwrap();
    let status = header
        .lines()
        .next()
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse()
        .unwrap();
    let size: usize = header
        .lines()
        .find_map(|line| line.strip_prefix("Content-Length: "))
        .unwrap()
        .parse()
        .unwrap();
    let body = &response[end + 4..];
    assert_eq!(body.len(), size, "No truncated success responses");
    (
        status,
        if body.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(body).unwrap()
        },
    )
}
fn request(
    addr: SocketAddr,
    certificate: &[u8],
    byte: u8,
    method: &str,
    path: &str,
    body: Option<&Value>,
) -> (u16, Value) {
    let config = ClientConfig {
        address: addr,
        server_name: "localhost".into(),
        ca_certificates: vec![certificate.to_vec()],
        token: Zeroizing::new(vec![byte; 32]),
        allow_plaintext_loopback: false,
        max_body_bytes: 32_000_000,
        max_response_bytes: 32_000_000,
    };
    let response =
        federation_http::request(&config, method, path, body.cloned(), &Deadline::new(5000))
            .unwrap();
    (response.status, response.body)
}

#[test]
fn actual_tls_import_and_scoped_read_preserve_source_and_foreign_namespace() {
    let (_sender_dir, sender, source) = workspace("shared");
    let (dir, receiver, local) = workspace("local");
    let grant = peer(&source);
    let envelope = federation::export_snapshot(
        &sender.store,
        &source,
        &scope(),
        &[9; 32],
        &Deadline::new(10_000),
    )
    .unwrap();
    let (mut config, certificate) = config();
    config.imports.push(ImportAccess {
        id: "peer".into(),
        grant: grant.clone(),
    });
    let service = federation_http::serve(
        dir.path(),
        config,
        vec![
            principal("reader", 3, &[Role::Read], &["peer"], &[]),
            principal("importer", 4, &[Role::Import], &["peer"], &[]),
        ],
    )
    .unwrap();
    let addr = service.local_addr();
    let (status, receipt) = request(
        addr,
        &certificate,
        4,
        "POST",
        "/v1/import/peer",
        Some(&envelope),
    );
    assert_eq!(status, 200, "{receipt}");
    assert_eq!(receipt["sourceWritten"], false);
    let id = receipt["remoteId"].as_str().unwrap();
    let (status, read) = request(
        addr,
        &certificate,
        3,
        "GET",
        &format!("/v1/read/peer/{id}?path=main.rs"),
        None,
    );
    assert_eq!(status, 200, "{read}");
    assert!(read["text"].as_str().unwrap().contains("fn shared()"));
    assert_eq!(read["namespace"], "federated");
    assert!(receiver.store.snapshot(id).is_err());
    assert_eq!(receiver.store.latest().unwrap(), Some(local.id));
    assert!(
        fs::read_to_string(dir.path().join("main.rs"))
            .unwrap()
            .contains("fn local()")
    );
    service.shutdown().unwrap();
}

#[test]
fn actual_tls_role_scope_token_and_revocation_gates_cannot_be_granted_by_payload() {
    let (_sender_dir, sender, source) = workspace("shared");
    let (dir, receiver, _) = workspace("local");
    let grant = peer(&source);
    let envelope = federation::export_snapshot(
        &sender.store,
        &source,
        &scope(),
        &[9; 32],
        &Deadline::new(10_000),
    )
    .unwrap();
    let (mut config, certificate) = config();
    config.imports.push(ImportAccess {
        id: "peer".into(),
        grant,
    });
    let service = federation_http::serve(
        dir.path(),
        config,
        vec![
            principal("reader", 3, &[Role::Read], &["peer"], &[]),
            principal("importer", 4, &[Role::Import], &["peer"], &[]),
            principal("admin", 5, &[Role::Admin], &[], &[]),
        ],
    )
    .unwrap();
    let addr = service.local_addr();
    assert_eq!(
        request(addr, &certificate, 8, "GET", "/v1/status", None).0,
        401
    );
    assert_eq!(
        request(
            addr,
            &certificate,
            3,
            "POST",
            "/v1/import/peer",
            Some(&envelope)
        )
        .0,
        403
    );
    assert_eq!(
        request(
            addr,
            &certificate,
            4,
            "POST",
            "/v1/import/other",
            Some(&envelope)
        )
        .0,
        403
    );
    let mut wrong_scope = envelope.clone();
    wrong_scope["manifest"]["scope"]["tenantId"] = json!("another-tenant");
    assert_eq!(
        request(
            addr,
            &certificate,
            4,
            "POST",
            "/v1/import/peer",
            Some(&wrong_scope)
        )
        .0,
        400
    );
    assert_eq!(
        federation::list(&receiver.store, &Deadline::new(10_000)).unwrap()["items"],
        json!([])
    );
    assert_eq!(
        request(
            addr,
            &certificate,
            5,
            "POST",
            "/v1/admin/revoke",
            Some(&json!({"principalId":"reader","roles":["admin"]}))
        )
        .0,
        400
    );
    assert_eq!(
        request(
            addr,
            &certificate,
            5,
            "POST",
            "/v1/admin/revoke",
            Some(&json!({"principalId":"reader"}))
        )
        .0,
        200
    );
    assert_eq!(
        request(addr, &certificate, 3, "GET", "/v1/status", None).0,
        401
    );
    service
        .upsert(principal("reader", 10, &[Role::Read], &["peer"], &[]))
        .unwrap();
    assert_eq!(
        request(addr, &certificate, 10, "GET", "/v1/status", None).0,
        200
    );
    assert!(
        service
            .upsert(principal(
                "reader",
                11,
                &[Role::Read],
                &["not-a-host-grant"],
                &[]
            ))
            .is_err()
    );
    assert_eq!(
        request(addr, &certificate, 10, "GET", "/v1/status", None).0,
        200
    );
    service.shutdown().unwrap();
}

#[test]
fn actual_tls_source_listing_does_not_disclose_other_tenants() {
    let (_sender_dir, sender, source) = workspace("shared");
    let (dir, receiver, _) = workspace("local");
    let grant = peer(&source);
    let envelope = federation::export_snapshot(
        &sender.store,
        &source,
        &scope(),
        &[9; 32],
        &Deadline::new(10_000),
    )
    .unwrap();
    federation::import_snapshot(&receiver, envelope, &grant, &Deadline::new(10_000)).unwrap();
    let mut other_scope = scope();
    other_scope.scope.tenant_id = "private-other-tenant".into();
    let mut other_grant = grant.clone();
    other_grant.scope = other_scope.scope.clone();
    let other = federation::export_snapshot(
        &sender.store,
        &source,
        &other_scope,
        &[9; 32],
        &Deadline::new(10_000),
    )
    .unwrap();
    federation::import_snapshot(&receiver, other, &other_grant, &Deadline::new(10_000)).unwrap();
    let (mut config, certificate) = config();
    config.imports.push(ImportAccess {
        id: "peer".into(),
        grant,
    });
    let service = federation_http::serve(
        dir.path(),
        config,
        vec![principal("reader", 3, &[Role::Read], &["peer"], &[])],
    )
    .unwrap();
    let (status, listing) = request(
        service.local_addr(),
        &certificate,
        3,
        "GET",
        "/v1/sources",
        None,
    );
    assert_eq!(status, 200, "{listing}");
    assert_eq!(listing["items"].as_array().unwrap().len(), 1);
    assert!(
        !serde_json::to_string(&listing)
            .unwrap()
            .contains("private-other-tenant")
    );
    service.shutdown().unwrap();
}

#[test]
fn actual_tls_export_keeps_existing_signed_envelope_contract() {
    let (dir, engine, snapshot) = workspace("local");
    let (recipient_dir, recipient, _) = workspace("recipient");
    let (mut config, certificate) = config();
    let mut export = scope();
    export.scope.sender_id = "local-service".into();
    config.exports.push(ExportAccess {
        id: "own".into(),
        configuration: export.clone(),
        private_key: Zeroizing::new([13; 32]),
    });
    let grant = ImportGrant {
        scope: export.scope,
        public_key: hex::encode(SigningKey::from_bytes(&[13; 32]).verifying_key().to_bytes()),
        sender_workspace_id: engine.store.workspace_id.clone(),
        exclude: vec![],
    };
    let service = federation_http::serve(
        dir.path(),
        config,
        vec![principal("reader", 3, &[Role::Read], &[], &["own"])],
    )
    .unwrap();
    let (status, envelope) = request(
        service.local_addr(),
        &certificate,
        3,
        "GET",
        &format!("/v1/export/own/{}", snapshot.id),
        None,
    );
    assert_eq!(status, 200, "{envelope}");
    let imported =
        federation::import_snapshot(&recipient, envelope, &grant, &Deadline::new(10_000)).unwrap();
    let read = federation::read(
        &recipient,
        imported["remoteId"].as_str().unwrap(),
        "main.rs",
        &grant,
        &Deadline::new(10_000),
    )
    .unwrap();
    assert!(read["text"].as_str().unwrap().contains("fn local()"));
    assert!(
        fs::read_to_string(recipient_dir.path().join("main.rs"))
            .unwrap()
            .contains("fn recipient()")
    );
    service.shutdown().unwrap();
}

#[test]
fn actual_tls_rejects_browser_origin_wrong_host_and_ambiguous_http_framing() {
    let (dir, _, _) = workspace("local");
    let (config, certificate) = config();
    let service = federation_http::serve(
        dir.path(),
        config,
        vec![principal("reader", 3, &[Role::Read], &[], &[])],
    )
    .unwrap();
    let addr = service.local_addr();
    for (target, host, extra) in [
        (
            "/v1/status",
            addr.to_string(),
            "Origin: https://untrusted.invalid\r\n",
        ),
        ("/v1/status", "other.invalid:443".into(), ""),
        (
            "/v1/status",
            addr.to_string(),
            "Content-Length: 0\r\nContent-Length: 0\r\n",
        ),
        (
            "/v1/status",
            addr.to_string(),
            "Transfer-Encoding: chunked\r\nContent-Length: 0\r\n",
        ),
        ("http://other.invalid/v1/status", addr.to_string(), ""),
    ] {
        let message = format!(
            "GET {target} HTTP/1.1\r\nHost: {host}\r\nAuthorization: Bearer {}\r\n{extra}Connection: close\r\n\r\n",
            hex::encode([3; 32])
        );
        let (status, _) = tls_request(addr, &certificate, message.as_bytes());
        assert!((400..500).contains(&status));
    }
    service.shutdown().unwrap();
}

#[test]
fn explicit_remote_tls_permission_and_fixed_host_grants_are_required_before_bind() {
    let (dir, _, _) = workspace("local");
    let (mut config, _) = config();
    config.bind = "0.0.0.0:0".parse().unwrap();
    config.tls = None;
    config.allow_remote_bind = true;
    assert!(
        federation_http::serve(
            dir.path(),
            config,
            vec![principal("reader", 3, &[Role::Read], &[], &[])]
        )
        .is_err()
    );
    let (mut config, _) = self::config();
    config.bind = "0.0.0.0:0".parse().unwrap();
    assert!(
        federation_http::serve(
            dir.path(),
            config,
            vec![principal("reader", 3, &[Role::Read], &[], &[])]
        )
        .is_err()
    );
    let (config, _) = self::config();
    assert!(
        federation_http::serve(
            dir.path(),
            config,
            vec![principal("reader", 3, &[Role::Read], &["unknown"], &[])]
        )
        .is_err()
    );
}

#[test]
fn shutdown_cancels_idle_tls_handshake_without_waiting_for_request_deadline() {
    let (dir, _, _) = workspace("local");
    let (mut config, _) = config();
    config.request_timeout_ms = 10_000;
    let service = federation_http::serve(
        dir.path(),
        config,
        vec![principal("reader", 3, &[Role::Read], &[], &[])],
    )
    .unwrap();
    let addr = service.local_addr();
    let _idle = TcpStream::connect(addr).unwrap();
    std::thread::sleep(Duration::from_millis(100));
    let started = Instant::now();
    service.shutdown().unwrap();
    assert!(started.elapsed() < Duration::from_secs(1));
    assert!(TcpStream::connect_timeout(&addr, Duration::from_millis(100)).is_err());
}

#[test]
fn http_body_and_response_budget_failures_never_become_complete_source_responses() {
    let (dir, _, _) = workspace("local");
    let (mut config, certificate) = config();
    config.max_body_bytes = 1024;
    config.max_response_bytes = 32;
    let service = federation_http::serve(
        dir.path(),
        config,
        vec![principal("reader", 3, &[Role::Read], &[], &[])],
    )
    .unwrap();
    let addr = service.local_addr();
    let header = format!(
        "POST /v1/import/none HTTP/1.1\r\nHost: {addr}\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nContent-Length: 1025\r\nConnection: close\r\n\r\n",
        hex::encode([3; 32])
    );
    assert_eq!(tls_request(addr, &certificate, header.as_bytes()).0, 413);
    assert!((400..600).contains(&request(addr, &certificate, 3, "GET", "/v1/status", None).0));
    service.shutdown().unwrap();
}

#[test]
fn authentication_role_and_host_denials_do_not_wait_for_untrusted_body_bytes() {
    let (_sender_dir, _, source) = workspace("shared");
    let (dir, _, _) = workspace("local");
    let (mut config, certificate) = config();
    config.imports.push(ImportAccess {
        id: "peer".into(),
        grant: peer(&source),
    });
    let service = federation_http::serve(
        dir.path(),
        config,
        vec![principal("reader", 3, &[Role::Read], &["peer"], &[])],
    )
    .unwrap();
    let addr = service.local_addr();
    for (byte, host, target, expected) in [
        (8, addr.to_string(), "/v1/import/peer", 401),
        (3, addr.to_string(), "/v1/import/peer", 403),
        (3, "other.invalid:443".into(), "/v1/import/peer", 403),
    ] {
        let header = format!(
            "POST {target} HTTP/1.1\r\nHost: {host}\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nContent-Length: 4096\r\nConnection: close\r\n\r\n",
            hex::encode([byte; 32])
        );
        let started = Instant::now();
        assert_eq!(
            tls_request(addr, &certificate, header.as_bytes()).0,
            expected
        );
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "Denial must not await source-controlled body bytes"
        );
    }
    service.shutdown().unwrap();
}

#[test]
fn actual_tls_client_requires_explicit_ca_name_and_loopback_plaintext_permission() {
    let (dir, _, _) = workspace("local");
    let (config, certificate) = config();
    let service = federation_http::serve(
        dir.path(),
        config,
        vec![principal("reader", 3, &[Role::Read], &[], &[])],
    )
    .unwrap();
    let mut client = ClientConfig {
        address: service.local_addr(),
        server_name: "localhost".into(),
        ca_certificates: vec![certificate],
        token: Zeroizing::new(vec![3; 32]),
        allow_plaintext_loopback: false,
        max_body_bytes: 32_000_000,
        max_response_bytes: 32_000_000,
    };
    client.server_name = "untrusted.invalid".into();
    assert!(
        federation_http::request(&client, "GET", "/v1/status", None, &Deadline::new(1000)).is_err()
    );
    client.server_name = "localhost".into();
    let rcgen::CertifiedKey { cert, .. } =
        rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    client.ca_certificates = vec![cert.der().to_vec()];
    assert!(
        federation_http::request(&client, "GET", "/v1/status", None, &Deadline::new(1000)).is_err()
    );
    client.ca_certificates.clear();
    assert!(
        federation_http::request(&client, "GET", "/v1/status", None, &Deadline::new(1000)).is_err()
    );
    client.allow_plaintext_loopback = true;
    client.address = "192.0.2.1:443".parse().unwrap();
    assert!(
        federation_http::request(&client, "GET", "/v1/status", None, &Deadline::new(1000)).is_err()
    );
    service.shutdown().unwrap();
}

fn scripted_client(
    reply: &'static [u8],
    delay_ms: u64,
    max_response: usize,
) -> anyhow::Result<federation_http::ClientResponse> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let worker = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let mut header = Vec::new();
        let mut byte = [0u8; 1];
        while !header.ends_with(b"\r\n\r\n") {
            if stream.read(&mut byte).unwrap_or(0) == 0 {
                return;
            }
            header.push(byte[0]);
            assert!(header.len() < 16_384);
        }
        std::thread::sleep(Duration::from_millis(delay_ms));
        let _ = stream.write_all(reply);
    });
    let config = ClientConfig {
        address,
        server_name: String::new(),
        ca_certificates: vec![],
        token: Zeroizing::new(vec![3; 32]),
        allow_plaintext_loopback: true,
        max_body_bytes: 1024,
        max_response_bytes: max_response,
    };
    let result = federation_http::request(&config, "GET", "/v1/status", None, &Deadline::new(150));
    worker.join().unwrap();
    result
}

#[test]
fn client_refuses_truncated_ambiguous_overbudget_redirect_and_late_responses() {
    let valid=b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{}";
    assert_eq!(scripted_client(valid, 0, 1024).unwrap().status, 200);
    for reply in [
        &b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\nContent-Type: application/json\r\n\r\n{}"[..],
        &b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nContent-Length: 2\r\nContent-Type: application/json\r\n\r\n{}"[..],
        &b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nTransfer-Encoding: chunked\r\nContent-Type: application/json\r\n\r\n{}"[..],
        &b"HTTP/1.1 200 OK\r\nContent-Length: 999999\r\nContent-Type: application/json\r\n\r\n"[..],
        &b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nContent-Type: application/json\r\n\r\n{}x"[..],
    ]{assert!(scripted_client(reply,0,1024).is_err());}
    assert!(scripted_client(valid, 250, 1024).is_err());
    assert!(scripted_client(valid, 0, 1).is_err());
    // A redirect remains an explicit non-success response and is not followed.
    let redirect=b"HTTP/1.1 302 Moved\r\nLocation: https://elsewhere.invalid/\r\nContent-Length: 2\r\nContent-Type: application/json\r\n\r\n{}";
    assert_eq!(scripted_client(redirect, 0, 1024).unwrap().status, 302);
}

#[test]
fn scoped_listing_does_not_disguise_invalid_retained_source_as_a_complete_empty_result() {
    let (_sender_dir, sender, source) = workspace("shared");
    let (dir, receiver, _) = workspace("local");
    let grant = peer(&source);
    let envelope = federation::export_snapshot(
        &sender.store,
        &source,
        &scope(),
        &[9; 32],
        &Deadline::new(10_000),
    )
    .unwrap();
    let imported =
        federation::import_snapshot(&receiver, envelope, &grant, &Deadline::new(10_000)).unwrap();
    let id = imported["remoteId"].as_str().unwrap();
    // The fixture modifies only its own retained receipt, preserving its scope
    // so it must be revalidated rather than silently omitted as another tenant.
    let raw: String = receiver
        .store
        .db
        .query_row(
            "SELECT value FROM records WHERE kind='native_federation_receipt_v1' AND id=?1",
            [id],
            |row| row.get(0),
        )
        .unwrap();
    let mut receipt: Value = serde_json::from_str(&raw).unwrap();
    receipt["publicKey"] = json!("0".repeat(64));
    receiver
        .store
        .db
        .execute(
            "UPDATE records SET value=?1 WHERE kind='native_federation_receipt_v1' AND id=?2",
            rusqlite::params![serde_json::to_string(&receipt).unwrap(), id],
        )
        .unwrap();
    let (mut config, certificate) = config();
    config.imports.push(ImportAccess {
        id: "peer".into(),
        grant,
    });
    let service = federation_http::serve(
        dir.path(),
        config,
        vec![principal("reader", 3, &[Role::Read], &["peer"], &[])],
    )
    .unwrap();
    let (status, body) = request(
        service.local_addr(),
        &certificate,
        3,
        "GET",
        "/v1/sources",
        None,
    );
    assert!(
        (400..600).contains(&status),
        "Invalid retained scoped data must not produce200: {body}"
    );
    assert_eq!(body["complete"], false);
    service.shutdown().unwrap();
}

#[test]
fn client_binds_http_authority_to_the_verified_tls_server_name() {
    let (mut config, certificate) = config();
    let material = config.tls.take().unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let tls = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_protocol_versions(&[&rustls::version::TLS13])
    .unwrap()
    .with_no_client_auth()
    .with_single_cert(
        material
            .certificates
            .into_iter()
            .map(rustls::pki_types::CertificateDer::from)
            .collect(),
        rustls::pki_types::PrivateKeyDer::try_from(material.private_key.to_vec()).unwrap(),
    )
    .unwrap();
    let worker = std::thread::spawn(move || {
        let (tcp, _) = listener.accept().unwrap();
        tcp.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
        let mut stream =
            rustls::StreamOwned::new(rustls::ServerConnection::new(Arc::new(tls)).unwrap(), tcp);
        let mut header = Vec::new();
        let mut byte = [0u8; 1];
        while !header.ends_with(b"\r\n\r\n") {
            assert_eq!(stream.read(&mut byte).unwrap(), 1);
            header.push(byte[0]);
            assert!(header.len() < 16_384);
        }
        let exact = format!("\r\nHost: localhost:{}\r\n", address.port());
        let matches = std::str::from_utf8(&header).unwrap().contains(&exact);
        let status = if matches { 200 } else { 403 };
        stream.write_all(format!("HTTP/1.1 {status} Response\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}").as_bytes()).unwrap();
        stream.conn.send_close_notify();
        stream.flush().unwrap();
    });
    let client = ClientConfig {
        address,
        server_name: "localhost".into(),
        ca_certificates: vec![certificate],
        token: Zeroizing::new(vec![3; 32]),
        allow_plaintext_loopback: false,
        max_body_bytes: 1024,
        max_response_bytes: 1024,
    };
    assert_eq!(
        federation_http::request(&client, "GET", "/v1/status", None, &Deadline::new(1000))
            .unwrap()
            .status,
        200,
        "TLS server authority must remain usable with an explicitly configured wildcard bind"
    );
    worker.join().unwrap();
}

#[test]
fn shutdown_bounds_a_core_import_waiting_on_a_state_writer() {
    let (_sender_dir, sender, source) = workspace("shared");
    let (dir, receiver, _) = workspace("local");
    let grant = peer(&source);
    let envelope = federation::export_snapshot(
        &sender.store,
        &source,
        &scope(),
        &[9; 32],
        &Deadline::new(10_000),
    )
    .unwrap();
    let (mut config, certificate) = config();
    config.request_timeout_ms = 30_000;
    config.imports.push(ImportAccess {
        id: "peer".into(),
        grant,
    });
    let service = federation_http::serve(
        dir.path(),
        config,
        vec![principal("importer", 4, &[Role::Import], &["peer"], &[])],
    )
    .unwrap();
    let address = service.local_addr();
    receiver.store.db.execute_batch("BEGIN IMMEDIATE").unwrap();
    let client = std::thread::spawn(move || {
        let config = ClientConfig {
            address,
            server_name: "localhost".into(),
            ca_certificates: vec![certificate],
            token: Zeroizing::new(vec![4; 32]),
            allow_plaintext_loopback: false,
            max_body_bytes: 32_000_000,
            max_response_bytes: 32_000_000,
        };
        federation_http::request(
            &config,
            "POST",
            "/v1/import/peer",
            Some(envelope),
            &Deadline::new(15_000),
        )
    });
    std::thread::sleep(Duration::from_millis(200));
    let started = Instant::now();
    let result = service.shutdown();
    receiver.store.db.execute_batch("ROLLBACK").unwrap();
    let _ = client.join().unwrap();
    assert!(
        result.is_ok(),
        "The core operation budget must allow joining the owned worker: {result:?}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "Shutdown waited beyond its explicit5s bound"
    );
    assert!(std::net::TcpStream::connect_timeout(&address, Duration::from_millis(100)).is_err());
    assert_eq!(
        federation::list(&receiver.store, &Deadline::new(10_000)).unwrap()["items"],
        json!([])
    );
    assert!(
        fs::read_to_string(dir.path().join("main.rs"))
            .unwrap()
            .contains("fn local()")
    );
}

#[test]
fn partial_tls_records_cannot_extend_the_server_deadline_or_block_the_next_client() {
    let (dir, _, _) = workspace("local");
    let (mut config, certificate) = config();
    config.request_timeout_ms = 100;
    let service = federation_http::serve(
        dir.path(),
        config,
        vec![principal("reader", 3, &[Role::Read], &[], &[])],
    )
    .unwrap();
    let address = service.local_addr();
    let mut attacker = TcpStream::connect(address).unwrap();
    attacker.set_nodelay(true).unwrap();
    attacker
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    let mut roots = rustls::RootCertStore::empty();
    roots
        .add(rustls::pki_types::CertificateDer::from(certificate.clone()))
        .unwrap();
    let tls = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_protocol_versions(&[&rustls::version::TLS13])
    .unwrap()
    .with_root_certificates(roots)
    .with_no_client_auth();
    let mut connection = rustls::ClientConnection::new(
        Arc::new(tls),
        rustls::pki_types::ServerName::try_from("localhost".to_owned()).unwrap(),
    )
    .unwrap();
    while connection.is_handshaking() {
        connection.complete_io(&mut attacker).unwrap();
    }
    while connection.wants_write() {
        connection.write_tls(&mut attacker).unwrap();
    }
    connection.writer().write_all(&vec![b'G'; 16_000]).unwrap();
    let mut record = Vec::new();
    connection.write_tls(&mut record).unwrap();
    assert_eq!(record[0], 0x17);
    assert!(u16::from_be_bytes([record[3], record[4]]) > 16_000);
    let (started, ready) = std::sync::mpsc::sync_channel(1);
    let drip = std::thread::spawn(move || {
        // Actual encrypted application bytes cannot be authenticated/decrypted
        // until their record is complete. This bypasses early malformed-hello
        // refusal and keeps each physical read below its inactivity timeout.
        attacker.write_all(&record[..5]).unwrap();
        started.send(()).unwrap();
        let until = Instant::now() + Duration::from_millis(1200);
        let mut offset = 5;
        while Instant::now() < until && offset < record.len() {
            if attacker.write_all(&record[offset..offset + 1]).is_err() {
                break;
            }
            offset += 1;
            std::thread::sleep(Duration::from_millis(1));
        }
    });
    ready.recv_timeout(Duration::from_secs(1)).unwrap();
    std::thread::sleep(Duration::from_millis(150));
    let client = ClientConfig {
        address,
        server_name: "localhost".into(),
        ca_certificates: vec![certificate],
        token: Zeroizing::new(vec![3; 32]),
        allow_plaintext_loopback: false,
        max_body_bytes: 1024,
        max_response_bytes: 1024,
    };
    let started = Instant::now();
    let response =
        federation_http::request(&client, "GET", "/v1/status", None, &Deadline::new(1000));
    let elapsed = started.elapsed();
    service.shutdown().unwrap();
    drip.join().unwrap();
    assert!(
        elapsed < Duration::from_millis(350),
        "Partial TLS bytes kept the sole worker beyond its100ms deadline: {elapsed:?}"
    );
    assert_eq!(
        response.unwrap().status,
        200,
        "A later authorized client must remain available"
    );
}

#[test]
fn partial_tls_records_cannot_extend_the_public_client_deadline() {
    let (_, certificate) = config();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let peer = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket.set_nodelay(true).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let mut hello = [0u8; 4096];
        assert!(socket.read(&mut hello).unwrap() > 0);
        socket.write_all(&[0x16, 0x03, 0x03, 0x40, 0x00]).unwrap();
        let until = Instant::now() + Duration::from_millis(1200);
        while Instant::now() < until {
            if socket.write_all(&[0]).is_err() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    });
    let client = ClientConfig {
        address,
        server_name: "localhost".into(),
        ca_certificates: vec![certificate],
        token: Zeroizing::new(vec![3; 32]),
        allow_plaintext_loopback: false,
        max_body_bytes: 1024,
        max_response_bytes: 1024,
    };
    let started = Instant::now();
    let result = federation_http::request(&client, "GET", "/v1/status", None, &Deadline::new(100));
    let elapsed = started.elapsed();
    peer.join().unwrap();
    assert!(
        result.is_err(),
        "Incomplete TLS cannot produce a source response"
    );
    assert!(
        elapsed < Duration::from_millis(350),
        "Peer drip extended the shared100ms client deadline: {elapsed:?}"
    );
}
