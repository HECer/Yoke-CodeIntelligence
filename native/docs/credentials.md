# Native host credentials

The host CLI and library may create federation signing keys, service tokens and TLS private identities in the current user's native credential store. Repository content and MCP tools cannot select credentials or call these host operations. Windows keeps its existing native Credential Manager implementation and target names.

The portable facade exposes `write_new(target: &str, bytes: &[u8]) -> Result<()>` and `read(target: &str) -> Result<Zeroizing<Vec<u8>>>`. It accepts only these exact namespaces:

- `yoke-code-intelligence/federation/{name}`
- `yoke-code-intelligence/service/tls/{name}`
- `yoke-code-intelligence/service/token/{name}`

Names contain 1–80 ASCII letters, digits, underscores, hyphens or periods and begin with a letter or digit. Secrets contain 1–2560 binary bytes. Invalid names and capacities fail before native store access. No password decoding, secret JSON, secret logs, default mock store, plaintext file fallback or analyzed-source credential configuration is used. Returned bytes are zeroized when their wrapper is dropped; callers must avoid formatting or copying them into logs.

macOS uses the native user Keychain with an explicitly constructed keyring `MacCredential`. Creation uses the native add operation, which refuses an existing entry atomically. A process mutex guards temporary suppression of Keychain interaction and restores its prior state afterward. This avoids opening unlock/permission dialogs for these operations. A locked, unavailable or denied Keychain fails explicitly. Yoke does not create or unlock keychains. The interaction setting is process-wide during the guarded call; embedding hosts must account for other Keychain users in their process.

Linux uses keyring's native SecretService credential mapping and the SecretService 4 encrypted blocking backend with OS randomness. A single explicit Unix session-bus address, an already owned `org.freedesktop.secrets` bus name, an existing default collection and its unlocked state are required. Yoke refuses missing, locked and ambiguous items and creates only an item with replacement disabled. It does not invoke a daemon, create a collection, request an unlock or configure a headless session. Missing desktop services fail explicitly. The native broker remains trusted: the upstream backend uses a well-known service name and may reactivate a daemon or handle a broker-issued prompt if the service or policy changes after preflight. Preflight is not an atomic no-activation or no-prompt guarantee.

Creation and reads use a secret-free per-target cross-process lock. Its directory is `.yoke-credential-locks` under the home obtained from the OS account database, independent of `HOME`, XDG variables or source-selected paths. Yoke verifies ownership, private permissions and every existing path component; Unix lock files use `openat`, `O_NOFOLLOW`, inode/device checks and a single hard link. Lock files remain empty and are never unlinked by normal operations, preserving exclusion across processes. Acquisition waits at most five seconds. Native-store calls retain the platform library's own timing and are not governed by the code-analysis deadline.

Cooperating Yoke processes refuse overwrite under that shared lock, including invalid or empty existing entries. macOS also has native create-only enforcement. SecretService's creation call disables replacement, but a separate writer outside Yoke's lock protocol can create an ambiguous duplicate; such entries are refused on reads. The OS account, native broker and same-user filesystem administration remain trusted boundaries.

The dependency features are target-specific: keyring 3.6.3 `apple-native` on macOS; `async-secret-service`, `async-io`, `crypto-rust` on Linux; SecretService 4.0.0 `rt-async-io-crypto-rust` and zbus 4 `async-io` for Linux; libc for Unix account/path checks. No synchronous libdbus backend is selected even though Cargo.lock includes its optional package resolution. The keyring global default builder cannot replace the explicitly selected native mapping with a mock.

Standard tests check namespace/capacity refusal and secret-free lock exclusion, timeout and tampering. Unix tests also reject symbolic links, hard links and public directories. Cross-compilation validates macOS Rust APIs; it does not prove macOS runtime access. Linux compilation and filesystem tests can run through the available Ubuntu WSL toolchain using a separate target directory. The ignored real-store gate writes a random new target with synthetic binary data, checks unchanged data after refused overwrite and removes only that test entry:

```sh
YOKE_NATIVE_CREDENTIAL_TEST=1 cargo test --locked -j2 --test platform_credentials actual_native_store_binary_roundtrip_refuses_overwrite -- --ignored
```

This gate requires an existing unlocked native store and performs no service provisioning. A missing store is a failure, not a mock-backed pass. Real-store tests and native model inference are separate capabilities.

Primary API references: [keyring 3.6.3 native features](https://docs.rs/keyring/3.6.3/keyring/), [macOS native Keychain mapping](https://docs.rs/crate/keyring/3.6.3/source/src/macos.rs), [Security framework create-only add](https://docs.rs/crate/security-framework/3.5.1/source/src/os/macos/passwords.rs), [SecretService 4 blocking API](https://docs.rs/crate/secret-service/4.0.0/source/src/blocking/mod.rs), and [SecretService OS-random encrypted sessions](https://docs.rs/crate/secret-service/4.0.0/source/src/session.rs).
