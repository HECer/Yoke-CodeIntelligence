use yoke::credentials;

#[test]
fn credential_names_cannot_escape_the_federation_namespace() {
    for name in [
        "",
        ".",
        "..",
        "a/b",
        "a\\b",
        "a:b",
        "a b",
        "a\nb",
        "_sender",
        "-sender",
        ".sender",
        "a\0b",
        "ä",
        "yoke-code-intelligence/federation/other",
        "codex-agent-mail/devtesting@totalcreations.de",
        "codex-agent-google/hfAppTesting@googlemail.com",
    ] {
        assert!(
            credentials::target_name(name).is_err(),
            "accepted invalid name"
        );
        assert!(
            credentials::generate(name).is_err(),
            "generated an invalid name"
        );
        // Never format a successful read result: it contains private seed bytes.
        assert!(credentials::read(name).is_err(), "read an invalid name");
    }
    assert!(credentials::target_name(&"a".repeat(81)).is_err());
}

#[test]
fn ordinary_names_resolve_only_to_the_federation_namespace() {
    for (name, expected) in [
        ("sender", "yoke-code-intelligence/federation/sender"),
        (
            "Sender_1.test-key",
            "yoke-code-intelligence/federation/Sender_1.test-key",
        ),
    ] {
        assert_eq!(
            credentials::target_name(name).expect("valid name"),
            expected
        );
    }
    assert!(credentials::target_name(&"a".repeat(80)).is_ok());
}

#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
#[test]
fn unsupported_hosts_do_not_use_a_plaintext_fallback() {
    for error in [
        credentials::generate("sender").err().expect("unavailable"),
        credentials::read("sender").err().expect("unavailable"),
    ] {
        assert!(error.to_string().contains("unavailable"));
    }
}

#[cfg(windows)]
mod windows_gate {
    use super::credentials;
    use ed25519_dalek::SigningKey;
    use std::sync::{
        Arc, Barrier,
        atomic::{AtomicBool, Ordering},
    };
    use zeroize::Zeroizing;

    #[link(name = "Advapi32")]
    unsafe extern "system" {
        fn CredDeleteW(target: *const u16, kind: u32, flags: u32) -> i32;
    }

    struct CreatedEntry {
        target: Vec<u16>,
        armed: Arc<AtomicBool>,
    }
    impl CreatedEntry {
        fn new(name: &str) -> Self {
            let target = credentials::target_name(name).expect("generated test name");
            assert!(name.starts_with("integration-test-"));
            Self {
                target: target.encode_utf16().chain(Some(0)).collect(),
                armed: Arc::new(AtomicBool::new(false)),
            }
        }
        fn delete(self) {
            // The target is exactly this test's random entry; never enumerate credentials.
            let deleted = unsafe { CredDeleteW(self.target.as_ptr(), 1, 0) };
            assert_ne!(deleted, 0, "test credential cleanup failed");
            self.armed.store(false, Ordering::SeqCst);
        }
    }
    impl Drop for CreatedEntry {
        fn drop(&mut self) {
            if self.armed.load(Ordering::SeqCst) {
                // Best-effort cleanup on assertion/unwind; the normal path checks deletion.
                unsafe {
                    CredDeleteW(self.target.as_ptr(), 1, 0);
                }
            }
        }
    }
    fn test_name() -> String {
        let mut nonce = [0_u8; 16];
        getrandom::fill(&mut nonce).expect("public test name randomness");
        format!("integration-test-{}", hex::encode(nonce))
    }
    fn public_key(name: &str) -> String {
        let seed = Zeroizing::new(credentials::read(name).expect("read test credential"));
        hex::encode(SigningKey::from_bytes(&seed).verifying_key().to_bytes())
    }

    #[test]
    #[ignore = "explicit Windows Credential Manager integration gate; creates and removes one random test entry"]
    fn windows_credential_roundtrip_refuses_overwrite() {
        let name = test_name();
        let entry = CreatedEntry::new(&name);
        let generated = credentials::generate(&name).expect("create test credential");
        entry.armed.store(true, Ordering::SeqCst);
        assert_eq!(generated.as_object().expect("metadata").len(), 2);
        assert_eq!(generated["keyName"], name);
        assert_eq!(generated["publicKey"], public_key(&name));
        assert!(
            credentials::generate(&name).is_err(),
            "existing key was overwritten"
        );
        assert!(
            credentials::generate(&name.to_ascii_uppercase()).is_err(),
            "case alias replaced an existing key"
        );
        assert_eq!(generated["publicKey"], public_key(&name));
        entry.delete();
        assert!(
            credentials::read(&name).is_err(),
            "deleted entry remained readable"
        );
    }

    #[test]
    #[ignore = "explicit Windows Credential Manager integration gate; tests concurrent creation and removes its random entry"]
    fn windows_concurrent_generation_keeps_one_key() {
        let name = test_name();
        let entry = CreatedEntry::new(&name);
        let barrier = Arc::new(Barrier::new(2));
        let handles: Vec<_> = (0..2)
            .map(|_| {
                let name = name.clone();
                let barrier = barrier.clone();
                let armed = entry.armed.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    let result = credentials::generate(&name);
                    if result.is_ok() {
                        armed.store(true, Ordering::SeqCst);
                    }
                    result
                })
            })
            .collect();
        // Join every writer before any assertion can unwind and run entry cleanup.
        let joined: Vec<_> = handles.into_iter().map(|h| h.join()).collect();
        assert!(joined.iter().all(Result::is_ok), "key creator panicked");
        let results: Vec<_> = joined
            .into_iter()
            .map(|r| match r {
                Ok(result) => result,
                Err(_) => panic!("key creator panicked"),
            })
            .collect();
        assert_eq!(
            results.iter().filter(|r| r.is_ok()).count(),
            1,
            "concurrent callers replaced a key"
        );
        let generated = results
            .into_iter()
            .find_map(Result::ok)
            .expect("one created entry");
        assert_eq!(generated["publicKey"], public_key(&name));
        entry.delete();
    }
}
