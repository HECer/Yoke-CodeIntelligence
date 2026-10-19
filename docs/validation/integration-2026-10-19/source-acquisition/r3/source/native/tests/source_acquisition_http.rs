//! Owned HTTP/TLS wire fixtures, distinct from actual public-backend evidence.
use base64::Engine as _;
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
use yoke::{
    capture::Deadline,
    source_acquisition::{Format, Limits},
    source_acquisition_http::{Permissions, Spec, capture},
};
trait Wire: Read + Write {}
impl<T: Read + Write> Wire for T {}
struct Server {
    spec: Spec,
    request: Arc<Mutex<Vec<u8>>>,
    closed: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Server {
    fn new(reply: Vec<u8>, tls: bool, stall: bool) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let (config, root) = if tls {
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
            (
                Some(Arc::new(config)),
                vec![base64::prelude::BASE64_STANDARD.encode(certificate.cert.der())],
            )
        } else {
            (None, Vec::new())
        };
        let request = Arc::new(Mutex::new(Vec::new()));
        let recorded = request.clone();
        let closed = Arc::new(AtomicBool::new(false));
        let observed = closed.clone();
        listener.set_nonblocking(true).unwrap();
        let worker = thread::spawn(move || {
            let start = Instant::now();
            let stream = loop {
                match listener.accept() {
                    Ok((s, _)) => break s,
                    Err(e)
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            && start.elapsed() < Duration::from_secs(5) =>
                    {
                        thread::sleep(Duration::from_millis(5))
                    }
                    _ => return,
                }
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            stream
                .set_write_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut io: Box<dyn Wire> = if let Some(config) = config {
                Box::new(rustls::StreamOwned::new(
                    rustls::ServerConnection::new(config).unwrap(),
                    stream,
                ))
            } else {
                Box::new(stream)
            };
            let mut bytes = Vec::new();
            let mut chunk = [0u8; 1024];
            while !bytes.windows(4).any(|v| v == b"\r\n\r\n") {
                let n = match io.read(&mut chunk) {
                    Ok(0) | Err(_) => return,
                    Ok(n) => n,
                };
                bytes.extend_from_slice(&chunk[..n]);
                if bytes.len() > 32000 {
                    return;
                }
            }
            *recorded.lock().unwrap() = bytes;
            if !stall {
                let _ = io.write_all(&reply);
                let _ = io.flush();
                return;
            }
            match io.read(&mut chunk) {
                Ok(0) | Err(_) => {
                    observed.store(true, Ordering::SeqCst);
                }
                Ok(_) => {}
            }
        });
        Self {
            spec: Spec {
                url: format!(
                    "{}://{}:{}/fixture.txt",
                    if tls { "https" } else { "http" },
                    if tls { "fixture.invalid" } else { "127.0.0.1" },
                    address.port()
                ),
                address,
                format: Format::Text,
                use_system_roots: false,
                root_certificates_der_base64: root,
            },
            request,
            closed,
            worker: Some(worker),
        }
    }
    fn join(&mut self) {
        if let Some(worker) = self.worker.take() {
            worker.join().unwrap();
        }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.join();
    }
}
fn reply(body: &[u8], extra: &str) -> Vec<u8> {
    [format!("HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\n{extra}Connection: close\r\n\r\n",body.len()).as_bytes(),body].concat()
}
fn permissions() -> Permissions {
    Permissions {
        allow_network: true,
        allow_loopback_http: true,
    }
}
#[test]
fn actual_loopback_get_and_chunked_representation_preserve_body_and_fixed_request_headers() {
    for wire in [reply("actual 💡\r\n".as_bytes(),""),b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n3\r\nabc\r\n2\r\nde\r\n0\r\n\r\n".to_vec()] {
        let mut server=Server::new(wire,false,false);let received=capture(&server.spec,&permissions(),&Limits::default(),&Deadline::new(5000)).unwrap();assert!(received.body()=="actual 💡\r\n".as_bytes() || received.body()==b"abcde");server.join();let request=String::from_utf8(server.request.lock().unwrap().clone()).unwrap().to_ascii_lowercase();assert!(request.starts_with("get /fixture.txt http/1.1"));assert!(request.contains("accept-encoding: identity"));assert!(!request.contains("authorization:") && !request.contains("cookie:"));assert_eq!(received.as_value()["sourceAuthority"],false);
    }
}
#[test]
fn tls_uses_explicit_roots_hostname_verification_and_literal_address_override_without_dns() {
    let mut server = Server::new(reply(b"TLS body", ""), true, false);
    let source = capture(
        &server.spec,
        &permissions(),
        &Limits::default(),
        &Deadline::new(5000),
    )
    .unwrap();
    assert_eq!(source.body(), b"TLS body");
    server.join();
    assert!(
        String::from_utf8(server.request.lock().unwrap().clone())
            .unwrap()
            .contains("fixture.invalid")
    );
    let mut server = Server::new(reply(b"must not pass", ""), true, false);
    server.spec.url = server.spec.url.replace("fixture.invalid", "wrong.invalid");
    assert!(
        capture(
            &server.spec,
            &permissions(),
            &Limits::default(),
            &Deadline::new(5000)
        )
        .is_err()
    );
    server.join();
    assert!(server.request.lock().unwrap().is_empty());
}
#[test]
fn redirects_error_bodies_duplicate_headers_compression_and_truncated_bodies_are_refused() {
    for wire in [
        b"HTTP/1.1 302 Found\r\nLocation: http://192.0.2.1/private\r\nContent-Length: 0\r\n\r\n"
            .to_vec(),
        b"HTTP/1.1 403 Forbidden\r\nContent-Length: 22\r\n\r\nprivate sentinel bytes".to_vec(),
        reply(b"abc", "Content-Encoding: gzip\r\n"),
        reply(b"abc", "Content-Type: application/pdf\r\n"),
        b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 100\r\n\r\nshort".to_vec(),
    ] {
        let mut server = Server::new(wire, false, false);
        let error = capture(
            &server.spec,
            &permissions(),
            &Limits::default(),
            &Deadline::new(5000),
        )
        .err()
        .unwrap();
        assert!(!error.to_string().contains("private"));
        server.join();
    }
}
#[test]
fn response_overflow_has_no_partial_success_and_deadline_or_host_cancel_closes_the_peer() {
    let mut server = Server::new(reply(b"12345", ""), false, false);
    assert!(
        capture(
            &server.spec,
            &permissions(),
            &Limits {
                max_body_bytes: 4,
                max_archive_bytes: 16000000
            },
            &Deadline::new(5000)
        )
        .is_err()
    );
    server.join();
    let mut server = Server::new(Vec::new(), false, true);
    let start = Instant::now();
    assert!(
        capture(
            &server.spec,
            &permissions(),
            &Limits::default(),
            &Deadline::new(150)
        )
        .is_err()
    );
    server.join();
    assert!(start.elapsed() < Duration::from_secs(2));
    assert!(server.closed.load(Ordering::SeqCst));
    let mut server = Server::new(Vec::new(), false, true);
    let flag = Arc::new(AtomicBool::new(false));
    let send = flag.clone();
    let trigger = thread::spawn(move || {
        thread::sleep(Duration::from_millis(100));
        send.store(true, Ordering::SeqCst);
    });
    let d = Deadline::new(5000);
    assert!(
        Deadline::run_bounded(&d, flag, || capture(
            &server.spec,
            &permissions(),
            &Limits::default(),
            &Deadline::new(5000)
        ))
        .is_err()
    );
    trigger.join().unwrap();
    server.join();
    assert!(server.closed.load(Ordering::SeqCst));
}
#[test]
fn host_permissions_address_binding_and_special_connector_admission_precede_connections() {
    let mut spec = Spec {
        url: "http://127.0.0.1:9/file".into(),
        address: "127.0.0.1:9".parse().unwrap(),
        format: Format::Text,
        use_system_roots: false,
        root_certificates_der_base64: Vec::new(),
    };
    let error = capture(
        &spec,
        &Permissions::default(),
        &Limits::default(),
        &Deadline::new(5000),
    )
    .err()
    .unwrap();
    assert_eq!(
        error.to_string(),
        "source_acquisition_network_permission_required"
    );
    let error = capture(
        &spec,
        &Permissions {
            allow_network: true,
            allow_loopback_http: false,
        },
        &Limits::default(),
        &Deadline::new(5000),
    )
    .err()
    .unwrap();
    assert_eq!(
        error.to_string(),
        "source_acquisition_loopback_http_permission_required"
    );
    spec.address = "127.0.0.2:9".parse().unwrap();
    assert!(spec.validate().is_err());
    spec.url = "https://export.arxiv.org./api/query".into();
    spec.address = "127.0.0.1:443".parse().unwrap();
    assert_eq!(
        spec.validate().err().unwrap().to_string(),
        "source_acquisition_arxiv_connector_required"
    );
    spec.url = "http://192.0.2.1:443/file".into();
    spec.address = "192.0.2.1:443".parse().unwrap();
    assert!(spec.validate().is_err());
}
