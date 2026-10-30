//! Owned arxiv.org-name TLS endpoints; no public service requests.
use base64::Engine as _;
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};
use yoke::{
    arxiv_source::{self, Permissions, Representation, Spec},
    capture::Deadline,
    source_acquisition::Limits,
};
struct Server {
    spec: Spec,
    request: Arc<Mutex<Vec<u8>>>,
    worker: Option<thread::JoinHandle<()>>,
    _dir: tempfile::TempDir,
}
impl Server {
    fn new(body: &[u8], pdf: bool, status: u16, stall: bool, name: &str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let cert = rcgen::generate_simple_self_signed(vec![name.into()]).unwrap();
        let config = rustls::ServerConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(
            vec![cert.cert.der().clone()],
            rustls::pki_types::PrivatePkcs8KeyDer::from(cert.key_pair.serialize_der()).into(),
        )
        .unwrap();
        let root = base64::prelude::BASE64_STANDARD.encode(cert.cert.der());
        let reply=[format!("HTTP/1.1 {status} Owned\r\nContent-Type: {}\r\nContent-Length: {}\r\nLocation: https://elsewhere.invalid/\r\nConnection: close\r\n\r\n",if pdf {"application/pdf"}else{"text/html; charset=utf-8"},body.len()).as_bytes(),body].concat();
        let request = Arc::new(Mutex::new(Vec::new()));
        let recorded = request.clone();
        let worker = thread::spawn(move || {
            let start = Instant::now();
            let stream = loop {
                match listener.accept() {
                    Ok((s, _)) => break s,
                    Err(e)
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            && start.elapsed() < Duration::from_secs(2) =>
                    {
                        thread::sleep(Duration::from_millis(5))
                    }
                    _ => return,
                }
            };
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            stream
                .set_write_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut io = rustls::StreamOwned::new(
                rustls::ServerConnection::new(Arc::new(config)).unwrap(),
                stream,
            );
            let mut bytes = Vec::new();
            let mut buf = [0; 1024];
            while !bytes.windows(4).any(|v| v == b"\r\n\r\n") {
                match io.read(&mut buf) {
                    Ok(0) | Err(_) => return,
                    Ok(n) => bytes.extend_from_slice(&buf[..n]),
                };
                if bytes.len() > 32000 {
                    return;
                }
            }
            *recorded.lock().unwrap() = bytes;
            if stall {
                let _ = io.read(&mut buf);
            } else {
                let _ = io.write_all(&reply);
                let _ = io.flush();
            }
        });
        let dir = tempfile::tempdir().unwrap();
        Self {
            spec: Spec {
                id: "2301.00001v1".into(),
                address,
                coordination_file: dir.path().join("arxiv-rate-v1.bin"),
                representation: if pdf {
                    Representation::Pdf
                } else {
                    Representation::Metadata
                },
                use_system_roots: false,
                root_certificates_der_base64: vec![root],
            },
            request,
            worker: Some(worker),
            _dir: dir,
        }
    }
    fn join(&mut self) {
        self.worker.take().unwrap().join().unwrap();
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        if let Some(w) = self.worker.take() {
            w.join().unwrap();
        }
    }
}
fn grant() -> Permissions {
    Permissions {
        allow_network: true,
        allow_rate_coordination: true,
    }
}
const META:&[u8]=b"<head><meta name=citation_title content=owned><meta name=citation_arxiv_id content=2301.00001v1><meta name=citation_author content=label></head>";
#[test]
fn literal_tls_capture_preserves_body_and_shared_gate_refuses_next_request() {
    let mut s = Server::new(META, false, 200, false, "arxiv.org");
    for p in [
        Permissions::default(),
        Permissions {
            allow_network: true,
            allow_rate_coordination: false,
        },
    ] {
        assert!(
            arxiv_source::capture(&s.spec, &p, &Limits::default(), &Deadline::new(3000)).is_err()
        );
        assert!(!s.spec.coordination_file.exists());
    }
    let got =
        arxiv_source::capture(&s.spec, &grant(), &Limits::default(), &Deadline::new(3000)).unwrap();
    s.join();
    assert_eq!(got.body(), META);
    let request = String::from_utf8(s.request.lock().unwrap().clone()).unwrap();
    assert!(request.starts_with("GET /abs/2301.00001v1 HTTP/1.1"));
    assert!(!request.to_ascii_lowercase().contains("authorization:"));
    let status =
        arxiv_source::rate_status(&s.spec.coordination_file, &Deadline::new(3000)).unwrap();
    assert_eq!(status["sequence"], 2);
    assert_eq!(
        got.capture_claims().unwrap()["rateCoordination"]["sequence"],
        2
    );
    assert!(
        arxiv_source::capture(&s.spec, &grant(), &Limits::default(), &Deadline::new(3000)).is_err()
    );
}
#[test]
fn pdf_route_remains_received_pdf_bytes_without_metadata_synthesis() {
    let pdf = b"%PDF-1.4\nowned finite bytes\n%%EOF\n";
    let mut s = Server::new(pdf, true, 200, false, "arxiv.org");
    let got =
        arxiv_source::capture(&s.spec, &grant(), &Limits::default(), &Deadline::new(3000)).unwrap();
    s.join();
    assert_eq!(got.body(), pdf);
    assert!(
        String::from_utf8(s.request.lock().unwrap().clone())
            .unwrap()
            .starts_with("GET /pdf/2301.00001v1 HTTP/1.1")
    );
    assert!(arxiv_source::inspect(&got, 32000, &Deadline::new(3000)).is_err());
}
#[test]
fn mismatched_selector_redirect_and_wrong_tls_name_refuse_and_complete_owned_attempt() {
    for (body, status, name) in [
        (
            b"<head><meta name=citation_arxiv_id content=2301.00002></head>".as_slice(),
            200,
            "arxiv.org",
        ),
        (META, 302, "arxiv.org"),
        (META, 200, "wrong.invalid"),
    ] {
        let mut s = Server::new(body, false, status, false, name);
        assert!(
            arxiv_source::capture(&s.spec, &grant(), &Limits::default(), &Deadline::new(3000))
                .is_err()
        );
        s.join();
        assert_eq!(
            arxiv_source::rate_status(&s.spec.coordination_file, &Deadline::new(3000)).unwrap()["phase"],
            "completed"
        );
    }
}
#[test]
fn stalled_owned_connection_stops_on_original_deadline_and_releases_rate_lock() {
    let mut s = Server::new(META, false, 200, true, "arxiv.org");
    let start = Instant::now();
    assert!(
        arxiv_source::capture(&s.spec, &grant(), &Limits::default(), &Deadline::new(300)).is_err()
    );
    assert!(start.elapsed() < Duration::from_secs(2));
    s.join();
    assert_eq!(
        arxiv_source::rate_status(&s.spec.coordination_file, &Deadline::new(3000)).unwrap()["phase"],
        "completed"
    );
}
