//! Host-selected shared rate state. No source Store and no automatic recovery.
use crate::{capture::Deadline, source_acquisition as source};
use anyhow::{Result, ensure};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Component, Path},
    time::{SystemTime, UNIX_EPOCH},
};

const MAGIC: &[u8; 8] = b"YARATE1\0";
fn valid(ok: bool) -> Result<()> {
    ensure!(ok, "arxiv_rate_state_invalid");
    Ok(())
}
fn io<T>(result: std::io::Result<T>) -> Result<T> {
    result.map_err(|_| anyhow::anyhow!("arxiv_rate_state_unavailable"))
}
fn path_valid(path: &Path) -> Result<()> {
    source::input(
        path.is_absolute()
            && path
                .to_str()
                .is_some_and(|s| s.len() <= 4096 && !s.chars().any(char::is_control)),
    )?;
    for c in path.components() {
        match c {
            Component::ParentDir | Component::CurDir => source::input(false)?,
            Component::Normal(n) => {
                crate::paths::normalized_syntax(
                    n.to_str()
                        .ok_or_else(|| anyhow::anyhow!("arxiv_rate_path_invalid"))?,
                )
                .map_err(|_| anyhow::anyhow!("arxiv_rate_path_invalid"))?;
            }
            #[cfg(windows)]
            Component::Prefix(p) => source::input(matches!(
                p.kind(),
                std::path::Prefix::Disk(_) | std::path::Prefix::VerbatimDisk(_)
            ))?,
            _ => {}
        }
    }
    Ok(())
}
fn directories(path: &Path, d: &Deadline) -> Result<Vec<File>> {
    path_valid(path)?;
    let mut files = Vec::new();
    for ancestor in path.ancestors() {
        source::check(d)?;
        let m = io(fs::symlink_metadata(ancestor))?;
        valid(m.is_dir() && !crate::paths::is_link(&m))?;
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options
                .share_mode(3)
                .custom_flags(0x0200_0000 | 0x0020_0000);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NOFOLLOW | libc::O_DIRECTORY);
        }
        let file = io(options.open(ancestor))?;
        let opened = io(file.metadata())?;
        valid(opened.is_dir() && !crate::paths::is_link(&opened))?;
        files.push(file);
    }
    Ok(files)
}
fn single_link(file: &File) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        valid(io(file.metadata())?.nlink() == 1)?;
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        #[repr(C)]
        #[derive(Default)]
        struct Information {
            attributes: u32,
            created: [u32; 2],
            accessed: [u32; 2],
            written: [u32; 2],
            volume: u32,
            size_high: u32,
            size_low: u32,
            links: u32,
            index_high: u32,
            index_low: u32,
        }
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetFileInformationByHandle(
                handle: *mut std::ffi::c_void,
                info: *mut Information,
            ) -> i32;
        }
        let mut info = Information::default();
        // Live File handle and the documented C layout remain valid for the call.
        valid(
            unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } != 0
                && info.links == 1,
        )?;
    }
    #[cfg(not(any(windows, unix)))]
    {
        let _ = file;
        anyhow::bail!("arxiv_rate_platform_unavailable");
    }
    Ok(())
}
fn slot(sequence: u64, millis: u64) -> [u8; 64] {
    let mut value = [0u8; 64];
    value[..8].copy_from_slice(MAGIC);
    value[8..16].copy_from_slice(&sequence.to_le_bytes());
    value[16..24].copy_from_slice(&millis.to_le_bytes());
    value[24] = if sequence % 2 == 1 { 1 } else { 0 };
    let digest: [u8; 32] = Sha256::digest(&value[..32]).into();
    value[32..].copy_from_slice(&digest);
    value
}
fn state(bytes: &[u8]) -> Result<Option<(u64, u64)>> {
    valid(matches!(bytes.len(), 0 | 64 | 128))?;
    let mut rows = Vec::new();
    for (index, entry) in bytes.chunks_exact(64).enumerate() {
        if entry.iter().all(|b| *b == 0) {
            continue;
        }
        valid(&entry[..8] == MAGIC && entry[25..32].iter().all(|b| *b == 0))?;
        let digest: [u8; 32] = Sha256::digest(&entry[..32]).into();
        valid(entry[32..] == digest)?;
        let seq = u64::from_le_bytes(entry[8..16].try_into().unwrap());
        let time = u64::from_le_bytes(entry[16..24].try_into().unwrap());
        valid(
            seq > 0
                && time > 0
                && ((seq - 1) % 2) as usize == index
                && entry[24] == if seq % 2 == 1 { 1 } else { 0 },
        )?;
        rows.push((seq, time));
    }
    rows.sort_unstable();
    match rows.as_slice() {
        [] => Ok(None),
        [(1, time)] => Ok(Some((1, *time))),
        [(a, t1), (b, t2)] => {
            valid(a.checked_add(1) == Some(*b) && t2 >= t1)?;
            Ok(Some((*b, *t2)))
        }
        _ => {
            valid(false)?;
            unreachable!()
        }
    }
}
/// Non-cloneable admission, held exclusively through the single owned request.
pub(crate) struct Permit {
    _file: File,
    _directories: Vec<File>,
    pub(crate) sequence: u64,
    pub(crate) millis: u64,
}
impl Permit {
    /// A tiny mandatory journal cleanup, after the owned request has stopped.
    /// It never resets or extends the network request's original deadline.
    pub(crate) fn complete(mut self) -> Result<(u64, u64)> {
        let millis = u64::try_from(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|_| anyhow::anyhow!("arxiv_rate_clock_invalid"))?
                .as_millis(),
        )
        .map_err(|_| anyhow::anyhow!("arxiv_rate_clock_invalid"))?;
        ensure!(millis >= self.millis, "arxiv_rate_clock_regressed");
        let sequence = self
            .sequence
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("arxiv_rate_sequence_exhausted"))?;
        io(self._file.seek(SeekFrom::Start(((sequence - 1) % 2) * 64)))?;
        io(self._file.write_all(&slot(sequence, millis)))?;
        io(self._file.sync_all())?;
        Ok((sequence, millis))
    }
}
pub(crate) fn admit(path: &Path, allowed: bool, deadline: &Deadline) -> Result<Permit> {
    ensure!(allowed, "arxiv_rate_coordination_permission_required");
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| anyhow::anyhow!("arxiv_rate_clock_invalid"))?
        .as_millis();
    admit_at(
        path,
        u64::try_from(millis).map_err(|_| anyhow::anyhow!("arxiv_rate_clock_invalid"))?,
        deadline,
    )
}
fn admit_at(path: &Path, millis: u64, deadline: &Deadline) -> Result<Permit> {
    source::check(deadline)?;
    path_valid(path)?;
    source::input(path.file_name().and_then(|s| s.to_str()) == Some("arxiv-rate-v1.bin"))?;
    let dirs = directories(
        path.parent()
            .ok_or_else(|| anyhow::anyhow!("arxiv_rate_path_invalid"))?,
        deadline,
    )?;
    let mut options = OpenOptions::new();
    options.read(true).write(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(3).custom_flags(0x0020_0000);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW).mode(0o600);
    }
    let mut file = match options.create_new(true).open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            io(options.create_new(false).open(path))?
        }
        Err(_) => anyhow::bail!("arxiv_rate_state_unavailable"),
    };
    let meta = io(file.metadata())?;
    valid(meta.is_file() && !crate::paths::is_link(&meta) && meta.len() <= 128)?;
    single_link(&file)?;
    fs2::FileExt::try_lock_exclusive(&file)
        .map_err(|_| anyhow::anyhow!("arxiv_rate_coordination_busy"))?;
    source::check(deadline)?;
    let meta = io(file.metadata())?;
    valid(meta.len() <= 128)?;
    single_link(&file)?;
    let mut bytes = Vec::new();
    io((&mut file).take(129).read_to_end(&mut bytes))?;
    let prior = state(&bytes)?;
    let sequence = if let Some((sequence, last)) = prior {
        ensure!(sequence % 2 == 0, "arxiv_rate_previous_request_unfinished");
        ensure!(millis >= last, "arxiv_rate_clock_regressed");
        ensure!(millis - last >= 3000, "arxiv_rate_limited");
        sequence
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("arxiv_rate_sequence_exhausted"))?
    } else {
        1
    };
    valid(millis > 0)?;
    source::check(deadline)?;
    io(file.seek(SeekFrom::Start(((sequence - 1) % 2) * 64)))?;
    io(file.write_all(&slot(sequence, millis)))?;
    io(file.sync_all())?;
    source::check(deadline)?;
    // A crash leaves either prior valid state or a corrupt slot that refuses
    // further admission. No recovery/truncation silently resets the interval.
    Ok(Permit {
        _file: file,
        _directories: dirs,
        sequence,
        millis,
    })
}

