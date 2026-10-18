use serde_json::{Value, json};
use yoke::capture::Deadline;
use yoke::github_app_auth::{AppKey, AuthProfile, Client, Options};

fn profile_value() -> Value {
    json!({"schemaVersion":1,"appId":17,"clientId":"Iv1.fixture",
        "keyCredential":"fixture-key","grants":[
            {"installationId":31,"repositoryId":47,"repository":"org/repo"}],
        "permissions":{"contents":"read","pull_requests":"write"}})
}
fn profile() -> AuthProfile {
    AuthProfile::parse(&serde_json::to_vec(&profile_value()).unwrap()).unwrap()
}
fn options() -> Options {
    Options {
        allow_network: true,
        ..Options::default()
    }
}

#[test]
fn host_profile_rejects_ambiguous_authority_and_unknown_fields() {
    assert!(AuthProfile::parse(&serde_json::to_vec(&profile_value()).unwrap()).is_ok());
    for (field, value) in [
        ("appId", json!(0)),
        ("appId", json!(9_007_199_254_740_992_u64)),
        ("clientId", json!("bad\r\nissuer")),
        ("keyCredential", json!("../key")),
        ("endpoint", json!("http://127.0.0.1")),
        ("permissions", json!({})),
        ("permissions", json!({"administration":"write"})),
        ("permissions", json!({"contents":"write"})),
    ] {
        let mut input = profile_value();
        input[field] = value;
        assert!(
            AuthProfile::parse(&serde_json::to_vec(&input).unwrap()).is_err(),
            "{field}"
        );
    }
    let mut duplicate = profile_value();
    duplicate["grants"]
        .as_array_mut()
        .unwrap()
        .push(profile_value()["grants"][0].clone());
    assert!(AuthProfile::parse(&serde_json::to_vec(&duplicate).unwrap()).is_err());
    let duplicate =
        serde_json::to_string(&profile_value())
            .unwrap()
            .replacen("{", "{\"appId\":18,", 1);
    assert!(AuthProfile::parse(duplicate.as_bytes()).is_err());
}

#[test]
fn production_permission_precedes_os_key_access() {
    let error = Client::production(profile(), Options::default(), &Deadline::new(5000))
        .err()
        .expect("network permission must refuse");
    assert_eq!(error.to_string(), "github_app_network_permission_required");
}

#[test]
fn malformed_or_encrypted_private_material_is_rejected_without_echo() {
    for input in [
        b"secret-canary".as_slice(),
        b"-----BEGIN ENCRYPTED PRIVATE KEY-----\nYQ==\n-----END ENCRYPTED PRIVATE KEY-----\n",
        &[0; 8193],
    ] {
        let error = AppKey::from_host_bytes(input).err().expect("invalid key");
        assert!(!error.to_string().contains("secret-canary"));
    }
}

#[cfg(windows)]
mod windows {
    use super::*;
    use base64::{
        Engine as _,
        prelude::{BASE64_STANDARD, BASE64_URL_SAFE_NO_PAD},
    };
    use std::{
        io::{Read, Write},
        net::{SocketAddr, TcpListener},
        process::Command,
        sync::{
            Arc, Mutex, OnceLock,
            atomic::{AtomicBool, AtomicU64, Ordering},
        },
        thread,
        time::Duration,
    };
    use yoke::github_app_auth::FixtureProfile;
    use zeroize::Zeroizing;

