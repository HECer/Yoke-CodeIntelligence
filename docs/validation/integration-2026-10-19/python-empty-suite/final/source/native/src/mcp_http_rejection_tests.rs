use super::*;

fn pair() -> (TcpStream, TcpStream) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (socket, _) = listener.accept().unwrap();
    (socket, peer)
}

#[test]
fn rejected_tail_cannot_consume_an_unbounded_peer_stream() {
    let (socket, mut peer) = pair();
    peer.set_write_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    peer.write_all(&[b'x'; 32_768]).unwrap();
    let deadline = Deadline::new(1000);
    let stop = AtomicBool::new(false);
    let mut bounded = BoundSocket {
        socket,
        deadline: &deadline,
        stop: &stop,
    };
    bounded.finish_rejection();
    // The extra half stays unread: shutdown cleanup must not drain an entire
    // attacker-controlled stream or treat its bytes as another HTTP request.
    bounded
        .socket
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    let mut remaining = [0u8; 16_384];
    bounded.socket.read_exact(&mut remaining).unwrap();
    assert!(remaining.iter().all(|byte| *byte == b'x'));
    peer.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
    assert_eq!(
        peer.read(&mut [0]).unwrap(),
        0,
        "Response side must be half-closed"
    );
}

#[test]
fn expired_or_stopped_rejection_shutdown_never_reads_a_queued_tail() {
    for expired in [false, true] {
        let (socket, mut peer) = pair();
        peer.write_all(b"unread").unwrap();
        let deadline = Deadline::new(if expired { 0 } else { 1000 });
        let stop = AtomicBool::new(!expired);
        let mut bounded = BoundSocket {
            socket,
            deadline: &deadline,
            stop: &stop,
        };
        bounded.finish_rejection();
        bounded
            .socket
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let mut remaining = [0; 6];
        bounded.socket.read_exact(&mut remaining).unwrap();
        assert_eq!(&remaining, b"unread");
    }
}

#[test]
fn idle_peer_cannot_extend_the_original_physical_deadline() {
    let (socket, _peer) = pair();
    let deadline = Deadline::new(20);
    let stop = AtomicBool::new(false);
    let mut bounded = BoundSocket {
        socket,
        deadline: &deadline,
        stop: &stop,
    };
    let start = Instant::now();
    bounded.finish_rejection();
    assert!(start.elapsed() < Duration::from_millis(250));
    assert!(
        deadline.check().is_err(),
        "Idle read must use the existing 20ms expiry"
    );
}
