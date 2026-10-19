use yoke::host_credentials;
#[test]
fn postgres_credential_binding_and_private_bytes_are_checked_before_storage() {
    for binding in [
        "",
        "../other",
        "codex-agent-mail/example",
        "a",
        &"g".repeat(64),
    ] {
        let error =
            host_credentials::import_postgres_password(binding, b"private sentinel").unwrap_err();
        assert!(!error.to_string().contains("private sentinel"));
    }
    for bytes in [
        b"".as_slice(),
        b"private\nvalue",
        b"private\0value",
        &[255],
        &vec![b'x'; 2561],
    ] {
        let error = host_credentials::import_postgres_password(&"a".repeat(64), bytes).unwrap_err();
        assert!(!error.to_string().contains("private"));
    }
}
#[test]
fn host_credential_names_cannot_access_unrelated_accounts() {
    for name in [
        "",
        "../other",
        "codex-agent-mail/devtesting@totalcreations.de",
        "a:b",
        "a\\b",
        "a b",
        "a\0b",
    ] {
        assert!(host_credentials::generate_token(name).is_err());
        assert!(host_credentials::read_token(name).is_err());
        assert!(host_credentials::generate_tls(name, &["localhost".into()]).is_err());
        assert!(host_credentials::read_tls(name).is_err());
        assert!(host_credentials::read_github_token(name).is_err());
        assert!(host_credentials::import_github_token(name, b"fixture-token").is_err());
        assert!(host_credentials::read_model_token(name).is_err());
        assert!(host_credentials::import_model_token(name, b"fixture-token").is_err());
    }
}

#[test]
fn model_token_admission_rejects_header_injection_and_oversize_without_exposure() {
    for token in [
        b"".as_slice(),
        b"sensitive\r\nAuthorization: injected",
        b"sensitive value",
        b"sensitive\0value",
        &[255],
        &vec![b'x'; 2561],
    ] {
        let error = host_credentials::import_model_token("invalid-fixture", token).unwrap_err();
        assert!(!error.to_string().contains("sensitive"));
    }
}

#[test]
fn github_token_admission_rejects_invalid_bytes_without_exposing_them() {
    for token in [
        b"".as_slice(),
        b"sensitive\nvalue",
        b"sensitive value",
        b"sensitive\0value",
        &[255],
        &vec![b'x'; 2561],
    ] {
        let error = host_credentials::import_github_token("invalid-fixture", token).unwrap_err();
        assert!(!error.to_string().contains("sensitive"));
    }
}

#[cfg(windows)]
#[test]
#[ignore = "Actual Windows Credential Manager; creates and removes only one unique Yoke GitHub fixture entry"]
fn actual_github_credential_round_trip_is_create_only_and_keeps_secrets_out_of_metadata() {
    use zeroize::Zeroizing;
    let mut nonce = [0u8; 16];
    getrandom::fill(&mut nonce).unwrap();
    let name = format!("github-test-{}", hex::encode(nonce));
    let target = format!("yoke-code-intelligence/service/github/{name}");
    struct Created(String);
    impl Drop for Created {
        fn drop(&mut self) {
            #[link(name = "Advapi32")]
            unsafe extern "system" {
                fn CredDeleteW(target: *const u16, kind: u32, flags: u32) -> i32;
            }
            assert!(
                self.0
                    .starts_with("yoke-code-intelligence/service/github/github-test-")
            );
            let wide: Vec<_> = self.0.encode_utf16().chain(Some(0)).collect();
            unsafe {
                CredDeleteW(wide.as_ptr(), 1, 0);
            }
        }
    }
    let secret = Zeroizing::new(format!("fixture_{}", hex::encode(nonce)));
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    let mut child = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .args(["credentials", "github-import", &name])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    {
        let mut input = child.stdin.take().unwrap();
        input.write_all(secret.as_bytes()).unwrap();
        input.write_all(b"\r\n").unwrap();
    }
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "GitHub credential CLI import failed"
    );
    let _created = Created(target);
    assert!(!String::from_utf8_lossy(&output.stderr).contains(secret.as_str()));
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(!metadata.to_string().contains(secret.as_str()));
    assert!(!metadata.to_string().contains("tokenHash"));
    assert!(host_credentials::import_github_token(&name, b"different-fixture").is_err());
    let loaded = host_credentials::read_github_token(&name).unwrap();
    assert_eq!(loaded.as_str(), secret.as_str());
}
#[cfg(windows)]
#[test]
#[ignore = "Requires the actual Windows per-user Credential Manager; creates and removes only unique Yoke test entries"]
fn actual_tls_and_service_token_never_export_private_material_or_overwrite_entries() {
    use zeroize::Zeroize;
    struct Created(Vec<String>);
    impl Drop for Created {
        fn drop(&mut self) {
            #[link(name = "Advapi32")]
            unsafe extern "system" {
                fn CredDeleteW(target: *const u16, kind: u32, flags: u32) -> i32;
            }
            for target in &self.0 {
                assert!(target.starts_with("yoke-code-intelligence/service/"));
                let wide: Vec<_> = target.encode_utf16().chain(Some(0)).collect();
                unsafe {
                    CredDeleteW(wide.as_ptr(), 1, 0);
                }
            }
        }
    }
    let mut nonce = [0u8; 16];
    getrandom::fill(&mut nonce).unwrap();
    let name = format!("test-{}", hex::encode(nonce));
    nonce.zeroize();
    let mut created = Created(vec![]);
    let tls =
        host_credentials::generate_tls(&name, &["localhost".into(), "127.0.0.1".into()]).unwrap();
    created
        .0
        .push(format!("yoke-code-intelligence/service/tls/{name}"));
    assert!(
        tls["certificatePem"]
            .as_str()
            .unwrap()
            .contains("BEGIN CERTIFICATE")
    );
    assert!(!serde_json::to_string(&tls).unwrap().contains("PRIVATE KEY"));
    assert!(host_credentials::generate_tls(&name, &["localhost".into()]).is_err());
    let loaded = host_credentials::read_tls(&name).unwrap();
    assert!(loaded.certificate.len() > 100 && loaded.private_key.len() > 30);
    let token = host_credentials::generate_token(&name).unwrap();
    created
        .0
        .push(format!("yoke-code-intelligence/service/token/{name}"));
    assert!(token.get("token").is_none());
    assert!(token.get("secret").is_none());
    let first = host_credentials::read_token(&name).unwrap();
    assert!(first.len() == 32);
    assert!(host_credentials::generate_token(&name).is_err());
    let second = host_credentials::read_token(&name).unwrap();
    assert!(first.as_slice() == second.as_slice());
}
