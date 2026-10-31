//! Best-effort owned-fixture diagnostics, never protocol or acceptance authority.
use serde::Deserialize;
use std::{io, net::UdpSocket, time::Instant};

const PACKET_BYTES: usize = 256;
const MAX_PACKETS: usize = 64;
const MAX_BYTES: usize = 8192;

pub(super) struct Observations {
    socket: UdpSocket,
    nonce: String,
    port: u16,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Phase {
    Started,
    Received,
    ReplyQueued,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Method {
    Startup,
    Initialize,
    Initialized,
    DidOpen,
    DocumentSymbol,
    Hover,
    Shutdown,
    Exit,
    Other,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Packet {
    nonce: String,
    worker: u32,
    phase: Phase,
    method: Method,
    time_ms: u64,
}

#[derive(Debug)]
pub(super) struct Summary {
    phases: Vec<(u32, Phase, Method, u64)>,
    rejected: usize,
    budget_reached: bool,
    read_unavailable: bool,
}

fn decode(bytes: &[u8], nonce: &str) -> Option<(u32, Phase, Method, u64)> {
    if bytes.is_empty() || bytes.len() >= PACKET_BYTES {
        return None;
    }
    let packet: Packet = serde_json::from_slice(bytes).ok()?;
    if packet.nonce != nonce || packet.worker == 0 || packet.time_ms > 600_000 {
        return None;
    }
    Some((packet.worker, packet.phase, packet.method, packet.time_ms))
}

impl Observations {
    pub(super) fn new() -> io::Result<Self> {
        let socket = UdpSocket::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
        socket.set_nonblocking(true)?;
        let port = socket.local_addr()?.port();
        // Correlation only: UDP observations never grant authority or prove completion.
        let nonce = yoke::model::hash(format!(
            "owned-lsp-observations:{}:{:?}",
            port,
            Instant::now()
        ));
        Ok(Self {
            socket,
            nonce,
            port,
        })
    }

    pub(super) fn arguments(&self) -> [String; 2] {
        [self.port.to_string(), self.nonce.clone()]
    }

    pub(super) fn collect(&self) -> Summary {
        let mut summary = Summary {
            phases: vec![],
            rejected: 0,
            budget_reached: false,
            read_unavailable: false,
        };
        let mut total = 0;
        for _ in 0..MAX_PACKETS {
            if MAX_BYTES - total < PACKET_BYTES {
                summary.budget_reached = true;
                return summary;
            }
            let mut buffer = [0; PACKET_BYTES];
            match self.socket.recv_from(&mut buffer) {
                Ok((count, peer)) => {
                    total += count;
                    if peer.ip() != std::net::Ipv4Addr::LOCALHOST {
                        summary.rejected += 1;
                    } else if let Some(phase) = decode(&buffer[..count], &self.nonce) {
                        summary.phases.push(phase);
                    } else {
                        summary.rejected += 1;
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => return summary,
                Err(_) => {
                    summary.read_unavailable = true;
                    return summary;
                }
            }
        }
        summary.budget_reached = true;
        summary
    }
}

#[test]
fn closed_phase_packets_drop_nonce_and_never_retain_protocol_content() {
    let nonce = "a".repeat(64);
    let bytes = serde_json::to_vec(&serde_json::json!({
        "nonce":nonce,"worker":7,"phase":"reply_queued","method":"hover","timeMs":19
    }))
    .unwrap();
    let phase = decode(&bytes, &nonce).unwrap();
    assert!(matches!(phase, (7, Phase::ReplyQueued, Method::Hover, 19)));
    let rendered = format!("{phase:?}");
    assert!(!rendered.contains(&nonce));
    assert!(!rendered.contains("contents"));
}

#[test]
fn malformed_foreign_oversized_and_open_diagnostic_fields_are_rejected() {
    let nonce = "a".repeat(64);
    let base = serde_json::json!({
        "nonce":nonce,"worker":7,"phase":"received","method":"initialize","timeMs":0
    });
    for (key, value) in [
        ("nonce", serde_json::json!("b".repeat(64))),
        ("worker", serde_json::json!(0)),
        ("worker", serde_json::json!(u64::MAX)),
        ("phase", serde_json::json!("source 🌍")),
        ("method", serde_json::json!("secret\nsource")),
        ("timeMs", serde_json::json!(600001)),
        ("timeMs", serde_json::json!(-1)),
        ("timeMs", serde_json::json!(0.5)),
        ("contents", serde_json::json!("arbitrary provider content")),
    ] {
        let mut forged = base.clone();
        forged[key] = value;
        assert!(decode(&serde_json::to_vec(&forged).unwrap(), &nonce).is_none());
    }
    let original = serde_json::to_string(&base).unwrap();
    let duplicate = original.replacen('{', "{\"timeMs\":0,", 1);
    assert!(decode(duplicate.as_bytes(), &nonce).is_none());
    for bytes in [vec![], vec![0xff], vec![b' '; PACKET_BYTES]] {
        assert!(decode(&bytes, &nonce).is_none());
    }
}

#[test]
fn empty_observer_collects_without_waiting_for_a_worker() {
    let observer = Observations::new().unwrap();
    let started = Instant::now();
    let summary = observer.collect();
    assert!(summary.phases.is_empty());
    assert_eq!(summary.rejected, 0);
    assert!(!summary.budget_reached && !summary.read_unavailable);
    assert!(started.elapsed() < std::time::Duration::from_millis(300));
}
