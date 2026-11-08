//! Versioned Windows OS credential chunks; manifest publication is the final step.
use crate::{capture::Deadline, model::hash, model_bedrock::AwsSession};
use anyhow::{Context, Result, ensure};
use serde::{
    Deserialize,
    de::{Deserializer, Visitor},
};
use serde_json::{Value, json};
use std::{
    fmt,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use zeroize::Zeroizing;
const CAPACITY: usize = 65_536;
const PAYLOAD_CAP: usize = CAPACITY + 23;
const CHUNK_DATA: usize = 2375;
const MANIFEST_LEN: usize = 67;
trait Storage {
    fn exists(&self, target: &str, deadline: &Deadline) -> Result<bool>;
    fn read(&self, target: &str, deadline: &Deadline) -> Result<Zeroizing<Vec<u8>>>;
    fn write_new(&self, target: &str, bytes: &[u8], deadline: &Deadline) -> Result<()>;
}
struct Native;
impl Storage for Native {
    fn exists(&self, target: &str, deadline: &Deadline) -> Result<bool> {
        deadline.check()?;
        #[cfg(windows)]
        {
            let found = super::windows::exists(target)?;
            deadline.check()?;
            Ok(found)
        }
        #[cfg(not(windows))]
        {
            let _ = target;
            anyhow::bail!("AWS session storage is currently Windows-only")
        }
    }
    fn read(&self, target: &str, deadline: &Deadline) -> Result<Zeroizing<Vec<u8>>> {
        deadline.check()?;
        #[cfg(windows)]
        {
            let bytes = super::windows::read(target)?;
            deadline.check()?;
            Ok(bytes)
        }
        #[cfg(not(windows))]
        {
            let _ = target;
            anyhow::bail!("AWS session storage is currently Windows-only")
        }
    }
    fn write_new(&self, target: &str, bytes: &[u8], deadline: &Deadline) -> Result<()> {
        deadline.check()?;
        #[cfg(windows)]
        {
            super::windows::write_new_bounded(target, bytes, deadline)?;
            deadline.check()?;
            Ok(())
        }
        #[cfg(not(windows))]
        {
            let _ = (target, bytes);
            anyhow::bail!("AWS session storage is currently Windows-only")
        }
    }
}
/// Create-only publication. Interruption may leave unpublished generation chunks
/// in the OS store; this is not a transaction spanning OS credential entries.
pub fn import_aws_session(name: &str, session: AwsSession, deadline: &Deadline) -> Result<Value> {
    import_with(&Native, name, session, deadline)
}
/// Private-pipe DTO; no generic JSON Value ever holds imported secret material.
pub fn import_aws_session_json(name: &str, input: &[u8], deadline: &Deadline) -> Result<Value> {
    super::target("aws-session", name)?;
    let session = decode_json(input, deadline)?;
    import_aws_session(name, session, deadline)
}
pub fn read_aws_session(name: &str, deadline: &Deadline) -> Result<AwsSession> {
    read_with(&Native, name, deadline)
}
fn expiration_ms(expiry: Option<SystemTime>) -> Result<u64> {
    match expiry {
        None => Ok(u64::MAX),
        Some(at) => {
            let ms = u64::try_from(
                at.duration_since(UNIX_EPOCH)
                    .context("Invalid AWS expiration")?
                    .as_millis(),
            )?;
            ensure!(ms != u64::MAX, "Invalid AWS expiration");
            Ok(ms)
        }
    }
}
fn expiry_from_ms(ms: u64) -> Result<Option<SystemTime>> {
    if ms == u64::MAX {
        Ok(None)
    } else {
        Ok(Some(
            UNIX_EPOCH
                .checked_add(Duration::from_millis(ms))
                .context("Invalid AWS expiration")?,
        ))
    }
}
fn encode(session: &AwsSession) -> Result<Zeroizing<Vec<u8>>> {
    session.validate(SystemTime::now())?;
    let (key, secret, token, expiry) = session.material();
    let token = token.unwrap_or("");
    let mut bytes = Zeroizing::new(Vec::with_capacity(
        key.len() + secret.len() + token.len() + 23,
    ));
    bytes.extend_from_slice(b"YAWS-P1");
    bytes.extend_from_slice(&u16::try_from(key.len())?.to_be_bytes());
    bytes.extend_from_slice(&u16::try_from(secret.len())?.to_be_bytes());
    bytes.extend_from_slice(&u32::try_from(token.len())?.to_be_bytes());
    bytes.extend_from_slice(&expiration_ms(expiry)?.to_be_bytes());
    bytes.extend_from_slice(key.as_bytes());
    bytes.extend_from_slice(secret.as_bytes());
    bytes.extend_from_slice(token.as_bytes());
    ensure!(
        bytes.len() <= PAYLOAD_CAP,
        "AWS session exceeds local payload admission"
    );
    Ok(bytes)
}
fn decode(bytes: &[u8]) -> Result<AwsSession> {
    ensure!(
        bytes.len() >= 23 && bytes.len() <= PAYLOAD_CAP && &bytes[..7] == b"YAWS-P1",
        "Invalid AWS session payload"
    );
    let key = u16::from_be_bytes(bytes[7..9].try_into()?) as usize;
    let secret = u16::from_be_bytes(bytes[9..11].try_into()?) as usize;
    let token = u32::from_be_bytes(bytes[11..15].try_into()?) as usize;
    let expiry = u64::from_be_bytes(bytes[15..23].try_into()?);
    ensure!(
        key <= 128
            && secret <= 256
            && token <= CAPACITY
            && key + secret + token <= CAPACITY
            && 23 + key + secret + token == bytes.len(),
        "Invalid AWS session field lengths"
    );
    let string = |start, end| -> Result<Zeroizing<String>> {
        Ok(Zeroizing::new(
            std::str::from_utf8(&bytes[start..end])
                .context("Invalid AWS session text")?
                .to_owned(),
        ))
    };
    AwsSession::new(
        string(23, 23 + key)?,
        string(23 + key, 23 + key + secret)?,
        if token == 0 {
            None
        } else {
            Some(string(23 + key + secret, bytes.len())?)
        },
        expiry_from_ms(expiry)?,
    )
}
struct Manifest {
    generation: [u8; 16],
    count: usize,
    length: usize,
    digest: [u8; 32],
    expiry: u64,
}
fn manifest(bytes: &[u8]) -> Result<Manifest> {
    ensure!(
        bytes.len() == MANIFEST_LEN && &bytes[..5] == b"YAWS1",
        "Invalid AWS session manifest"
    );
    let result = Manifest {
        generation: bytes[5..21].try_into()?,
        count: u16::from_be_bytes(bytes[21..23].try_into()?) as usize,
        length: u32::from_be_bytes(bytes[23..27].try_into()?) as usize,
        digest: bytes[27..59].try_into()?,
        expiry: u64::from_be_bytes(bytes[59..67].try_into()?),
    };
    ensure!(
        result.length >= 23
            && result.length <= PAYLOAD_CAP
            && result.count == result.length.div_ceil(CHUNK_DATA)
            && result.count <= 28,
        "Invalid AWS session chunk admission"
    );
    ensure!(
        expiry_from_ms(result.expiry)?.is_none_or(|at| at > SystemTime::now()),
        "Host AWS session expired"
    );
    Ok(result)
}
fn chunk_target(base: &str, generation: &[u8; 16], index: usize) -> String {
    format!("{base}/chunk/{}/{index:02}", hex::encode(generation))
}
fn import_with(
    storage: &dyn Storage,
    name: &str,
    session: AwsSession,
    deadline: &Deadline,
) -> Result<Value> {
    deadline.check()?;
    let target = super::target("aws-session", name)?;
    let payload = encode(&session)?;
    ensure!(
        !storage.exists(&target, deadline)?,
        "Host AWS session already exists; refusing overwrite"
    );
    let mut generation = [0u8; 16];
    getrandom::fill(&mut generation)
        .map_err(|_| anyhow::anyhow!("AWS generation randomness unavailable"))?;
    let count = payload.len().div_ceil(CHUNK_DATA);
    for (index, piece) in payload.chunks(CHUNK_DATA).enumerate() {
        deadline.check()?;
        let mut chunk = Zeroizing::new(Vec::with_capacity(25 + piece.len()));
        chunk.extend_from_slice(b"YAWC1");
        chunk.extend_from_slice(&generation);
        chunk.extend_from_slice(&(index as u16).to_be_bytes());
        chunk.extend_from_slice(&(count as u16).to_be_bytes());
        chunk.extend_from_slice(piece);
        storage
            .write_new(&chunk_target(&target, &generation, index), &chunk, deadline)
            .map_err(|_| {
                anyhow::anyhow!(
                    "AWS session chunk publication interrupted; unpublished OS chunks may remain"
                )
            })?;
    }
    deadline.check()?;
    session.validate(SystemTime::now())?;
    let mut record = Vec::with_capacity(MANIFEST_LEN);
    record.extend_from_slice(b"YAWS1");
    record.extend_from_slice(&generation);
    record.extend_from_slice(&(count as u16).to_be_bytes());
    record.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    record.extend_from_slice(&hex::decode(hash(&payload))?);
    record.extend_from_slice(&expiration_ms(session.material().3)?.to_be_bytes());
    storage.write_new(&target, &record, deadline).map_err(|_| {
        anyhow::anyhow!("AWS session manifest publication failed; unpublished OS chunks remain")
    })?;
    Ok(
        json!({"credentialName":name,"credentialType":"aws_session_v1","privateMaterial":"os_credential_store_only"}),
    )
}
fn read_with(storage: &dyn Storage, name: &str, deadline: &Deadline) -> Result<AwsSession> {
    deadline.check()?;
    let target = super::target("aws-session", name)?;
    let bytes = storage.read(&target, deadline)?;
    let record = manifest(&bytes)?;
    let mut payload = Zeroizing::new(Vec::with_capacity(record.length));
    for index in 0..record.count {
        deadline.check()?;
        let bytes = storage.read(&chunk_target(&target, &record.generation, index), deadline)?;
        let expected = (record.length - index * CHUNK_DATA).min(CHUNK_DATA);
        ensure!(
            bytes.len() == 25 + expected
                && &bytes[..5] == b"YAWC1"
                && bytes[5..21] == record.generation
                && u16::from_be_bytes(bytes[21..23].try_into()?) as usize == index
                && u16::from_be_bytes(bytes[23..25].try_into()?) as usize == record.count,
            "AWS session chunk missing, mixed, or invalid"
        );
        payload.extend_from_slice(&bytes[25..]);
    }
    deadline.check()?;
    ensure!(
        payload.len() == record.length && hash(&payload) == hex::encode(record.digest),
        "AWS session payload integrity mismatch"
    );
    let session = decode(&payload)?;
    ensure!(
        expiration_ms(session.material().3)? == record.expiry,
        "AWS session expiry binding mismatch"
    );
    // Recheck the current publication after assembling immutable chunks. Revoked
    // or replaced manifests cannot authorize a stale session reconstructed here.
    let current = storage.read(&target, deadline)?;
    ensure!(
        *current == *bytes,
        "AWS session manifest changed or was revoked"
    );
    deadline.check()?;
    session.validate(SystemTime::now())?;
    Ok(session)
}
struct Secret(Zeroizing<String>);
impl<'de> Deserialize<'de> for Secret {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        struct SecretVisitor;
        impl<'de> Visitor<'de> for SecretVisitor {
            type Value = Secret;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("private credential string")
            }
            fn visit_str<E: serde::de::Error>(
                self,
                value: &str,
            ) -> std::result::Result<Self::Value, E> {
                Ok(Secret(Zeroizing::new(value.to_owned())))
            }
            fn visit_string<E: serde::de::Error>(
                self,
                value: String,
            ) -> std::result::Result<Self::Value, E> {
                Ok(Secret(Zeroizing::new(value)))
            }
        }
        deserializer.deserialize_string(SecretVisitor)
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Imported {
    access_key_id: Secret,
    secret_access_key: Secret,
    session_token: Option<Secret>,
    expires_at_unix_ms: Option<u64>,
}
fn decode_json(input: &[u8], deadline: &Deadline) -> Result<AwsSession> {
    deadline.check()?;
    ensure!(
        !input.is_empty() && input.len() <= 70_000,
        "Private AWS JSON exceeds local input admission"
    );
    let imported: Imported = serde_json::from_slice(input)
        .map_err(|_| anyhow::anyhow!("Invalid private AWS session JSON"))?;
    deadline.check()?;
    AwsSession::new(
        imported.access_key_id.0,
        imported.secret_access_key.0,
        imported.session_token.map(|s| s.0),
        match imported.expires_at_unix_ms {
            None => None,
            Some(ms) => Some(
                UNIX_EPOCH
                    .checked_add(Duration::from_millis(ms))
                    .context("Invalid AWS expiration")?,
            ),
        },
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        cell::{Cell, RefCell},
        collections::BTreeMap,
        time::{Duration, SystemTime},
    };
    #[derive(Default)]
    struct Memory {
        rows: RefCell<BTreeMap<String, Zeroizing<Vec<u8>>>>,
        writes: Cell<usize>,
        fail_at: Cell<Option<usize>>,
        revoke_during_chunk_read: Cell<bool>,
    }
    impl Storage for Memory {
        fn exists(&self, target: &str, deadline: &Deadline) -> Result<bool> {
            deadline.check()?;
            Ok(self.rows.borrow().contains_key(target))
        }
        fn read(&self, target: &str, deadline: &Deadline) -> Result<Zeroizing<Vec<u8>>> {
            deadline.check()?;
            let bytes = self
                .rows
                .borrow()
                .get(target)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("missing owned fixture"))?;
            if self.revoke_during_chunk_read.get()
                && let Some((base, _)) = target.split_once("/chunk/")
            {
                self.rows.borrow_mut().remove(base);
            }
            Ok(bytes)
        }
        fn write_new(&self, target: &str, bytes: &[u8], deadline: &Deadline) -> Result<()> {
            deadline.check()?;
            let n = self.writes.get() + 1;
            self.writes.set(n);
            anyhow::ensure!(self.fail_at.get() != Some(n), "owned write interruption");
            let mut rows = self.rows.borrow_mut();
            anyhow::ensure!(!rows.contains_key(target), "create only");
            anyhow::ensure!(bytes.len() <= 2400, "chunk bound");
            rows.insert(target.into(), Zeroizing::new(bytes.to_vec()));
            Ok(())
        }
    }
    fn fixture() -> AwsSession {
        AwsSession::new(
            Zeroizing::new("AKIDEXAMPLE".into()),
            Zeroizing::new("synthetic-secret-not-real".into()),
            Some(Zeroizing::new("x".repeat(4096))),
            Some(SystemTime::now() + Duration::from_secs(3600)),
        )
        .unwrap()
    }
    #[test]
    fn aws_chunks_roundtrip_larger_than_single_blob_and_are_create_only() {
        let storage = Memory::default();
        let deadline = Deadline::new(5000);
        let receipt = import_with(&storage, "owned-fixture", fixture(), &deadline)
            .expect("chunked OS session must publish");
        assert!(storage.rows.borrow().len() >= 3);
        assert!(!receipt.to_string().contains("synthetic-secret"));
        assert!(!receipt.to_string().contains("AKIDEXAMPLE"));
        let restored = read_with(&storage, "owned-fixture", &deadline).unwrap();
        let (key, secret, token, _) = restored.material();
        assert!(key == "AKIDEXAMPLE" && secret == "synthetic-secret-not-real");
        assert!(token.is_some_and(|token| token.len() == 4096 && token.bytes().all(|b| b == b'x')));
        let before = storage.rows.borrow().clone();
        assert!(import_with(&storage, "owned-fixture", fixture(), &deadline).is_err());
        assert!(*storage.rows.borrow() == before);
    }
    #[test]
    fn interrupted_publication_and_changed_chunks_never_yield_session() {
        let storage = Memory::default();
        storage.fail_at.set(Some(2));
        let deadline = Deadline::new(5000);
        assert!(import_with(&storage, "owned-fixture", fixture(), &deadline).is_err());
        assert_eq!(storage.rows.borrow().len(), 1);
        assert!(read_with(&storage, "owned-fixture", &deadline).is_err());
        storage.fail_at.set(None);
        import_with(&storage, "owned-fixture", fixture(), &deadline).unwrap();
        let chunk = storage
            .rows
            .borrow()
            .keys()
            .find(|key| key.contains("/chunk/") && !key.ends_with("orphan"))
            .unwrap()
            .clone();
        storage.rows.borrow_mut().get_mut(&chunk).unwrap()[0] ^= 1;
        // There may be an orphan from the interrupted generation. Corrupt every
        // chunk to ensure the current manifest cannot select an intact generation.
        for (key, bytes) in storage.rows.borrow_mut().iter_mut() {
            if key.contains("/chunk/") {
                bytes[0] = 0;
            }
        }
        assert!(read_with(&storage, "owned-fixture", &deadline).is_err());
    }
    #[test]
    fn aws_private_json_parser_accepts_exact_dto_and_refuses_ambiguity() {
        let deadline = Deadline::new(5000);
        assert!(
            decode_json(
                br#"{"accessKeyId":"AKIDEXAMPLE","secretAccessKey":"synthetic-secret"}"#,
                &deadline
            )
            .is_ok()
        );
        for input in [br#"{"accessKeyId":"AKIDEXAMPLE","accessKeyId":"SECOND","secretAccessKey":"fixture"}"#.as_slice(),br#"{"accessKeyId":"AKIDEXAMPLE","secretAccessKey":"fixture","endpoint":"https://untrusted.example"}"#.as_slice(),br#"{"accessKeyId":"AKIDEXAMPLE","secretAccessKey":"fixture","sessionToken":"temporary"}"#.as_slice()] {assert!(decode_json(input,&deadline).is_err());}
    }
    #[test]
    fn aws_manifest_rejects_missing_mixed_corrupt_and_expired_generation() {
        let deadline = Deadline::new(5000);
        for mutation in 0..7 {
            let storage = Memory::default();
            import_with(&storage, "owned-fixture", fixture(), &deadline).unwrap();
            let base = super::super::target("aws-session", "owned-fixture").unwrap();
            let record = storage.rows.borrow().get(&base).unwrap().clone();
            let selected = manifest(&record).unwrap();
            let chunk = chunk_target(&base, &selected.generation, 0);
            match mutation {
                0 => {
                    storage.rows.borrow_mut().remove(&chunk);
                }
                1 => {
                    storage.rows.borrow_mut().get_mut(&chunk).unwrap()[5] ^= 1;
                }
                2 => {
                    let mut rows = storage.rows.borrow_mut();
                    let bytes = rows.get_mut(&chunk).unwrap();
                    let last = bytes.len() - 1;
                    bytes[last] ^= 1;
                }
                3 => {
                    storage.rows.borrow_mut().get_mut(&base).unwrap()[21..23]
                        .copy_from_slice(&u16::MAX.to_be_bytes());
                }
                4 => {
                    storage.rows.borrow_mut().get_mut(&base).unwrap()[59..67]
                        .copy_from_slice(&0u64.to_be_bytes());
                }
                5 => {
                    storage.rows.borrow_mut().get_mut(&base).unwrap()[0] = 0;
                }
                _ => {
                    storage.revoke_during_chunk_read.set(true);
                }
            }
            assert!(
                read_with(&storage, "owned-fixture", &deadline).is_err(),
                "mutation {mutation}"
            );
        }
    }
    #[test]
    fn aws_manifest_failure_leaves_only_unpublished_chunks_and_expired_deadline_writes_nothing() {
        let storage = Memory::default();
        let deadline = Deadline::new(5000);
        storage.fail_at.set(Some(3));
        let error = import_with(&storage, "owned-fixture", fixture(), &deadline).unwrap_err();
        assert!(error.to_string().contains("unpublished OS chunks"));
        assert_eq!(storage.rows.borrow().len(), 2);
        assert!(read_with(&storage, "owned-fixture", &deadline).is_err());
        let empty = Memory::default();
        assert!(import_with(&empty, "owned-fixture", fixture(), &Deadline::new(0)).is_err());
        assert_eq!(empty.writes.get(), 0);
    }
}
