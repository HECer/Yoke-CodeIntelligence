use super::*;

fn pair() -> (TcpStream, TcpStream) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let (server, _) = listener.accept().unwrap();
    (client, server)
}

#[test]
fn delayed_work_expiry_still_delivers_a_complete_refusal_within_the_physical_budget() {
    let (mut client, server) = pair();
    let stop = AtomicBool::new(false);
    let (physical, work) = connection_deadlines(1800);
    let mut channel = Channel::Plain(BoundSocket {
        socket: server,
        deadline: &physical,
        stop: &stop,
    });
    // Simulate a network thread resuming 150ms after the work deadline. The
    // previous 100ms reserve left no physical budget to write even a refusal.
    thread::sleep(work.remaining() + Duration::from_millis(150));
    assert!(work.check().is_err());
    send_reply(
        &mut channel,
        Reply::fault(Fault(503, "MCP request deadline or shutdown")),
        &physical,
        &stop,
    )
    .expect("Expired work must leave time for the bounded HTTP refusal");
    physical.check().unwrap();
    drop(channel);
    let mut bytes = vec![];
    client.read_to_end(&mut bytes).unwrap();
    let boundary = bytes.windows(4).position(|b| b == b"\r\n\r\n").unwrap() + 4;
    let headers = std::str::from_utf8(&bytes[..boundary]).unwrap();
    assert!(headers.starts_with("HTTP/1.1 503 "));
    let length: usize = headers
        .lines()
        .find_map(|line| {
            line.split_once(':')
                .filter(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                .map(|(_, value)| value.trim().parse().unwrap())
        })
        .unwrap();
    assert_eq!(bytes.len(), boundary + length);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&bytes[boundary..]).unwrap(),
        json!({"error":"MCP request deadline or shutdown"})
    );
}

#[test]
fn expired_or_cancelled_physical_budget_cannot_be_renewed_for_an_error_reply() {
    for cancelled in [false, true] {
        let (mut client, server) = pair();
        let stop = AtomicBool::new(cancelled);
        let physical = Deadline::new(if cancelled { 5000 } else { 0 });
        let mut channel = Channel::Plain(BoundSocket {
            socket: server,
            deadline: &physical,
            stop: &stop,
        });
        assert!(
            send_reply(
                &mut channel,
                Reply::fault(Fault(503, "Expired")),
                &physical,
                &stop
            )
            .is_err()
        );
        drop(channel);
        let mut bytes = vec![];
        client.read_to_end(&mut bytes).unwrap();
        assert!(bytes.is_empty());
    }
}
