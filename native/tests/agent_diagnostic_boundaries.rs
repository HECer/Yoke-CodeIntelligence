use serde_json::{Value, json};
use std::{
    fs, io,
    net::{Ipv4Addr, UdpSocket},
    path::Path,
    time::Instant,
};
use yoke::{
    Engine, agent_repl,
    capture::Deadline,
    project_session::{HostProfile, Session},
};

const PHASE_NAMES: [&str; 10] = [
    "startup",
    "first-input",
    "initialize",
    "initialize-reply-queued",
    "did-open",
    "diagnostics",
    "diagnostics-reply-queued",
    "shutdown",
    "shutdown-reply-queued",
    "exit",
];

fn decode_fixture_phase(packet: &[u8], nonce: &[u8; 16]) -> Option<(usize, u32)> {
    if packet.len() != 22 || packet[0] != 1 || packet[1..17] != nonce[..] {
        return None;
    }
    let phase = usize::from(packet[17]);
    let elapsed = u32::from_le_bytes(packet[18..22].try_into().ok()?);
    (phase < PHASE_NAMES.len() && elapsed <= 600_000).then_some((phase, elapsed))
}

struct FixturePhases {
    sink: Option<(UdpSocket, u16)>,
    nonce: [u8; 16],
}
impl FixturePhases {
    fn new() -> Self {
        let mut nonce = [0; 16];
        let sink = (|| {
            getrandom::fill(&mut nonce).ok()?;
            let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).ok()?;
            socket.set_nonblocking(true).ok()?;
            let port = socket.local_addr().ok()?.port();
            Some((socket, port))
        })();
        Self { sink, nonce }
    }
    fn arguments(&self) -> Vec<String> {
        self.sink.as_ref().map_or_else(Vec::new, |(_, port)| {
            vec![port.to_string(), hex::encode(self.nonce)]
        })
    }
    fn sample(&self) -> Value {
        let Some((socket, _)) = &self.sink else {
            return json!({"available":false});
        };
        // Failure-only, nonblocking and finite. UDP loss, unavailable setup or
        // invalid packets never alter the LSP result or extend its deadline.
        let mut first = [None; 10];
        let mut packets = 0;
        let mut discarded = 0;
        let mut io_error = None;
        for _ in 0..32 {
            let mut packet = [0; 23];
            match socket.recv_from(&mut packet) {
                Ok((length, peer)) => {
                    packets += 1;
                    let observation = if peer.ip().is_loopback() {
                        decode_fixture_phase(&packet[..length], &self.nonce)
                    } else {
                        None
                    };
                    if let Some((phase, elapsed)) = observation {
                        first[phase] = first[phase].or(Some(elapsed));
                    } else {
                        discarded += 1;
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) => {
                    io_error = Some(format!("{:?}", error.kind()));
                    break;
                }
            }
        }
        let phases: Vec<_> = PHASE_NAMES.iter().zip(first).collect();
        json!({"available":true,"firstElapsedMs":phases,"packets":packets,
               "discarded":discarded,"capacityReached":packets==32,"ioError":io_error})
    }
}

#[test]
fn phase_datagrams_reject_malformed_foreign_and_out_of_budget_observations() {
    let nonce = [19; 16];
    let mut frame = [0; 22];
    frame[0] = 1;
    frame[1..17].copy_from_slice(&nonce);
    frame[17] = 5;
    frame[18..].copy_from_slice(&125u32.to_le_bytes());
    assert_eq!(decode_fixture_phase(&frame, &nonce), Some((5, 125)));
    for length in 0..22 {
        assert_eq!(decode_fixture_phase(&frame[..length], &nonce), None);
    }
    let mut oversized = frame.to_vec();
    oversized.push(0);
    assert_eq!(decode_fixture_phase(&oversized, &nonce), None);
    assert_eq!(decode_fixture_phase(&frame, &[20; 16]), None);
    for (slot, value) in [(0, 0), (0, 2), (17, 10), (17, 255)] {
        let mut malformed = frame;
        malformed[slot] = value;
        assert_eq!(decode_fixture_phase(&malformed, &nonce), None);
    }
    frame[18..].copy_from_slice(&600_000u32.to_le_bytes());
    assert_eq!(decode_fixture_phase(&frame, &nonce), Some((5, 600_000)));
    for elapsed in [600_001u32, u32::MAX] {
        frame[18..].copy_from_slice(&elapsed.to_le_bytes());
        assert_eq!(decode_fixture_phase(&frame, &nonce), None);
    }
}

