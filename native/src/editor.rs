use crate::{
    Engine,
    capture::Deadline,
    model::{Snapshot, SourceFile, hash, identity, valid_id},
    paths::{normalized, obvious_secret, scoped},
    store::Store,
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Buffer {
    pub path: String,
    pub version: u32,
    pub hash: String,
    pub size: usize,
    pub base_hash: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: String,
    pub base_snapshot_id: String,
    pub revision: u64,
    pub buffers: BTreeMap<String, Buffer>,
    pub last_snapshot_id: Option<String>,
}
fn random_id() -> Result<String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|_| anyhow::anyhow!("System randomness unavailable"))?;
    Ok(hex::encode(bytes))
}
pub fn session(store: &Store, id: &str) -> Result<Session> {
    ensure!(valid_id(id), "Invalid editor session");
    let value: Session = store.get_record("editor-session", id)?;
    ensure!(value.id == id, "Editor session identity mismatch");
    Ok(value)
}
fn save(store: &Store, before: &Session, after: &Session) -> Result<()> {
    ensure!(
        store.replace_record("editor-session", &before.id, before, after)?,
        "Editor changed concurrently; retry with its latest version"
    );
    Ok(())
}
pub fn open(engine: &mut Engine, deadline: &Deadline) -> Result<Session> {
    engine.store.set_deadline(deadline)?;
    let count: i64 = engine.store.db.query_row(
        "SELECT count(*) FROM records WHERE kind='editor-session'",
        [],
        |row| row.get(0),
    )?;
    ensure!(
        count < 64,
        "Too many editor sessions; close unused sessions"
    );
    let (base, _, _, _) = engine.index(deadline)?;
    let value = Session {
        id: random_id()?,
        base_snapshot_id: base.id,
        revision: 0,
        buffers: BTreeMap::new(),
        last_snapshot_id: None,
    };
    engine.store.record("editor-session", &value.id, &value)?;
    Ok(value)
}
fn allowed_new_path(
    engine: &Engine,
    base: &Snapshot,
    path: &str,
    deadline: &Deadline,
    scope: &crate::capture::CapturedSourceScope<'_>,
) -> Result<()> {
    scope.check_host_write_scope(path)?;
    let absolute = scoped(&engine.root, path)?;
    ensure!(
        !base.omissions.iter().any(|omission| omission.path == path),
        "Omitted source cannot enter an editor overlay"
    );
    if base.files.iter().any(|file| file.path == path) {
        return Ok(());
    }
    ensure!(
        !absolute.exists(),
        "Uncaptured existing source cannot enter an editor overlay"
    );
    let git = engine.root.join(".git");
    if let Ok(meta) = std::fs::symlink_metadata(&git) {
        ensure!(
            !crate::paths::is_link(&meta) && meta.is_dir(),
            "New overlay files require captured Git ignore policy; external Git directories are unsupported"
        );
        let info = git.join("info");
        if let Ok(meta) = std::fs::symlink_metadata(&info) {
            ensure!(
                !crate::paths::is_link(&meta) && meta.is_dir(),
                "Git exclude policy link refused"
            );
            let policy = info.join("exclude");
            if let Ok(meta) = std::fs::symlink_metadata(&policy) {
                ensure!(
                    !crate::paths::is_link(&meta) && meta.is_file() && meta.len() <= 1_000_000,
                    "Unsupported Git exclude policy"
                );
            }
        }
    }
    // Use the walker's composed single-path matcher. This
    // includes ancestor ignore files and their precedence for nonexistent
    // editor paths, exactly as the saved-source walker does.
    deadline.check()?;
    let mut matchers = ignore::WalkBuilder::new(&engine.root)
        .hidden(false)
        .follow_links(false)
        .git_ignore(true)
        .git_global(false)
        .git_exclude(true)
        .require_git(false)
        .build_matchers();
    let (matched, error) = matchers[0].matched_with_errors(path, false);
    if let Some(error) = error {
        return Err(error.into());
    }
    ensure!(
        !matched.is_ignore(),
        "Ignored source cannot enter an editor overlay"
    );
    deadline.check()?;
    Ok(())
}
pub fn update(engine: &Engine, id: &str, input: &Value, deadline: &Deadline) -> Result<Value> {
    engine.store.set_deadline(deadline)?;
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct Input {
        path: String,
        version: u32,
        text: String,
        expected_version: Option<u32>,
    }
    let input: Input = serde_json::from_value(input.clone())?;
    let path = normalized(&input.path)?;
    ensure!(
        input.version <= i32::MAX as u32
            && input.text.len() <= 4_000_000
            && !input.text.contains('\0')
            && !obvious_secret(&input.text),
        "Invalid, oversized or secret editor buffer"
    );
    let before = session(&engine.store, id)?;
    let base = engine.store.snapshot(&before.base_snapshot_id)?;
    let scope = crate::capture::source_scope_policy(&engine.store, &base)?;
    allowed_new_path(engine, &base, &path, deadline, &scope)?;
    let digest = hash(input.text.as_bytes());
    let prior = before.buffers.get(&path);
    ensure!(
        input
            .expected_version
            .is_none_or(|version| prior.is_some_and(|buffer| buffer.version == version)),
        "Editor expected version is stale"
    );
    if prior.is_some_and(|buffer| buffer.version == input.version && buffer.hash == digest) {
        return Ok(json!({"sessionId":id,"revision":before.revision,"idempotent":true}));
    }
    ensure!(
        prior.is_none_or(|buffer| input.version > buffer.version),
        "Editor buffer version is not newer"
    );
    ensure!(
        prior.is_some() || before.buffers.len() < 1000,
        "Too many editor buffers"
    );
    let mut after = before.clone();
    let base_hash = prior
        .map(|buffer| buffer.base_hash.clone())
        .unwrap_or_else(|| {
            base.files
                .iter()
                .find(|file| file.path == path)
                .map(|file| file.hash.clone())
        });
    let digest = engine.store.blob(input.text.as_bytes())?;
    after.buffers.insert(
        path.clone(),
        Buffer {
            path,
            version: input.version,
            hash: digest.clone(),
            size: input.text.len(),
            base_hash,
        },
    );
    let mut bytes = 0usize;
    for buffer in after.buffers.values() {
        deadline.check()?;
        bytes = bytes
            .checked_add(buffer.size)
            .ok_or_else(|| anyhow::anyhow!("Editor byte capacity overflow"))?;
    }
    ensure!(
        bytes <= 128_000_000,
        "Editor session byte capacity exceeded"
    );
    after.revision = after
        .revision
        .checked_add(1)
        .ok_or_else(|| anyhow::anyhow!("Editor revision overflow"))?;
    deadline.check()?;
    save(&engine.store, &before, &after)?;
    Ok(
        json!({"sessionId":id,"revision":after.revision,"version":input.version,"contentHash":digest}),
    )
}
pub fn close_file(
    store: &Store,
    id: &str,
    path: &str,
    expected: Option<u32>,
    deadline: &Deadline,
) -> Result<()> {
    store.set_deadline(deadline)?;
    let path = normalized(path)?;
    let before = session(store, id)?;
    ensure!(
        expected.is_none_or(|version| before
            .buffers
            .get(&path)
            .is_some_and(|buffer| buffer.version == version)),
        "Cannot close a newer editor buffer"
    );
    let mut after = before.clone();
    after.buffers.remove(&path);
    after.revision = after
        .revision
        .checked_add(1)
        .ok_or_else(|| anyhow::anyhow!("Editor revision overflow"))?;
    save(store, &before, &after)
}
pub fn close(store: &Store, id: &str, deadline: &Deadline) -> Result<()> {
    store.set_deadline(deadline)?;
    let before = session(store, id)?;
    ensure!(
        store.delete_record("editor-session", id, &before)?,
        "Editor changed concurrently"
    );
    Ok(())
}
pub fn capture(engine: &mut Engine, id: &str, deadline: &Deadline) -> Result<Snapshot> {
    capture_at(engine, id, engine.capture_epoch, deadline)
}
pub fn capture_at(
    engine: &mut Engine,
    id: &str,
    epoch: crate::analysis::AnalyzerEpoch,
    deadline: &Deadline,
) -> Result<Snapshot> {
    engine.store.set_deadline(deadline)?;
    let before = session(&engine.store, id)?;
    let (disk, _, _, _) = engine.index(deadline)?;
    let mut files = disk.files.clone();
    let mut after = before.clone();
    let scope = crate::capture::source_scope_policy(&engine.store, &disk)?;
    for buffer in before.buffers.values() {
        deadline.check()?;
        allowed_new_path(engine, &disk, &buffer.path, deadline, &scope)?;
        let saved = disk.files.iter().find(|file| file.path == buffer.path);
        let saved_hash = saved.map(|file| file.hash.clone());
        ensure!(
            saved_hash == buffer.base_hash || saved_hash.as_ref() == Some(&buffer.hash),
            "Disk changed independently of editor buffer: {}",
            buffer.path
        );
        let bytes = engine.store.read_blob(&buffer.hash)?;
        ensure!(
            bytes.len() == buffer.size
                && bytes.len() <= 4_000_000
                && !bytes.contains(&0)
                && !obvious_secret(std::str::from_utf8(&bytes)?),
            "Stored editor buffer violates source policy"
        );
        if saved_hash.as_ref() == Some(&buffer.hash) {
            after.buffers.get_mut(&buffer.path).unwrap().base_hash = Some(buffer.hash.clone());
        }
        let file = SourceFile {
            path: buffer.path.clone(),
            hash: buffer.hash.clone(),
            size: bytes.len(),
            language: crate::capture::language_for_snapshot(&engine.store, &disk, &buffer.path)?,
            mode: saved.map_or(0o644, |file| file.mode),
        };
        files.retain(|file| file.path != buffer.path);
        files.push(file);
    }
    drop(scope);
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let size = files
        .iter()
        .try_fold(0usize, |total, file| total.checked_add(file.size))
        .ok_or_else(|| anyhow::anyhow!("Overlay byte capacity overflow"))?;
    ensure!(
        files.len() <= 100000 && size <= 512_000_000,
        "Overlay snapshot capacity exceeded"
    );
    let versions: BTreeMap<_, _> = before
        .buffers
        .iter()
        .map(|(path, buffer)| (path, buffer.version))
        .collect();
    let snapshot = Snapshot {
        id: identity(&(
            epoch.producer(),
            &disk.id,
            id,
            before.revision,
            &versions,
            &files,
        )),
        workspace_id: disk.workspace_id.clone(),
        kind: "overlay".into(),
        files,
        omissions: disk.omissions.clone(),
        capture_consistency: "overlay-from-stable-disk-and-immutable-buffers".into(),
    };
    let (snapshot, _, _, _) = engine.analyze_snapshot_at(epoch, snapshot, deadline)?;
    after.base_snapshot_id = disk.id;
    after.last_snapshot_id = Some(snapshot.id.clone());
    engine.store.record("overlay-snapshot",&snapshot.id,&json!({"snapshotId":snapshot.id,"sessionId":id,"revision":before.revision,"versions":versions,"baseSnapshotId":after.base_snapshot_id}))?;
    deadline.check()?;
    save(&engine.store, &before, &after)?;
    Ok(snapshot)
}