    const NOW: u64 = 1_700_000_000;
    const TOKEN: &str = "installation-fixture-token-canary_variable-length_2026";
    struct Keys {
        pkcs1: Zeroizing<Vec<u8>>,
        pkcs8: Zeroizing<Vec<u8>>,
        public: Vec<u8>,
    }
    fn keys() -> &'static Keys {
        static KEYS: OnceLock<Keys> = OnceLock::new();
        KEYS.get_or_init(|| {
            // Ephemeral, owned fixture only; private child stdout is never printed or persisted.
            let output = Command::new("pwsh.exe").args(["-NoProfile", "-NonInteractive", "-Command",
                "$rsa=[System.Security.Cryptography.RSA]::Create(2048); try { @{one=[Convert]::ToBase64String($rsa.ExportRSAPrivateKey());eight=[Convert]::ToBase64String($rsa.ExportPkcs8PrivateKey());public=[Convert]::ToBase64String($rsa.ExportRSAPublicKey())} | ConvertTo-Json -Compress } finally { $rsa.Dispose() }"])
                .output().expect("host PowerShell fixture generator");
            assert!(output.status.success(), "fixture generation failed");
            let stdout = Zeroizing::new(output.stdout);
            let mut value: Value = serde_json::from_slice(&stdout).expect("fixture encoding");
            let mut take = |field: &str| {
                let encoded = Zeroizing::new(value[field].take().as_str().expect("fixture field").to_owned());
                BASE64_STANDARD.decode(encoded.as_bytes()).expect("fixture base64")
            };
            Keys { pkcs1: Zeroizing::new(take("one")), pkcs8: Zeroizing::new(take("eight")), public: take("public") }
        })
    }
    fn key() -> AppKey {
        AppKey::from_host_bytes(&keys().pkcs1).unwrap()
    }
    fn pem(label: &str, der: &[u8]) -> Zeroizing<Vec<u8>> {
        Zeroizing::new(
            format!(
                "-----BEGIN {label}-----\r\n{}\r\n-----END {label}-----\r\n",
                BASE64_STANDARD.encode(der)
            )
            .into_bytes(),
        )
    }
    type Reply = (u16, Vec<(String, String)>, String);
    struct Seen {
        head: Zeroizing<String>,
        body: Zeroizing<Vec<u8>>,
    }
    struct Fixture {
        endpoint: SocketAddr,
        seen: Arc<Mutex<Vec<Seen>>>,
        stop: Arc<AtomicBool>,
        worker: Option<thread::JoinHandle<()>>,
    }
    impl Fixture {
        fn new(replies: Vec<Reply>) -> Self {
            Self::with_stalled_response(replies, None)
        }
        fn with_stalled_response(replies: Vec<Reply>, cancel: Option<Arc<AtomicBool>>) -> Self {
            Self::with_controls(replies, cancel, None)
        }
        fn with_controls(
            replies: Vec<Reply>,
            cancel: Option<Arc<AtomicBool>>,
            advance: Option<(Arc<AtomicU64>, usize, u64)>,
        ) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            listener.set_nonblocking(true).unwrap();
            let endpoint = listener.local_addr().unwrap();
            let seen = Arc::new(Mutex::new(Vec::new()));
            let target = seen.clone();
            let stop = Arc::new(AtomicBool::new(false));
            let stopped = stop.clone();
            let worker = thread::spawn(move || {
                let mut replies = replies.into_iter();
                while !stopped.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((mut stream, _)) => {
                            stream.set_nonblocking(false).unwrap();
                            stream
                                .set_read_timeout(Some(Duration::from_secs(3)))
                                .unwrap();
                            stream
                                .set_write_timeout(Some(Duration::from_secs(3)))
                                .unwrap();
                            let mut raw = Zeroizing::new(Vec::new());
                            let mut byte = [0];
                            while raw.len() < 65536 && !raw.ends_with(b"\r\n\r\n") {
                                match stream.read(&mut byte) {
                                    Ok(1) => raw.push(byte[0]),
                                    _ => break,
                                }
                            }
                            if !raw.ends_with(b"\r\n\r\n") {
                                continue;
                            }
                            let head = Zeroizing::new(
                                String::from_utf8(std::mem::take(&mut *raw)).unwrap(),
                            );
                            let length: usize = head
                                .lines()
                                .find_map(|line| {
                                    line.split_once(':')
                                        .filter(|(name, _)| {
                                            name.eq_ignore_ascii_case("content-length")
                                        })
                                        .map(|(_, v)| v.trim().parse().unwrap())
                                })
                                .unwrap_or(0);
                            assert!(length < 10000);
                            let mut body = Zeroizing::new(vec![0; length]);
                            stream.read_exact(&mut body).unwrap();
                            target.lock().unwrap().push(Seen { head, body });
                            let (status, headers, body) =
                                replies.next().unwrap_or((500, vec![], "{}".into()));
                            let mut response = format!(
                                "HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n",
                                body.len()
                            );
                            for (name, value) in headers {
                                response.push_str(&format!("{name}: {value}\r\n"));
                            }
                            response.push_str("\r\n");
                            if let Some((clock, request, next)) = &advance
                                && target.lock().unwrap().len() == *request
                            {
                                clock.store(*next, Ordering::SeqCst);
                            }
                            if let Some(cancel) = &cancel {
                                response.push('{');
                                stream.write_all(response.as_bytes()).unwrap();
                                stream.flush().unwrap();
                                // Let the client enter its next body read before cancellation.
                                thread::sleep(Duration::from_millis(100));
                                cancel.store(true, Ordering::SeqCst);
                                while !stopped.load(Ordering::Relaxed) {
                                    thread::sleep(Duration::from_millis(2));
                                }
                                continue;
                            }
                            response.push_str(&body);
                            let _ = stream.write_all(response.as_bytes());
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(2))
                        }
                        Err(_) => break,
                    }
                }
            });
            Self {
                endpoint,
                seen,
                stop,
                worker: Some(worker),
            }
        }
        fn client(&self, o: Options) -> Client {
            Client::for_fixture(
                profile(),
                key(),
                FixtureProfile::loopback(self.endpoint, NOW).unwrap(),
                o,
                &Deadline::new(10000),
            )
            .unwrap()
        }
        fn count(&self) -> usize {
            self.seen.lock().unwrap().len()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Relaxed);
            self.worker.take().unwrap().join().unwrap();
        }
    }
    fn replies() -> Vec<Reply> {
        [json!({"id":17,"client_id":"Iv1.fixture","name":"Fixture app"}),
            json!({"id":31,"app_id":17,"suspended_at":null,"suspended_by":null,"repository_selection":"all","permissions":{"contents":"read","pull_requests":"write","issues":"write","metadata":"read"}}),
            json!({"token":TOKEN,"expires_at":"2023-11-14T23:13:20Z","permissions":{"contents":"read","pull_requests":"write","metadata":"read"},"repository_selection":"selected","repositories":[{"id":47,"full_name":"Org/Repo","private":true}]}),
            json!({"total_count":1,"repositories":[{"id":47,"full_name":"Org/Repo","private":true}]})]
            .into_iter().enumerate().map(|(i,v)| (if i==2 {201} else {200}, vec![], v.to_string())).collect()
    }
    fn authorize(
        fixture: &Fixture,
        o: Options,
    ) -> anyhow::Result<yoke::github_app_auth::InstallationLease> {
        fixture.client(o).authorize(31, 47, &Deadline::new(10000))
    }
    fn jwt(head: &str) -> Zeroizing<String> {
        Zeroizing::new(
            head.lines()
                .find_map(|line| {
                    line.split_once(':')
                        .filter(|(n, _)| n.eq_ignore_ascii_case("authorization"))
                        .map(|(_, v)| v.trim().strip_prefix("Bearer ").unwrap().to_owned())
                })
                .expect("Bearer authorization"),
        )
    }
    #[test]
    fn downloaded_pkcs1_and_host_pkcs8_are_accepted_without_trailing_der() {
        for bytes in [
            &*keys().pkcs1,
            &*keys().pkcs8,
            &*pem("RSA PRIVATE KEY", &keys().pkcs1),
            &*pem("PRIVATE KEY", &keys().pkcs8),
        ] {
            assert!(AppKey::from_host_bytes(bytes).is_ok());
        }
        let mut trailing = Zeroizing::new(keys().pkcs1.to_vec());
        trailing.push(0);
        assert!(AppKey::from_host_bytes(&trailing).is_err());
        assert!(AppKey::from_host_bytes(&pem("PRIVATE KEY", &keys().pkcs1)).is_err());
    }
    #[test]
    fn rs256_authorization_binds_identity_restricted_repository_and_opaque_token() {
        let fixture = Fixture::new(replies());
        let lease = authorize(&fixture, options()).unwrap();
        let public = lease.evidence();
        assert_eq!(public["appId"], 17);
        assert_eq!(public["installationId"], 31);
        assert_eq!(public["repositoryId"], 47);
        assert_eq!(public["repository"], "org/repo");
        assert_eq!(public["installationTokenVerified"], true);
        assert_eq!(public["fixture"], true);
        assert_eq!(public["requests"], 4);
        assert_eq!(public["published"], false);
        assert!(!public.to_string().contains(TOKEN));
        assert!(!public.to_string().contains("tokenHash"));
        let seen = fixture.seen.lock().unwrap();
        assert_eq!(seen.len(), 4);
        for (request, line) in seen.iter().zip([
            "GET /app HTTP/1.1",
            "GET /app/installations/31 HTTP/1.1",
            "POST /app/installations/31/access_tokens HTTP/1.1",
            "GET /installation/repositories?per_page=100&page=1 HTTP/1.1",
        ]) {
            assert_eq!(request.head.lines().next().unwrap(), line);
            assert!(
                request
                    .head
                    .to_ascii_lowercase()
                    .contains("x-github-api-version: 2026-03-10")
            );
        }
        let bearer = jwt(&seen[0].head);
        let parts: Vec<_> = bearer.split('.').collect();
        assert_eq!(parts.len(), 3);
        let header: Value =
            serde_json::from_slice(&BASE64_URL_SAFE_NO_PAD.decode(parts[0]).unwrap()).unwrap();
        let claims: Value =
            serde_json::from_slice(&BASE64_URL_SAFE_NO_PAD.decode(parts[1]).unwrap()).unwrap();
        assert_eq!(header, json!({"alg":"RS256","typ":"JWT"}));
        assert_eq!(
            claims,
            json!({"iat":1_699_999_940_u64,"exp":1_700_000_540_u64,"iss":"Iv1.fixture"})
        );
        let signature = BASE64_URL_SAFE_NO_PAD.decode(parts[2]).unwrap();
        let verifier = ring::signature::UnparsedPublicKey::new(
            &ring::signature::RSA_PKCS1_2048_8192_SHA256,
            &keys().public,
        );
        assert!(
            verifier
                .verify(format!("{}.{}", parts[0], parts[1]).as_bytes(), &signature)
                .is_ok()
        );
        assert!(verifier.verify(b"altered.issuer", &signature).is_err());
        for request in &seen[1..3] {
            let other = jwt(&request.head);
            assert!(
                other.as_str() == bearer.as_str(),
                "installation reads and token creation require the verified app JWT"
            );
        }
        assert_eq!(
            serde_json::from_slice::<Value>(&seen[2].body).unwrap(),
            json!({"repository_ids":[47],"permissions":{"contents":"read","pull_requests":"write"}})
        );
        assert!(jwt(&seen[3].head).as_str() == TOKEN);
        assert!(!public.to_string().contains(bearer.as_str()));
    }
    #[test]
    fn identity_suspension_and_missing_permissions_stop_before_token_creation() {
        for (row, field, value, count) in [
            (0, "id", json!(18), 1),
            (0, "client_id", json!("Iv1.other"), 1),
            (1, "id", json!(32), 2),
            (1, "app_id", json!(18), 2),
            (1, "suspended_at", json!("2026-01-01T00:00:00Z"), 2),
            (
                1,
                "permissions",
                json!({"contents":"read","pull_requests":"read"}),
                2,
            ),
        ] {
            let mut data = replies();
            let mut body: Value = serde_json::from_str(&data[row].2).unwrap();
            body[field] = value;
            data[row].2 = body.to_string();
            let fixture = Fixture::new(data);
            assert!(authorize(&fixture, options()).is_err(), "{row}/{field}");
            assert_eq!(fixture.count(), count, "{row}/{field}");
        }
    }
    #[test]
    fn token_response_refuses_broadened_wrong_expired_and_header_unsafe_credentials() {
        for (field, value) in [
            ("token", json!("canary\r\nInjected: yes")),
            ("token", json!("")),
            ("token", json!("x".repeat(16_385))),
            ("expires_at", json!("2023-11-14T22:13:19Z")),
            ("expires_at", json!("2024-11-14T23:13:20Z")),
            (
                "permissions",
                json!({"contents":"read","pull_requests":"write","issues":"write"}),
            ),
            (
                "permissions",
                json!({"contents":"read","pull_requests":"write","issues":"read"}),
            ),
            ("permissions", json!({"contents":"read"})),
            (
                "permissions",
                json!({"contents":"read","pull_requests":"read"}),
            ),
            ("repository_selection", json!("all")),
            ("repositories", json!([{"id":48,"full_name":"org/repo"}])),
            ("repositories", json!([{"id":47,"full_name":"org/other"}])),
            (
                "repositories",
                json!([{"id":47,"full_name":"org/repo"},{"id":47,"full_name":"org/repo"}]),
            ),
        ] {
            let mut data = replies();
            let mut body: Value = serde_json::from_str(&data[2].2).unwrap();
            body[field] = value;
            data[2].2 = body.to_string();
            let fixture = Fixture::new(data);
            let result = authorize(&fixture, options());
            assert!(result.is_err(), "{field}");
            assert_eq!(fixture.count(), 3, "{field}");
            assert!(!result.err().unwrap().to_string().contains(TOKEN));
        }
    }
    #[test]
    fn repository_proof_requires_exact_total_id_and_name() {
        for body in [
            json!({"total_count":2,"repositories":[{"id":47,"full_name":"org/repo"}]}),
            json!({"total_count":1,"repositories":[{"id":48,"full_name":"org/repo"}]}),
            json!({"total_count":1,"repositories":[{"id":47,"full_name":"org/other"}]}),
            json!({"total_count":0,"repositories":[]}),
        ] {
            let mut data = replies();
            data[3].2 = body.to_string();
            let fixture = Fixture::new(data);
            assert!(authorize(&fixture, options()).is_err());
            assert_eq!(fixture.count(), 4);
        }
    }
    #[test]
    fn redirect_duplicates_raw_capacity_and_errors_never_publish_remote_secrets() {
        for (status, headers, body) in [
            (
                302,
                vec![("Location".into(), "http://127.0.0.1:9/leak".into())],
                "{}".into(),
            ),
            (200, vec![], "{\"id\":17,\"id\":18}".into()),
            (500, vec![], TOKEN.into()),
            (200, vec![], " ".repeat(64_001)),
        ] {
            let fixture = Fixture::new(vec![(status, headers, body)]);
            let mut o = options();
            o.max_response_bytes = 64_000;
            let result = authorize(&fixture, o);
            assert!(result.is_err());
            assert_eq!(fixture.count(), 1);
            assert!(!result.err().unwrap().to_string().contains(TOKEN));
        }
    }
    #[test]
    fn zero_budgets_and_cancelled_deadline_refuse_before_io() {
        for field in 0..4 {
            let fixture = Fixture::new(replies());
            let mut o = options();
            match field {
                0 => o.max_requests = 0,
                1 => o.max_response_bytes = 0,
                2 => o.max_total_bytes = 0,
                _ => o.max_output_bytes = 0,
            }
            let result = Client::for_fixture(
                profile(),
                key(),
                FixtureProfile::loopback(fixture.endpoint, NOW).unwrap(),
                o,
                &Deadline::new(10000),
            );
            assert!(result.is_err());
            assert_eq!(fixture.count(), 0);
        }
        let fixture = Fixture::new(replies());
        let mut client = fixture.client(options());
        assert!(client.authorize(31, 47, &Deadline::new(0)).is_err());
        assert_eq!(fixture.count(), 0);
        assert!(client.authorize(31, 48, &Deadline::new(10000)).is_err());
        assert_eq!(fixture.count(), 0);
    }
    #[test]
    fn positive_request_aggregate_and_output_limits_are_enforced() {
        let fixture = Fixture::new(replies());
        let mut o = options();
        o.max_requests = 3;
        assert!(
            Client::for_fixture(
                profile(),
                key(),
                FixtureProfile::loopback(fixture.endpoint, NOW).unwrap(),
                o,
                &Deadline::new(10000)
            )
            .is_err()
        );
        assert_eq!(fixture.count(), 0);
        let fixture = Fixture::new(replies());
        let mut o = options();
        o.max_total_bytes = 1;
        assert!(authorize(&fixture, o).is_err());
        assert_eq!(fixture.count(), 1);
        let fixture = Fixture::new(replies());
        let mut o = options();
        o.max_output_bytes = 1;
        assert!(authorize(&fixture, o).is_err());
        assert_eq!(fixture.count(), 4);
        let fixture = Fixture::new(replies());
        let lease = authorize(&fixture, options()).unwrap();
        let size = serde_json::to_vec(lease.evidence()).unwrap().len() + 1;
        let exact = Fixture::new(replies());
        let mut o = options();
        o.max_output_bytes = size;
        assert!(authorize(&exact, o).is_ok());
        let short = Fixture::new(replies());
        let mut o = options();
        o.max_output_bytes = size - 1;
        assert!(authorize(&short, o).is_err());
    }
    #[test]
    fn inherited_cancellation_interrupts_a_stalled_response_and_keeps_its_cause() {
        let cancel = Arc::new(AtomicBool::new(false));
        let fixture = Fixture::with_stalled_response(replies(), Some(cancel.clone()));
        let mut client = fixture.client(options());
        let mut inner_cancelled = false;
        let started = std::time::Instant::now();
        let result = Deadline::run_bounded(&Deadline::new(5000), cancel, || {
            let result = client.authorize(31, 47, &Deadline::new(10000));
            inner_cancelled = result
                .as_ref()
                .err()
                .is_some_and(|e| e.to_string().contains("cancelled"));
            result.map(|_| ())
        });
        assert!(
            inner_cancelled,
            "authorize must preserve the inherited cancellation cause"
        );
        assert!(result.unwrap_err().to_string().contains("cancelled"));
        assert!(
            started.elapsed() < Duration::from_millis(750),
            "cancelled I/O must not wait for its 5s deadline"
        );
        assert_eq!(fixture.count(), 1);
    }
    #[test]
    fn token_lifetime_is_rechecked_after_the_last_repository_reply() {
        for after in [NOW + 2, NOW + 3600] {
            let clock = Arc::new(AtomicU64::new(NOW));
            let mut data = replies();
            let mut token: Value = serde_json::from_str(&data[2].2).unwrap();
            token["expires_at"] = json!("2023-11-14T22:13:51Z");
            data[2].2 = token.to_string();
            let fixture = Fixture::with_controls(data, None, Some((clock.clone(), 4, after)));
            let mut client = Client::for_fixture(
                profile(),
                key(),
                FixtureProfile::loopback_with_clock(fixture.endpoint, clock).unwrap(),
                options(),
                &Deadline::new(10000),
            )
            .unwrap();
            assert!(client.authorize(31, 47, &Deadline::new(10000)).is_err());
            assert_eq!(fixture.count(), 4);
        }
    }
    #[test]
    fn synchronously_created_client_refuses_later_async_use_without_panicking() {
        let fixture = Fixture::new(replies());
        let mut client = fixture.client(options());
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let error = client
                .authorize(31, 47, &Deadline::new(10000))
                .err()
                .expect("async use must refuse");
            assert_eq!(error.to_string(), "github_app_requires_synchronous_host");
        });
        assert_eq!(fixture.count(), 0);
        runtime.block_on(async move {
            drop(client);
        });
    }
}