fn invoke_method(source: &str, method: &str, args: Value) -> Value {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("main.rs"), source).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let observer = FixturePhases::new();
    let mut server_args = vec![
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/diagnostic-boundaries-lsp.mjs")
            .display()
            .to_string(),
    ];
    server_args.extend(observer.arguments());
    engine
        .register_configured_server(yoke::lsp::ConfiguredServer {
            server: yoke::lsp::Server {
                command: "node".into(),
                args: server_args,
                language: "rust".into(),
                trust: true,
            },
            configuration: yoke::lsp::HostConfiguration {
                passive_profile: Some(yoke::lsp::PassiveProfile::ContractFixture),
                ..Default::default()
            },
        })
        .unwrap();
    let deadline = Deadline::new(20000);
    let started = Instant::now();
    let selected_lines = (
        args.get("start_line").and_then(Value::as_u64),
        args.get("end_line").and_then(Value::as_u64),
    );
    let mut session = Session::open(
        engine,
        HostProfile::default(),
        Some("diagnostic-eof"),
        &deadline,
    )
    .unwrap_or_else(|error| {
        let fixture_phases = observer.sample();
        panic!(
            "diagnostic fixture failed; phase=session-open; method={method}; selected_lines={selected_lines:?}; elapsed={:?}; remaining={:?}; fixture_phases={fixture_phases}; error={error:#}",
            started.elapsed(),
            deadline.remaining()
        )
    });
    let opened_elapsed = started.elapsed();
    let opened_remaining = deadline.remaining();
    agent_repl::invoke_facade(&mut session, method, args, None, &deadline).unwrap_or_else(|error| {
        let fixture_phases = observer.sample();
        panic!(
            "diagnostic fixture failed; phase=facade; method={method}; selected_lines={selected_lines:?}; opened_elapsed={opened_elapsed:?}; opened_remaining={opened_remaining:?}; elapsed={:?}; remaining={:?}; fixture_phases={fixture_phases}; error={error:#}",
            started.elapsed(),
            deadline.remaining()
        )
    })
}
fn invoke(source: &str, args: Value) -> Value {
    invoke_method(source, "lsp.get_diagnostics_for_file", args)
}

#[test]
fn whole_file_diagnostics_keep_valid_zero_width_eof_points_including_empty_source() {
    for source in ["", "fn selected() {}\nfn next() {}\n"] {
        let value = invoke(source, json!({"relative_path":"main.rs"}));
        assert!(
            value["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item["startByte"] == source.len() && item["endByte"] == source.len()),
            "Valid EOF diagnostic lost for {} bytes: {value}",
            source.len()
        );
    }
}

#[test]
fn partial_line_diagnostics_do_not_admit_the_next_line_point() {
    let value = invoke(
        "fn selected() {}\nfn next() {}\n",
        json!({"relative_path":"main.rs","start_line":0,"end_line":0}),
    );
    assert!(
        value["items"].as_array().unwrap().is_empty(),
        "Next line diagnostic escaped selected line: {value}"
    );
}

#[test]
fn selecting_lines_before_the_trailing_empty_line_excludes_its_eof_point() {
    let value = invoke(
        "fn selected() {}\nfn next() {}\n",
        json!({"relative_path":"main.rs","start_line":0,"end_line":1}),
    );
    assert_eq!(value["items"].as_array().unwrap().len(), 1);
    assert_eq!(
        value["items"][0]["message"],
        "Next line declaration warning"
    );
}

#[test]
fn independent_inspection_line_ranges_exclude_the_next_line_but_keep_full_file_eof() {
    let source = "fn selected() {}\nfn next() {}\n";
    let partial = invoke_method(
        source,
        "jb.run_inspections",
        json!({"relative_path":"main.rs","start_line":0,"end_line":0}),
    );
    assert!(
        partial["items"].as_array().unwrap().is_empty(),
        "Next line inspection escaped scope: {partial}"
    );
    let full = invoke_method(
        source,
        "jb.run_inspections",
        json!({"relative_path":"main.rs"}),
    );
    assert_eq!(full["items"].as_array().unwrap().len(), 2);
}
