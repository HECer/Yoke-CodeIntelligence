use super::*;
use serde_json::json;
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
    time::Instant,
};
fn fixture(response: String, delay: Duration) -> (Target, thread::JoinHandle<usize>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let handle = thread::spawn(move || {
        let end = Instant::now() + Duration::from_secs(3);
        let mut count = 0;
        while Instant::now() < end {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    count += 1;
                    stream.set_nonblocking(false).unwrap();
                    stream
                        .set_read_timeout(Some(Duration::from_secs(1)))
                        .unwrap();
                    let mut bytes = Vec::new();
                    let mut buf = [0; 4096];
                    while let Ok(n) = stream.read(&mut buf) {
                        if n == 0 {
                            break;
                        }
                        bytes.extend_from_slice(&buf[..n]);
                        if let Some(i) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
                            let head = std::str::from_utf8(&bytes[..i]).unwrap();
                            let length = head
                                .lines()
                                .find_map(|l| {
                                    l.to_ascii_lowercase()
                                        .strip_prefix("content-length: ")
                                        .and_then(|n| n.parse::<usize>().ok())
                                })
                                .unwrap();
                            if bytes.len() >= i + 4 + length {
                                break;
                            }
                        }
                    }
                    assert!(bytes.starts_with(b"POST /db/neo4j/query/v2 HTTP/1.1\r\n"));
                    let expected =
                        base64::prelude::BASE64_STANDARD.encode("neo4j:private \u{1f4a1}");
                    assert!(
                        String::from_utf8_lossy(&bytes)
                            .contains(&format!("authorization: Basic {expected}"))
                    );
                    thread::sleep(delay);
                    let _ = stream.write_all(response.as_bytes());
                    let quiet = Instant::now() + Duration::from_millis(120);
                    while Instant::now() < quiet {
                        if listener.accept().is_ok() {
                            count += 1
                        }
                        thread::sleep(Duration::from_millis(5));
                    }
                    break;
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5))
                }
                Err(_) => break,
            }
        }
        count
    });
    let target = Target {
        base_url: format!("http://{address}/"),
        address,
        database: "neo4j".into(),
        namespace: "fixture".into(),
        user: "neo4j".into(),
        use_system_roots: false,
        root_certificates_der_base64: vec![],
    };
    (target, handle)
}
fn response(status: &str, body: &str, extra: &str) -> String {
    format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n{extra}\r\n{body}",
        body.len()
    )
}
#[test]
fn query_api_202_errors_redirect_auth_bad_shapes_and_encodings_never_report_success_or_retry() {
    let good = r#"{"data":{"fields":["v"],"values":[[1]]},"queryType":"r"}"#;
    let cases=[
        (response("202 Accepted",good,""),true),
        (response("202 Accepted",r#"{"errors":[{"message":"private \ud83d\udca1"}]}"#,""),false),
        (response("401 Unauthorized","private \u{1f4a1}",""),false),
        (response("400 Bad Request","private \u{1f4a1}",""),false),
        (response("307 Temporary Redirect",good,"Location: http://127.0.0.1:1/\r\n"),false),
        (response("202 Accepted",r#"{"data":{"fields":["v","v"],"values":[[1,2]]}}"#,""),false),
        (response("202 Accepted",r#"{"data":{"fields":["v"],"values":[[]]}}"#,""),false),
        (response("202 Accepted",r#"{"data":{"fields":[],"values":[]},"errors":null}"#,""),false),
        (response("202 Accepted",good,"Content-Encoding: gzip\r\n"),false),
        (response("202 Accepted",good,"Content-Type: application/json\r\n"),false),
        ("HTTP/1.1 202 Accepted\r\nContent-Type: application/json\r\nContent-Length: 32000001\r\nConnection: close\r\n\r\n".into(),false),
        (response("202 Accepted",r#"{"data":{"fields":[],"fields":[],"values":[]}}"#,""),false),
    ];
    for (wire, success) in cases {
        let (target, server) = fixture(wire, Duration::ZERO);
        let d = Deadline::new(3000);
        let session = Session::with_secret(&target, "private \u{1f4a1}", &d).unwrap();
        let result = session.query(
            &json!({"statement":"CYPHER 5 RETURN 1 AS v","parameters":{}}),
            &d,
        );
        assert_eq!(result.is_ok(), success);
        if let Err(e) = result {
            assert!(!format!("{e:#}").contains("private"));
        }
        drop(session);
        assert_eq!(server.join().unwrap(), 1);
    }
}
#[test]
fn query_deadline_drops_the_owned_future_and_does_not_start_a_retry() {
    let (target, server) = fixture(
        response("202 Accepted", r#"{"data":{"fields":[],"values":[]}}"#, ""),
        Duration::from_millis(250),
    );
    let session = Session::with_secret(&target, "private \u{1f4a1}", &Deadline::new(3000)).unwrap();
    let start = Instant::now();
    assert!(
        session
            .query(
                &json!({"statement":"CYPHER 5 RETURN 1","parameters":{}}),
                &Deadline::new(40)
            )
            .is_err()
    );
    assert!(start.elapsed() < Duration::from_secs(1));
    drop(session);
    assert_eq!(server.join().unwrap(), 1);
}
#[test]
fn missing_host_permissions_refuse_before_os_lookup_or_connection() {
    let target:Target=serde_json::from_value(json!({"baseUrl":"http://127.0.0.1:1/","address":"127.0.0.1:1","database":"neo4j","namespace":"fixture","user":"neo4j","useSystemRoots":false})).unwrap();
    for permissions in [
        Permissions::default(),
        Permissions {
            allow_network: true,
            ..Default::default()
        },
        Permissions {
            allow_network: true,
            allow_credential_lookup: true,
            ..Default::default()
        },
    ] {
        let result = Session::open(&target, &permissions, &Deadline::new(1000));
        assert!(result.is_err());
        assert!(!format!("{:#}", result.err().unwrap()).contains("credential_unavailable"));
    }
}
#[test]
fn pinned_address_tls_verifies_static_roots_and_original_hostname_before_credentials_are_sent() {
    use std::sync::Arc;
    for case in 0..3 {
        let certificate =
            rcgen::generate_simple_self_signed(vec!["fixture.invalid".into()]).unwrap();
        let config = rustls::ServerConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(
            vec![certificate.cert.der().clone()],
            rustls::pki_types::PrivatePkcs8KeyDer::from(certificate.key_pair.serialize_der())
                .into(),
        )
        .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let root = if case == 2 {
            rcgen::generate_simple_self_signed(vec!["unrelated.invalid".into()])
                .unwrap()
                .cert
                .der()
                .to_vec()
        } else {
            certificate.cert.der().to_vec()
        };
        let hostname = if case == 1 {
            "wrong.invalid"
        } else {
            "fixture.invalid"
        };
        let target = Target {
            base_url: format!("https://{hostname}:{}/", address.port()),
            address,
            database: "neo4j".into(),
            namespace: "fixture".into(),
            user: "neo4j".into(),
            use_system_roots: false,
            root_certificates_der_base64: vec![base64::prelude::BASE64_STANDARD.encode(root)],
        };
        target.validate().unwrap();
        let server = thread::spawn(move || {
            let end = Instant::now() + Duration::from_secs(3);
            let stream = loop {
                match listener.accept() {
                    Ok((s, _)) => break s,
                    Err(e)
                        if e.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < end =>
                    {
                        thread::sleep(Duration::from_millis(5))
                    }
                    _ => return false,
                }
            };
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            stream
                .set_write_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut wire = rustls::StreamOwned::new(
                rustls::ServerConnection::new(Arc::new(config)).unwrap(),
                stream,
            );
            let mut bytes = Vec::new();
            let mut buf = [0; 4096];
            loop {
                let n = match wire.read(&mut buf) {
                    Ok(0) | Err(_) => return false,
                    Ok(n) => n,
                };
                bytes.extend_from_slice(&buf[..n]);
                if let Some(i) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
                    let head = std::str::from_utf8(&bytes[..i]).unwrap();
                    let length = head
                        .lines()
                        .find_map(|l| {
                            l.to_ascii_lowercase()
                                .strip_prefix("content-length: ")
                                .and_then(|n| n.parse::<usize>().ok())
                        })
                        .unwrap();
                    if bytes.len() >= i + 4 + length {
                        break;
                    }
                }
                assert!(bytes.len() < 65536);
            }
            assert!(String::from_utf8_lossy(&bytes).contains("authorization: Basic "));
            let reply = response(
                "202 Accepted",
                r#"{"data":{"fields":["v"],"values":[[1]]}}"#,
                " ",
            );
            // A normal, bounded HTTP response is sent only after TLS and auth.
            let reply = reply.replace("\r\n \r\n", "\r\n\r\n");
            wire.write_all(reply.as_bytes()).unwrap();
            wire.flush().unwrap();
            true
        });
        let d = Deadline::new(3000);
        let session = Session::with_secret(&target, "private \u{1f4a1}", &d).unwrap();
        let value = session.query(
            &json!({"statement":"CYPHER 5 RETURN 1 AS v","parameters":{}}),
            &d,
        );
        assert_eq!(value.is_ok(), case == 0);
        drop(session);
        assert_eq!(server.join().unwrap(), case == 0);
    }
}
