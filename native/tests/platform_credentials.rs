use yoke::platform_credentials::{read, write_new};

#[test]
fn rejects_foreign_ambiguous_and_traversing_targets_before_backend_access() {
    for target in [
        "",
        "codex-agent-mail/devtesting@totalcreations.de",
        "Yoke-code-intelligence/federation/a",
        "yoke-code-intelligence/federation/",
        "yoke-code-intelligence/federation/../a",
        "yoke-code-intelligence/federation/a/b",
        "yoke-code-intelligence/federation/.a",
        "yoke-code-intelligence/federation/ä",
        "yoke-code-intelligence/federation/a\0b",
        "yoke-code-intelligence/service/password/a",
        "yoke-code-intelligence/service/tls/a/",
        "yoke-code-intelligence/service/token/-a",
        "yoke-code-intelligence/service/token/a\\b",
    ] {
        let error = write_new(target, b"synthetic-test-secret")
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("Invalid Yoke credential target"),
            "Unexpected rejection category"
        );
        assert!(!error.contains("synthetic-test-secret"));
        assert!(
            read(target)
                .err()
                .unwrap()
                .to_string()
                .contains("Invalid Yoke credential target")
        );
    }
    let too_long = format!("yoke-code-intelligence/federation/{}", "a".repeat(81));
    assert!(
        read(&too_long)
            .err()
            .unwrap()
            .to_string()
            .contains("Invalid Yoke credential target")
    );
}

#[test]
fn rejects_empty_and_oversized_binary_secrets_before_backend_access() {
    for bytes in [Vec::new(), vec![0xa5; 2561]] {
        let error = write_new("yoke-code-intelligence/service/token/test", &bytes)
            .unwrap_err()
            .to_string();
        assert!(error.contains("Yoke credential secret must contain 1-2560 bytes"));
    }
}

#[cfg(windows)]
#[test]
fn valid_native_targets_do_not_use_mock_or_windows_fallback() {
    for prefix in ["federation", "service/tls", "service/token"] {
        let target = format!("yoke-code-intelligence/{prefix}/A.z_9-{}\n", "x");
        // The newline is rejected; valid spelling reaches explicit platform absence.
        assert!(
            read(&target)
                .err()
                .unwrap()
                .to_string()
                .contains("Invalid Yoke credential target")
        );
        let valid = format!("yoke-code-intelligence/{prefix}/A.z_9-x");
        assert!(
            write_new(&valid, &[0, 255, 128])
                .unwrap_err()
                .to_string()
                .contains("unavailable on this platform")
        );
        assert!(
            read(&valid)
                .err()
                .unwrap()
                .to_string()
                .contains("unavailable on this platform")
        );
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
#[ignore = "Writes only a new synthetic OS credential; requires explicit YOKE_NATIVE_CREDENTIAL_TEST=1 and an existing unlocked native store"]
fn actual_native_store_binary_roundtrip_refuses_overwrite() {
    assert!(
        std::env::var("YOKE_NATIVE_CREDENTIAL_TEST").as_deref() == Ok("1"),
        "Explicit native credential test gate is required"
    );
    let mut nonce = [0u8; 16];
    getrandom::fill(&mut nonce).unwrap();
    let target = format!(
        "yoke-code-intelligence/service/token/native-gate-{}",
        hex::encode(nonce)
    );
    let bytes = zeroize::Zeroizing::new((0..2560).map(|i| (i % 256) as u8).collect::<Vec<_>>());
    assert!(
        write_new(&target, &bytes).is_ok(),
        "Existing native credential store is required; no fallback is permitted"
    );
    struct Cleanup(String);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            use keyring::credential::CredentialApi;
            #[cfg(target_os = "macos")]
            let entry = keyring::macos::MacCredential::new_with_target(
                Some("User"),
                "yoke-code-intelligence",
                &self.0,
            );
            #[cfg(target_os = "linux")]
            let entry = keyring::secret_service::SsCredential::new_with_target(
                None,
                "yoke-code-intelligence",
                &self.0,
            );
            if let Ok(entry) = entry {
                let _ = entry.delete_credential();
            }
        }
    }
    let _cleanup = Cleanup(target.clone());
    assert!(
        read(&target).is_ok_and(|actual| *actual == *bytes),
        "Binary native roundtrip failed"
    );
    assert!(
        write_new(&target, b"different-synthetic-gate-value").is_err(),
        "Existing native entries must not be overwritten"
    );
    assert!(
        read(&target).is_ok_and(|actual| *actual == *bytes),
        "Rejected overwrite changed original credential"
    );
}
