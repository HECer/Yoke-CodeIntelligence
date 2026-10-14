use yoke::host_credentials;
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
    }
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
