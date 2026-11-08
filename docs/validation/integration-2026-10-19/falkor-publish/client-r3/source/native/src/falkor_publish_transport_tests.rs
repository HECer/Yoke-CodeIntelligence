use super::*;
use std::{net::TcpListener, thread, time::Instant};
fn fixture(
    auth_response: &'static [u8],
    query_response: &'static [u8],
    stall: bool,
) -> (Target, thread::JoinHandle<usize>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let worker = thread::spawn(move || {
        let until = Instant::now() + Duration::from_secs(3);
        let mut stream = loop {
            match listener.accept() {
                Ok((s, _)) => break s,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(Instant::now() < until);
                    thread::sleep(Duration::from_millis(2));
                }
                Err(e) => panic!("{e}"),
            }
        };
        stream.set_nonblocking(false).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let d = Deadline::new(2000);
        let auth = falkor_wire::read(&mut stream, &Default::default(), &d).unwrap();
        assert_eq!(
            auth,
            Value::Array(Some(vec![
                Value::Bulk(Some(b"AUTH".to_vec())),
                Value::Bulk(Some(b"default".to_vec())),
                Value::Bulk(Some("private \u{1f4a1}".as_bytes().to_vec()))
            ]))
        );
        stream.write_all(auth_response).unwrap();
        if auth_response == b"+OK\r\n" {
            let query = falkor_wire::read(&mut stream, &Default::default(), &d).unwrap();
            assert_eq!(
                query,
                Value::Array(Some(vec![
                    Value::Bulk(Some(b"GRAPH.RO_QUERY".to_vec())),
                    Value::Bulk(Some(b"owned".to_vec())),
                    Value::Bulk(Some(
                        "CYPHER value=\"quotes ' \\\" \\n \u{1f4a1}\" RETURN $value"
                            .as_bytes()
                            .to_vec()
                    ))
                ]))
            );
            if stall {
                thread::sleep(Duration::from_millis(150));
            } else {
                let _ = stream.write_all(query_response);
            }
        }
        drop(stream);
        let quiet = Instant::now() + Duration::from_millis(80);
        let mut count = 1;
        while Instant::now() < quiet {
            if listener.accept().is_ok() {
                count += 1
            }
            thread::sleep(Duration::from_millis(2));
        }
        count
    });
    (
        Target {
            base_url: format!("redis://{address}/"),
            address,
            namespace: "owned".into(),
            user: "default".into(),
            use_system_roots: false,
            root_certificates_der_base64: vec![],
        },
        worker,
    )
}
fn query() -> Vec<Vec<u8>> {
    vec![
        b"GRAPH.RO_QUERY".to_vec(),
        b"owned".to_vec(),
        "CYPHER value=\"quotes ' \\\" \\n \u{1f4a1}\" RETURN $value"
            .as_bytes()
            .to_vec(),
    ]
}
#[test]
fn native_connection_preserves_unicode_auth_and_binary_frame_shapes_without_reconnect() {
    let (target, worker) = fixture(b"+OK\r\n", b"*3\r\n:7\r\n$-1\r\n$0\r\n\r\n", false);
    let d = Deadline::new(2000);
    let mut session = Session::with_secret(&target, "private \u{1f4a1}", &d).unwrap();
    assert_eq!(
        session.send(&query(), &d).unwrap(),
        Value::Array(Some(vec![
            Value::Integer(7),
            Value::Bulk(None),
            Value::Bulk(Some(vec![]))
        ]))
    );
    drop(session);
    assert_eq!(worker.join().unwrap(), 1);
}
#[test]
fn authentication_and_query_errors_do_not_echo_secrets_or_retry() {
    let (target, worker) = fixture(b"-private \xf0\x9f\x92\xa1\r\n", b"", false);
    let d = Deadline::new(2000);
    let error = Session::with_secret(&target, "private \u{1f4a1}", &d)
        .err()
        .unwrap()
        .to_string();
    assert!(!error.contains("private") && !error.contains('\u{1f4a1}'));
    assert_eq!(worker.join().unwrap(), 1);
    for response in [
        b"-private \xf0\x9f\x92\xa1\r\n".as_slice(),
        b"*1\r\n-private\r\n",
        b"$32000001\r\n",
        b"*2\r\n:1\r\n",
        b"?private\r\n",
    ] {
        let (target, worker) = fixture(b"+OK\r\n", response, false);
        let d = Deadline::new(2000);
        let mut session = Session::with_secret(&target, "private \u{1f4a1}", &d).unwrap();
        let error = session.send(&query(), &d).err().unwrap().to_string();
        assert!(!error.contains("private"));
        drop(session);
        assert_eq!(worker.join().unwrap(), 1);
    }
}
#[test]
fn stalled_query_keeps_the_original_deadline_and_never_reissues_the_request() {
    let (target, worker) = fixture(b"+OK\r\n", b"", true);
    let d = Deadline::new(80);
    let started = Instant::now();
    let mut session = Session::with_secret(&target, "private \u{1f4a1}", &d).unwrap();
    assert!(session.send(&query(), &d).is_err());
    assert!(started.elapsed() < Duration::from_secs(1));
    drop(session);
    assert_eq!(worker.join().unwrap(), 1);
}
#[test]
fn missing_host_permissions_and_expired_deadline_open_no_connection() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let target = Target {
        base_url: format!("redis://{address}/"),
        address,
        namespace: "owned".into(),
        user: "default".into(),
        use_system_roots: false,
        root_certificates_der_base64: vec![],
    };
    for p in [
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
        assert!(Session::open(&target, &p, &Deadline::new(1000)).is_err());
    }
    assert!(Session::with_secret(&target, "private", &Deadline::new(0)).is_err());
    assert!(matches!(listener.accept(),Err(e)if e.kind()==std::io::ErrorKind::WouldBlock));
}