fn existing(path: &Path, write: bool, deadline: &Deadline) -> Result<(File, Vec<File>, Vec<u8>)> {
    source::check(deadline)?;
    path_valid(path)?;
    source::input(path.file_name().and_then(|s| s.to_str()) == Some("arxiv-rate-v1.bin"))?;
    let dirs = directories(
        path.parent()
            .ok_or_else(|| anyhow::anyhow!("arxiv_rate_path_invalid"))?,
        deadline,
    )?;
    let mut options = OpenOptions::new();
    options.read(true).write(write);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(3).custom_flags(0x0020_0000);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    let mut file = io(options.open(path))?;
    let meta = io(file.metadata())?;
    valid(meta.is_file() && !crate::paths::is_link(&meta) && meta.len() <= 128)?;
    single_link(&file)?;
    let locked = if write {
        fs2::FileExt::try_lock_exclusive(&file)
    } else {
        fs2::FileExt::try_lock_shared(&file)
    };
    locked.map_err(|_| anyhow::anyhow!("arxiv_rate_coordination_busy"))?;
    valid(io(file.metadata())?.len() <= 128)?;
    single_link(&file)?;
    let mut bytes = Vec::new();
    io((&mut file).take(129).read_to_end(&mut bytes))?;
    source::check(deadline)?;
    Ok((file, dirs, bytes))
}
pub fn status(path: &Path, deadline: &Deadline) -> Result<serde_json::Value> {
    let (_file, _dirs, bytes) = existing(path, false, deadline)?;
    let prior = state(&bytes)?;
    Ok(
        serde_json::json!({"kind":"native_arxiv_rate_status_v1","journalHash":crate::model::hash(&bytes),"sequence":prior.map(|v|v.0),"lastTimestampMs":prior.map(|v|v.1),"phase":match prior {None=>"empty",Some((n,_))if n%2==1=>"pending_requires_explicit_recovery",_=>"completed"},"networkConnections":0}),
    )
}
/// Exact expected bytes, exclusive lock and an explicit host recovery grant.
/// Corrupt journals are preserved; recovery only completes a valid pending row.
pub fn recover(
    path: &Path,
    allowed: bool,
    expected_hash: &str,
    deadline: &Deadline,
) -> Result<serde_json::Value> {
    ensure!(allowed, "arxiv_rate_recovery_permission_required");
    source::input(crate::model::valid_id(expected_hash))?;
    let (mut file, _dirs, bytes) = existing(path, true, deadline)?;
    ensure!(
        crate::model::hash(&bytes) == expected_hash,
        "arxiv_rate_recovery_stale"
    );
    let (sequence, last) =
        state(&bytes)?.ok_or_else(|| anyhow::anyhow!("arxiv_rate_no_pending_request"))?;
    ensure!(sequence % 2 == 1, "arxiv_rate_no_pending_request");
    let millis = u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| anyhow::anyhow!("arxiv_rate_clock_invalid"))?
            .as_millis(),
    )
    .map_err(|_| anyhow::anyhow!("arxiv_rate_clock_invalid"))?;
    ensure!(millis >= last, "arxiv_rate_clock_regressed");
    let next = sequence
        .checked_add(1)
        .ok_or_else(|| anyhow::anyhow!("arxiv_rate_sequence_exhausted"))?;
    source::check(deadline)?;
    io(file.seek(SeekFrom::Start(((next - 1) % 2) * 64)))?;
    io(file.write_all(&slot(next, millis)))?;
    io(file.sync_all())?;
    io(file.seek(SeekFrom::Start(0)))?;
    let mut current = Vec::new();
    io((&mut file).take(129).read_to_end(&mut current))?;
    valid(current.len() <= 128)?;
    Ok(
        serde_json::json!({"kind":"native_arxiv_rate_recovery_v1","journalHash":crate::model::hash(&current),"sequence":next,"completedAtMs":millis,"cooldownMs":3000,"coordinationUpdated":true,"networkConnections":0}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn now() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    }
    fn deadline() -> Deadline {
        Deadline::new(30000)
    }
    fn completed(path: &Path, timestamp: u64) {
        let bytes = [slot(1, timestamp), slot(2, timestamp)].concat();
        fs::write(path, bytes).unwrap();
    }
    #[test]
    fn journal_holds_exclusive_lock_and_completion_starts_cooldown() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("arxiv-rate-v1.bin");
        let permit = admit(&path, true, &deadline()).unwrap();
        assert_eq!(state(&fs::read(&path).unwrap()).unwrap().unwrap().0, 1);
        assert!(format!("{}", admit(&path, true, &deadline()).err().unwrap()).contains("busy"));
        assert!(status(&path, &deadline()).is_err());
        let (sequence, stamp) = permit.complete().unwrap();
        assert_eq!(sequence, 2);
        assert_eq!(status(&path, &deadline()).unwrap()["phase"], "completed");
        let bytes = fs::read(&path).unwrap();
        for (time, error) in [
            (stamp - 1, "clock_regressed"),
            (stamp + 2999, "rate_limited"),
        ] {
            assert!(
                format!("{}", admit_at(&path, time, &deadline()).err().unwrap()).contains(error)
            );
            assert_eq!(fs::read(&path).unwrap(), bytes);
        }
        let next = admit_at(&path, stamp + 3000, &deadline()).unwrap();
        assert_eq!(next.sequence, 3);
        drop(next);
        assert_eq!(
            status(&path, &deadline()).unwrap()["phase"],
            "pending_requires_explicit_recovery"
        );
    }
    #[test]
    fn abandoned_request_requires_explicit_exact_hash_recovery() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("arxiv-rate-v1.bin");
        drop(admit(&path, true, &deadline()).unwrap());
        let bytes = fs::read(&path).unwrap();
        let hash = crate::model::hash(&bytes);
        assert!(admit_at(&path, now() + 10000, &deadline()).is_err());
        assert!(recover(&path, false, &hash, &deadline()).is_err());
        assert!(recover(&path, true, &crate::model::hash(b"stale"), &deadline()).is_err());
        assert_eq!(fs::read(&path).unwrap(), bytes);
        let v = recover(&path, true, &hash, &deadline()).unwrap();
        assert_eq!(v["sequence"], 2);
        assert_eq!(v["networkConnections"], 0);
        let current = fs::read(&path).unwrap();
        assert_ne!(current, bytes);
        assert!(recover(&path, true, v["journalHash"].as_str().unwrap(), &deadline()).is_err());
        assert!(admit(&path, true, &deadline()).is_err());
        assert_eq!(fs::read(&path).unwrap(), current);
    }
    #[test]
    fn corrupt_partial_or_reordered_slots_are_never_reset_or_recovered() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("arxiv-rate-v1.bin");
        let t = now() - 10000;
        let mut corrupt = slot(1, t).to_vec();
        corrupt[40] ^= 1;
        for bytes in [
            vec![0; 1],
            vec![0; 129],
            corrupt,
            slot(2, t).to_vec(),
            [slot(3, t), slot(2, t - 1)].concat(),
            [slot(1, t), slot(4, t)].concat(),
            [slot(1, t), slot(2, t - 1)].concat(),
        ] {
            fs::write(&path, &bytes).unwrap();
            assert!(admit(&path, true, &deadline()).is_err());
            assert!(status(&path, &deadline()).is_err());
            assert!(recover(&path, true, &crate::model::hash(&bytes), &deadline()).is_err());
            assert_eq!(fs::read(&path).unwrap(), bytes);
        }
    }
    #[test]
    fn permissions_paths_deadlines_and_hardlinks_refuse_before_state_mutation() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("owned-\u{1f4a1}");
        fs::create_dir(&folder).unwrap();
        let path = folder.join("arxiv-rate-v1.bin");
        assert!(admit(&path, false, &deadline()).is_err());
        assert!(!path.exists());
        assert!(admit(&path, true, &Deadline::new(0)).is_err());
        assert!(!path.exists());
        assert!(admit(&folder.join("wrong.bin"), true, &deadline()).is_err());
        assert!(admit(Path::new("arxiv-rate-v1.bin"), true, &deadline()).is_err());
        completed(&path, now() - 10000);
        let bytes = fs::read(&path).unwrap();
        let alias = folder.join("alias.bin");
        fs::hard_link(&path, &alias).unwrap();
        assert!(admit(&path, true, &deadline()).is_err());
        assert!(status(&path, &deadline()).is_err());
        assert_eq!(fs::read(&path).unwrap(), bytes);
        fs::remove_file(alias).unwrap();
        let permit = admit(&path, true, &deadline()).unwrap();
        permit.complete().unwrap();
    }
}
