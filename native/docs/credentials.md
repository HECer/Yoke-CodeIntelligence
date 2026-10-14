# Federation signing credentials

`credentials::generate(name: &str) -> anyhow::Result<serde_json::Value>` creates a
new Ed25519 signing key in the current Windows user's Credential Manager. Its
only returned fields are `keyName` and `publicKey`; the public key is 64 lowercase
hexadecimal characters. The private seed consists of 32 bytes from the OS-backed
`getrandom` provider and is never returned by generation.

`credentials::read(name: &str) -> anyhow::Result<[u8; 32]>` loads that seed into
memory for a host-authorized signing operation. The caller must immediately wrap
the result in `zeroize::Zeroizing`, use it only in memory, and never format or log
the array. Generation temporaries and Credential Manager read buffers are wiped
before they are released. These memory wipes do not promise control over OS swap,
crash dumps, or copies made by callers.

Names contain 1–80 ASCII letters, digits, `_`, `-`, or `.`, and start with a letter
or digit. The only credential targets accessible through this module are
`yoke-code-intelligence/federation/<name>`. Invalid names fail before any OS call.
The module does not enumerate credentials or access unrelated account entries.

Windows storage uses generic credentials with `CRED_PERSIST_LOCAL_MACHINE`: the
credential persists across logons on this computer for the current user and does
not request roaming persistence. Generation refuses an existing generic target,
even if its blob has an invalid format. Reads accept only an exact 32-byte seed.
A named Windows mutex serializes the check and write across helper processes and
logon sessions, including case aliases, with a five-second lock wait. Windows
does not provide a create-only `CredWriteW` operation; another program running as
the same user can still replace an entry without following this helper's lock.
OS failures return fixed context and numeric error codes, without credential
contents. Non-Windows builds explicitly report that the OS store is unavailable;
there is no file, SQLite, configuration, or environment-variable fallback.

This module is an explicit host facility. The host CLI may generate a credential
or read one for a separately authorized federation export. It must not be exposed
as an MCP operation or inferred from source files, editor buffers, imported
artifacts, or requests supplied by an untrusted source. Possessing a signing key
does not grant source access or execution authority.

## Validation

The normal `credentials` test target validates namespace boundaries without
accessing Credential Manager. Two ignored Windows integration tests exercise the
actual OS store: generation/public-key/read consistency, overwrite refusal,
case aliases, and concurrent creation. Each uses a random `integration-test-`
name, arms its cleanup guard only after a successful creation, and deletes
precisely that target. Normal cleanup checks deletion; unwind cleanup is best
effort. An externally terminated process can leave its random test credential.

Run the explicit Windows gate from the workspace root:

```powershell
cargo test -p yoke-code-intelligence -j 2 --test credentials -- --ignored
```

The tests do not print private seeds or credential blobs. They compare public
keys derived from the memory-only seed and verify that a second creator cannot
change the stored public key.
