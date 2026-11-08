// Native explicit, immutable media inspection and transcript ingestion.
use crate::{
    Engine,
    capture::Deadline,
    model::{hash, identity, valid_id},
    paths::{is_link, normalized, obvious_secret, scoped},
};
use anyhow::{Context, Result, ensure};
use ignore::WalkBuilder;
use rusqlite::{OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

const MAX_ASSET: usize = 50_000_000;
const MAX_TEXT: usize = 2_000_000;
const PRODUCER: &str = "yoke-native-media-1";
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Asset {
    pub(crate) id: String,
    pub(crate) workspace_id: String,
    pub(crate) path: String,
    pub(crate) source_hash: String,
    pub(crate) blob_id: String,
    pub(crate) size: usize,
}

pub(crate) fn validate_asset(asset: &Asset) -> Result<()> {
    normalized(&asset.path)?;
    ensure!(
        valid_id(&asset.workspace_id)
            && valid_id(&asset.source_hash)
            && asset.blob_id == asset.source_hash
            && asset.size <= MAX_ASSET
            && asset.id
                == identity(&(
                    asset.workspace_id.as_str(),
                    asset.path.as_str(),
                    &asset.source_hash
                )),
        "Explicit asset identity/size mismatch"
    );
    Ok(())
}
/// Read a registered immutable asset without reopening its original filesystem path.
pub(crate) fn checked_asset(
    store: &crate::store::Store,
    id: &str,
    deadline: &Deadline,
) -> Result<(Asset, Vec<u8>)> {
    ensure!(valid_id(id), "Invalid explicit asset ID");
    store.set_deadline(deadline)?;
    let limit = rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH;
    let prior = store
        .db
        .set_limit(limit, store.db.limit(limit)?.min(64000))?;
    let raw: rusqlite::Result<String> = store.db.query_row(
        "SELECT value FROM records WHERE kind='native_media_asset_v1' AND id=?1",
        [id],
        |row| row.get(0),
    );
    store.db.set_limit(limit, prior)?;
    let raw = raw?;
    let asset: Asset = crate::json::decode(crate::json::parse(raw.as_bytes(), 64000)?)?;
    validate_asset(&asset)?;
    ensure!(
        asset.id == id && asset.workspace_id == store.workspace_id,
        "Foreign explicit asset registration"
    );
    let bytes = store.read_blob_exact(&asset.blob_id, asset.size, MAX_ASSET)?;
    deadline.check()?;
    Ok((asset, bytes))
}

fn policy_file(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(meta) => ensure!(
            meta.is_file() && !is_link(&meta) && meta.len() <= 1_000_000,
            "Unsupported or linked ignore policy"
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(())
}
fn allowed(engine: &Engine, path: &str, deadline: &Deadline) -> Result<PathBuf> {
    let path = normalized(path)?;
    let absolute = scoped(&engine.root, &path)?;
    // Ignore files are policy data; links cannot make them an external read path.
    for parent in absolute
        .parent()
        .context("Asset parent missing")?
        .ancestors()
    {
        deadline.check()?;
        for name in [".gitignore", ".ignore"] {
            policy_file(&parent.join(name))?;
        }
        let git = parent.join(".git");
        if let Ok(meta) = fs::symlink_metadata(&git) {
            ensure!(
                meta.is_dir() && !is_link(&meta),
                "External or linked Git policy directories are unsupported for explicit assets"
            );
            let info = git.join("info");
            if let Ok(meta) = fs::symlink_metadata(&info) {
                ensure!(
                    meta.is_dir() && !is_link(&meta),
                    "Linked Git exclude policy directory refused"
                );
            }
            policy_file(&info.join("exclude"))?;
        }
    }
    let target = absolute.clone();
    let walker = WalkBuilder::new(&engine.root)
        .hidden(false)
        .follow_links(false)
        .git_ignore(true)
        .git_global(false)
        .git_exclude(true)
        .require_git(false)
        .filter_entry(move |entry| {
            entry.depth() == 0 || entry.path() == target || target.starts_with(entry.path())
        })
        .build();
    let mut count = 0usize;
    for entry in walker {
        deadline.check()?;
        count += 1;
        ensure!(count <= 100_000, "Media path discovery exceeded capacity");
        let entry = entry?;
        if entry.path() == absolute {
            ensure!(
                entry.file_type().is_some_and(|kind| kind.is_file()),
                "Media is not a regular file"
            );
            return Ok(absolute);
        }
    }
    anyhow::bail!("Asset is missing or excluded by source ignore policy")
}
fn read_once(path: &Path, deadline: &Deadline) -> Result<(Vec<u8>, fs::Metadata)> {
    deadline.check()?;
    let before = fs::symlink_metadata(path)?;
    ensure!(
        before.is_file() && !is_link(&before) && before.len() <= MAX_ASSET as u64,
        "Media must be a regular nonlinked file at most 50 MB"
    );
    let mut file = fs::File::open(path)?;
    let opened = file.metadata()?;
    ensure!(
        opened.len() == before.len() && opened.modified()? == before.modified()?,
        "Media changed before read"
    );
    let mut bytes = Vec::with_capacity(usize::try_from(before.len())?);
    let mut buffer = [0u8; 64 * 1024];
    loop {
        deadline.check()?;
        let length = file.read(&mut buffer)?;
        if length == 0 {
            break;
        }
        ensure!(
            bytes.len().saturating_add(length) <= MAX_ASSET,
            "Media grew past capture capacity"
        );
        bytes.extend_from_slice(&buffer[..length]);
    }
    let after = fs::symlink_metadata(path)?;
    ensure!(
        after.is_file()
            && !is_link(&after)
            && before.len() == after.len()
            && before.modified()? == after.modified()?
            && bytes.len() as u64 == before.len(),
        "Media changed during read"
    );
    Ok((bytes, after))
}
fn capture(engine: &Engine, path: &str, deadline: &Deadline) -> Result<(Asset, Vec<u8>)> {
    engine.store.set_deadline(deadline)?;
    let absolute = allowed(engine, path, deadline)?;
    let (bytes, before) = read_once(&absolute, deadline)?;
    let second_path = allowed(engine, path, deadline)?;
    ensure!(absolute == second_path, "Media path changed");
    let (second, after) = read_once(&second_path, deadline)?;
    ensure!(
        bytes == second && before.len() == after.len() && before.modified()? == after.modified()?,
        "Media did not stabilize across double read"
    );
    deadline.check()?;
    ensure!(
        !obvious_secret(&String::from_utf8_lossy(&bytes)),
        "Obvious secret content refused in explicit asset"
    );
    let source_hash = hash(&bytes);
    let asset = Asset {
        id: identity(&(engine.store.workspace_id.as_str(), path, &source_hash)),
        workspace_id: engine.store.workspace_id.clone(),
        path: path.to_string(),
        blob_id: source_hash.clone(),
        source_hash,
        size: bytes.len(),
    };
    Ok((asset, bytes))
}
fn persist(
    engine: &Engine,
    asset: &Asset,
    bytes: &[u8],
    data: &Value,
    deadline: &Deadline,
) -> Result<Value> {
    deadline.check()?;
    let raw = serde_json::to_vec(data)?;
    ensure!(raw.len() <= 8_000_000, "Media artifact exceeds capacity");
    engine.store.set_deadline(deadline)?;
    let tx =
        rusqlite::Transaction::new_unchecked(&engine.store.db, TransactionBehavior::Immediate)?;
    let old: Option<String> = tx
        .query_row(
            "SELECT value FROM records WHERE kind='native_media_asset_v1' AND id=?1",
            [&asset.id],
            |row| row.get(0),
        )
        .optional()?;
    let serialized = serde_json::to_string(asset)?;
    if let Some(old) = old {
        ensure!(old == serialized, "Existing asset identity is corrupt");
    } else {
        let (count,total):(usize,usize)=tx.query_row("SELECT count(*),coalesce(sum(json_extract(value,'$.size')),0) FROM records WHERE kind='native_media_asset_v1'",[],|row|Ok((row.get(0)?,row.get(1)?)))?;
        ensure!(
            count < 1000 && total.saturating_add(bytes.len()) <= 1_000_000_000,
            "Explicit media store capacity reached"
        );
        deadline.check()?;
        tx.execute(
            "INSERT INTO records VALUES('native_media_asset_v1',?1,?2)",
            params![asset.id, serialized],
        )?;
    }
    ensure!(
        engine.store.artifact(bytes)? == asset.source_hash,
        "Media blob hash mismatch"
    );
    let artifact_id = engine.store.artifact(&raw)?;
    engine.store.set_deadline(deadline)?;
    tx.commit()?;
    Ok(
        json!({"assetId":asset.id,"workspaceId":asset.workspace_id,"path":asset.path,"sourceHash":asset.source_hash,"blobId":asset.blob_id,"size":asset.size,"artifactId":artifact_id,"producer":PRODUCER,"modelCalls":0,"sourceWritten":false}),
    )
}
fn u16_be(bytes: &[u8], at: usize) -> Result<u16> {
    Ok(u16::from_be_bytes(
        bytes
            .get(at..at + 2)
            .context("Truncated media field")?
            .try_into()?,
    ))
}
fn u32_be(bytes: &[u8], at: usize) -> Result<u32> {
    Ok(u32::from_be_bytes(
        bytes
            .get(at..at + 4)
            .context("Truncated media field")?
            .try_into()?,
    ))
}
fn u32_le(bytes: &[u8], at: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(
        bytes
            .get(at..at + 4)
            .context("Truncated media field")?
            .try_into()?,
    ))
}
fn crc32(bytes: &[u8], deadline: &Deadline) -> Result<u32> {
    let mut crc = 0xffff_ffffu32;
    for block in bytes.chunks(64 * 1024) {
        deadline.check()?;
        for &byte in block {
            crc ^= u32::from(byte);
            for _ in 0..8 {
                crc = (crc >> 1) ^ if crc & 1 == 1 { 0xedb8_8320 } else { 0 };
            }
        }
    }
    Ok(!crc)
}
fn png(bytes: &[u8], deadline: &Deadline) -> Result<Value> {
    ensure!(
        bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        "Invalid PNG signature"
    );
    let mut cursor = 8usize;
    let mut header = None;
    let mut image_data = false;
    let mut ended = false;
    let mut chunks = 0usize;
    while cursor < bytes.len() {
        deadline.check()?;
        chunks += 1;
        ensure!(chunks <= 10_000, "PNG chunk capacity exceeded");
        let length = usize::try_from(u32_be(bytes, cursor)?)?;
        let end = cursor
            .checked_add(12)
            .and_then(|v| v.checked_add(length))
            .context("PNG chunk size overflow")?;
        ensure!(end <= bytes.len(), "Truncated PNG chunk");
        let kind = &bytes[cursor + 4..cursor + 8];
        ensure!(
            kind.iter().all(u8::is_ascii_alphabetic),
            "Invalid PNG chunk type"
        );
        ensure!(
            crc32(&bytes[cursor + 4..end - 4], deadline)? == u32_be(bytes, end - 4)?,
            "PNG chunk CRC mismatch"
        );
        if kind == b"IHDR" {
            ensure!(
                header.is_none() && cursor == 8 && length == 13,
                "Invalid PNG header placement or size"
            );
            let data = &bytes[cursor + 8..end - 4];
            let width = u32_be(data, 0)?;
            let height = u32_be(data, 4)?;
            ensure!(
                width > 0 && height > 0 && width <= 100_000 && height <= 100_000,
                "Invalid PNG dimensions"
            );
            let depth = data[8];
            let color = data[9];
            ensure!(
                match color {
                    0 => [1, 2, 4, 8, 16].contains(&depth),
                    2 | 4 | 6 => [8, 16].contains(&depth),
                    3 => [1, 2, 4, 8].contains(&depth),
                    _ => false,
                } && data[10] == 0
                    && data[11] == 0
                    && data[12] <= 1,
                "Unsupported PNG header fields"
            );
            header = Some(
                json!({"kind":"image","format":"png","width":width,"height":height,"bitDepth":depth,"colorType":color,"interlaced":data[12]==1,"validation":"container_chunks_and_crc_only"}),
            );
        } else if kind == b"IDAT" {
            ensure!(
                header.is_some() && length > 0,
                "PNG data needs a header and nonempty compressed bytes"
            );
            image_data = true;
        } else if kind == b"IEND" {
            ensure!(
                length == 0 && image_data && end == bytes.len(),
                "Invalid PNG end chunk"
            );
            ended = true;
        }
        cursor = end;
    }
    ensure!(ended, "PNG end chunk missing");
    header.context("PNG header missing")
}
fn jpeg(bytes: &[u8], deadline: &Deadline) -> Result<Value> {
    ensure!(bytes.starts_with(&[0xff, 0xd8]), "Invalid JPEG signature");
    let mut cursor = 2usize;
    let mut metadata = None;
    let mut scan = false;
    let mut markers = 0usize;
    while cursor < bytes.len() {
        deadline.check()?;
        markers += 1;
        ensure!(markers <= 100_000, "JPEG marker capacity exceeded");
        if scan {
            let mut scanned = 0usize;
            while cursor < bytes.len() && bytes[cursor] != 0xff {
                cursor += 1;
                scanned += 1;
                if scanned.is_multiple_of(65536) {
                    deadline.check()?;
                }
            }
        }
        ensure!(bytes.get(cursor) == Some(&0xff), "Missing JPEG marker");
        while bytes.get(cursor) == Some(&0xff) {
            cursor += 1;
        }
        let marker = *bytes.get(cursor).context("Truncated JPEG marker")?;
        cursor += 1;
        if marker == 0x00 {
            ensure!(scan, "JPEG stuffed byte outside scan");
            continue;
        }
        if (0xd0..=0xd7).contains(&marker) {
            ensure!(scan, "JPEG restart marker outside scan");
            continue;
        }
        if marker == 0xd9 {
            ensure!(
                scan && metadata.is_some() && cursor == bytes.len(),
                "JPEG lacks a complete structural image"
            );
            return metadata.context("JPEG frame header missing");
        }
        ensure!(
            marker != 0xd8 && marker != 0x01,
            "Unsupported JPEG standalone marker"
        );
        let length = usize::from(u16_be(bytes, cursor)?);
        ensure!(
            length >= 2 && cursor.saturating_add(length) <= bytes.len(),
            "Truncated JPEG segment"
        );
        let data = &bytes[cursor + 2..cursor + length];
        if [
            0xc0, 0xc1, 0xc2, 0xc3, 0xc5, 0xc6, 0xc7, 0xc9, 0xca, 0xcb, 0xcd, 0xce, 0xcf,
        ]
        .contains(&marker)
        {
            ensure!(
                metadata.is_none() && data.len() >= 6,
                "Invalid JPEG frame header"
            );
            let height = u16_be(data, 1)?;
            let width = u16_be(data, 3)?;
            let components = data[5];
            ensure!(
                width > 0
                    && height > 0
                    && (1..=4).contains(&components)
                    && data.len() == 6 + 3 * usize::from(components),
                "Invalid JPEG dimensions or frame components"
            );
            metadata = Some(
                json!({"kind":"image","format":"jpeg","width":width,"height":height,"precision":data[0],"components":components,"progressive":marker==0xc2,"validation":"structural_headers_only"}),
            );
        }
        if marker == 0xda {
            ensure!(
                metadata.is_some() && data.len() >= 6,
                "JPEG scan requires a frame header"
            );
            let count = usize::from(data[0]);
            ensure!(
                count > 0 && count <= 4 && data.len() == 1 + 2 * count + 3,
                "Invalid JPEG scan header"
            );
            scan = true;
        }
        cursor += length;
    }
    anyhow::bail!("JPEG end marker missing")
}
fn wav(bytes: &[u8], deadline: &Deadline) -> Result<Value> {
    ensure!(
        bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WAVE"),
        "Invalid RIFF/WAVE signature"
    );
    ensure!(
        usize::try_from(u32_le(bytes, 4)?)?.checked_add(8) == Some(bytes.len()),
        "WAV RIFF size mismatch"
    );
    let mut cursor = 12usize;
    let mut format = None;
    let mut data_size = 0usize;
    let mut data_seen = false;
    let mut chunks = 0usize;
    while cursor < bytes.len() {
        deadline.check()?;
        chunks += 1;
        ensure!(
            chunks <= 10_000 && cursor + 8 <= bytes.len(),
            "Truncated WAV chunk or capacity exceeded"
        );
        let length = usize::try_from(u32_le(bytes, cursor + 4)?)?;
        let start = cursor + 8;
        let end = start
            .checked_add(length)
            .context("WAV chunk size overflow")?;
        ensure!(end <= bytes.len(), "Truncated WAV chunk");
        let kind = &bytes[cursor..cursor + 4];
        if kind == b"fmt " {
            ensure!(format.is_none() && length >= 16, "Invalid WAV format chunk");
            let data = &bytes[start..end];
            let encoding = u16::from_le_bytes(data[0..2].try_into()?);
            let channels = u16::from_le_bytes(data[2..4].try_into()?);
            let rate = u32_le(data, 4)?;
            let byte_rate = u32_le(data, 8)?;
            let alignment = u16::from_le_bytes(data[12..14].try_into()?);
            let bits = u16::from_le_bytes(data[14..16].try_into()?);
            ensure!(
                [1, 3].contains(&encoding)
                    && channels > 0
                    && channels <= 64
                    && rate > 0
                    && rate <= 768000
                    && [8, 16, 24, 32, 64].contains(&bits),
                "Unsupported WAV sample format"
            );
            ensure!(
                (encoding != 3 || [32, 64].contains(&bits))
                    && u32::from(alignment) == u32::from(channels) * u32::from(bits) / 8
                    && byte_rate
                        == rate
                            .checked_mul(u32::from(alignment))
                            .context("WAV byte-rate overflow")?,
                "WAV format rate or frame alignment mismatch"
            );
            format = Some((encoding, channels, rate, alignment, bits));
        } else if kind == b"data" {
            ensure!(format.is_some(), "WAV data must follow its format");
            data_size = data_size
                .checked_add(length)
                .context("WAV data size overflow")?;
            data_seen = true;
        }
        cursor = end
            .checked_add(length % 2)
            .context("WAV chunk pad overflow")?;
        ensure!(cursor <= bytes.len(), "WAV chunk pad missing");
    }
    let (encoding, channels, rate, alignment, bits) = format.context("WAV format chunk missing")?;
    ensure!(
        data_seen && data_size.is_multiple_of(usize::from(alignment)),
        "WAV data is missing or ends inside a sample frame"
    );
    let frames = data_size / usize::from(alignment);
    Ok(
        json!({"kind":"audio","format":"wav","encoding":if encoding==1 {"pcm"}else{"ieee_float"},"channels":channels,"sampleRate":rate,"bitsPerSample":bits,"frames":frames,"durationSeconds":frames as f64/f64::from(rate),"validation":"riff_structure_and_sample_layout_only"}),
    )
}
fn mp4_boxes(
    bytes: &[u8],
    depth: usize,
    count: &mut usize,
    metadata: &mut Value,
    deadline: &Deadline,
) -> Result<()> {
    ensure!(depth <= 8, "MP4 nesting capacity exceeded");
    let mut cursor = 0usize;
    while cursor < bytes.len() {
        deadline.check()?;
        *count += 1;
        ensure!(
            *count <= 10_000 && cursor + 8 <= bytes.len(),
            "Truncated MP4 box or capacity exceeded"
        );
        let declared = u32_be(bytes, cursor)?;
        let kind = &bytes[cursor + 4..cursor + 8];
        let mut header = 8usize;
        let size = if declared == 1 {
            header = 16;
            usize::try_from(u64::from_be_bytes(
                bytes
                    .get(cursor + 8..cursor + 16)
                    .context("Truncated MP4 large box")?
                    .try_into()?,
            ))?
        } else if declared == 0 {
            bytes.len() - cursor
        } else {
            usize::try_from(declared)?
        };
        ensure!(
            size >= header && cursor.saturating_add(size) <= bytes.len(),
            "Invalid or truncated MP4 box size"
        );
        let data = &bytes[cursor + header..cursor + size];
        if kind == b"ftyp" {
            ensure!(
                depth == 0
                    && metadata.get("majorBrand").is_none()
                    && data.len() >= 8
                    && (data.len() - 8).is_multiple_of(4),
                "Invalid MP4 file-type box"
            );
            let brand = std::str::from_utf8(&data[..4])?;
            ensure!(
                brand.chars().all(|c| c.is_ascii_alphanumeric() || c == ' '),
                "Unsupported MP4 brand"
            );
            metadata["majorBrand"] = json!(brand);
        } else if kind == b"moov" {
            ensure!(depth == 0, "Invalid nested MP4 movie box");
            metadata["movieBoxPresent"] = json!(true);
            mp4_boxes(data, depth + 1, count, metadata, deadline)?;
        } else if kind == b"mvhd" {
            ensure!(
                depth == 1 && metadata.get("durationSeconds").is_none() && data.len() >= 20,
                "Invalid MP4 movie header"
            );
            let version = data[0];
            let (scale, duration) = match version {
                0 => (u32_be(data, 12)?, u64::from(u32_be(data, 16)?)),
                1 => {
                    ensure!(data.len() >= 32, "Truncated MP4 version-1 header");
                    (
                        u32_be(data, 20)?,
                        u64::from_be_bytes(data[24..32].try_into()?),
                    )
                }
                _ => anyhow::bail!("Unsupported MP4 movie-header version"),
            };
            ensure!(scale > 0, "MP4 time scale must be positive");
            metadata["timeScale"] = json!(scale);
            if duration == u64::MAX || (version == 0 && duration == u64::from(u32::MAX)) {
                metadata["durationSeconds"] = Value::Null;
            } else {
                metadata["durationSeconds"] = json!(duration as f64 / f64::from(scale));
            }
        } else if [b"trak".as_slice(), b"mdia", b"minf", b"stbl"].contains(&kind) {
            mp4_boxes(data, depth + 1, count, metadata, deadline)?;
        }
        cursor += size;
    }
    Ok(())
}
fn mp4(bytes: &[u8], deadline: &Deadline) -> Result<Value> {
    let mut metadata =
        json!({"kind":"audiovisual","format":"mp4","validation":"iso_bmff_container_headers_only"});
    let mut count = 0usize;
    mp4_boxes(bytes, 0, &mut count, &mut metadata, deadline)?;
    ensure!(
        metadata.get("majorBrand").is_some() && metadata.get("durationSeconds").is_some(),
        "MP4 file-type or movie-duration header missing"
    );
    Ok(metadata)
}
fn technical(path: &str, bytes: &[u8], deadline: &Deadline) -> Result<Value> {
    let extension = Path::new(path)
        .extension()
        .and_then(|v| v.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match extension.as_str() {
        "png" => png(bytes, deadline),
        "jpg" | "jpeg" => jpeg(bytes, deadline),
        "wav" => wav(bytes, deadline),
        "mp4" | "mov" | "m4a" => mp4(bytes, deadline),
        "pdf" => {
            ensure!(
                bytes.starts_with(b"%PDF-") && bytes.windows(5).any(|w| w == b"%%EOF"),
                "Invalid or truncated PDF structure"
            );
            Ok(
                json!({"kind":"document","format":"pdf","nativeTextAvailable":false,"validation":"signature_and_end_marker_only"}),
            )
        }
        _ => anyhow::bail!("Unsupported native media format"),
    }
}
pub(crate) fn office_inspection_data(
    asset: &Asset,
    bytes: &[u8],
    deadline: &Deadline,
) -> Result<Value> {
    let extension = Path::new(&asset.path)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let extracted = crate::office::extract(bytes, &extension, deadline)?;
    Ok(
        json!({"kind":"media_inspection","workspaceId":asset.workspace_id,"assetId":asset.id,"path":asset.path,"sourceHash":asset.source_hash,"blobId":asset.blob_id,"metadata":extracted["metadata"],"segments":extracted["segments"],"producer":PRODUCER,"captureConsistency":"stable_double_read","contentInterpretation":"native_openxml_text_and_cached_values","limitations":extracted["limitations"]}),
    )
}
pub fn inspect(engine: &Engine, path: &str, deadline: &Deadline) -> Result<Value> {
    let (asset, bytes) = capture(engine, path, deadline)?;
    let extension = Path::new(path)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if matches!(extension.as_str(), "docx" | "xlsx") {
        let data = office_inspection_data(&asset, &bytes, deadline)?;
        let mut result = persist(engine, &asset, &bytes, &data, deadline)?;
        result["metadata"] = data["metadata"].clone();
        result["segments"] = data["segments"].clone();
        result["sourceHash"] = json!(asset.source_hash);
        result["contentInterpretation"] = data["contentInterpretation"].clone();
        return Ok(result);
    }
    let metadata = technical(path, &bytes, deadline)?;
    let data = json!({"kind":"media_inspection","workspaceId":asset.workspace_id,"assetId":asset.id,"path":asset.path,"sourceHash":asset.source_hash,"blobId":asset.blob_id,"metadata":metadata,"producer":PRODUCER,"captureConsistency":"stable_double_read","contentInterpretation":"not_performed","limitations":["Technical metadata is not OCR, speech recognition, visual interpretation or proof of a decodable media stream.","Native PDF text extraction requires the explicitly selected bounded native worker."]});
    let mut result = persist(engine, &asset, &bytes, &data, deadline)?;
    result["metadata"] = metadata;
    result["contentInterpretation"] = json!("not_performed");
    Ok(result)
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TranscriptInput {
    expected_hash: String,
    producer: String,
    text: String,
    language: Option<String>,
}
pub fn ingest_transcript(
    engine: &Engine,
    path: &str,
    input: Value,
    deadline: &Deadline,
) -> Result<Value> {
    ensure!(
        serde_json::to_vec(&input)?.len() <= MAX_TEXT + 4000,
        "Transcript input exceeds capacity"
    );
    let input: TranscriptInput = serde_json::from_value(input)?;
    ensure!(
        valid_id(&input.expected_hash)
            && !input.text.trim().is_empty()
            && input.text.len() <= MAX_TEXT
            && !obvious_secret(&input.text)
            && !input
                .text
                .chars()
                .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t')),
        "Invalid, empty or secret transcript"
    );
    ensure!(
        !input.producer.trim().is_empty()
            && input.producer.len() <= 200
            && !input.producer.chars().any(char::is_control)
            && !obvious_secret(&input.producer),
        "Explicit bounded transcript producer required"
    );
    if let Some(language) = &input.language {
        ensure!(
            !language.is_empty() && language.len() <= 64 && !language.chars().any(char::is_control),
            "Invalid transcript language label"
        );
    }
    let (asset, bytes) = capture(engine, path, deadline)?;
    ensure!(
        asset.source_hash == input.expected_hash,
        "Transcript asset hash is stale"
    );
    let metadata = technical(path, &bytes, deadline)?;
    let data = json!({"kind":"media_transcript","workspaceId":asset.workspace_id,"assetId":asset.id,"path":asset.path,"sourceHash":asset.source_hash,"blobId":asset.blob_id,"metadata":metadata,"text":input.text,"producer":input.producer,"language":input.language,"origin":"host_supplied_transcript_not_verified_against_asset","trust":"authored_data_not_execution_authority","modelCalls":0,"sourceWritten":false});
    let mut result = persist(engine, &asset, &bytes, &data, deadline)?;
    result["origin"] = data["origin"].clone();
    result["producer"] = data["producer"].clone();
    Ok(result)
}

const PDF_MEMORY: u64 = 256 * 1024 * 1024;
const PDF_INPUT: usize = 10_000_000;
const PDF_OUTPUT: usize = 8_000_000;
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ResourceLimits {
    memory_bytes: u64,
    cpu_seconds: u32,
    memory_enforcement: String,
}
#[cfg(windows)]
fn pdf_limits() -> Result<ResourceLimits> {
    use std::ffi::c_void;
    #[repr(C)]
    #[derive(Default)]
    struct Basic {
        process_time: i64,
        job_time: i64,
        flags: u32,
        min_working: usize,
        max_working: usize,
        active: u32,
        affinity: usize,
        priority: u32,
        scheduling: u32,
    }
    #[repr(C)]
    #[derive(Default)]
    struct Counters {
        read_ops: u64,
        write_ops: u64,
        other_ops: u64,
        read_bytes: u64,
        write_bytes: u64,
        other_bytes: u64,
    }
    #[repr(C)]
    #[derive(Default)]
    struct Extended {
        basic: Basic,
        io: Counters,
        process_memory: usize,
        job_memory: usize,
        peak_process: usize,
        peak_job: usize,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateJobObjectW(attributes: *mut c_void, name: *const u16) -> *mut c_void;
        fn SetInformationJobObject(
            job: *mut c_void,
            class: i32,
            value: *const c_void,
            length: u32,
        ) -> i32;
        fn QueryInformationJobObject(
            job: *mut c_void,
            class: i32,
            value: *mut c_void,
            length: u32,
            written: *mut u32,
        ) -> i32;
        fn AssignProcessToJobObject(job: *mut c_void, process: *mut c_void) -> i32;
        fn GetCurrentProcess() -> *mut c_void;
        fn CloseHandle(handle: *mut c_void) -> i32;
        fn SetErrorMode(mode: u32) -> u32;
    }
    let limits = Extended {
        basic: Basic {
            process_time: 20 * 10_000_000,
            flags: 0x2 | 0x8 | 0x100 | 0x200 | 0x400,
            active: 1,
            ..Basic::default()
        },
        process_memory: PDF_MEMORY as usize,
        job_memory: PDF_MEMORY as usize,
        ..Extended::default()
    };
    // This is called only by the isolated worker before parsing. The self-owned
    // nested job survives until process exit; closing it here would remove limits.
    unsafe {
        SetErrorMode(0x1 | 0x2 | 0x8000);
        let job = CreateJobObjectW(std::ptr::null_mut(), std::ptr::null());
        ensure!(!job.is_null(), "Cannot create bounded PDF worker job");
        let set = SetInformationJobObject(
            job,
            9,
            (&limits as *const Extended).cast(),
            std::mem::size_of::<Extended>() as u32,
        );
        let assigned = set != 0 && AssignProcessToJobObject(job, GetCurrentProcess()) != 0;
        if !assigned {
            let error = std::io::Error::last_os_error();
            CloseHandle(job);
            return Err(error).context("Cannot enforce PDF worker memory limits");
        }
        let mut installed = Extended::default();
        let queried = QueryInformationJobObject(
            job,
            9,
            (&mut installed as *mut Extended).cast(),
            std::mem::size_of::<Extended>() as u32,
            std::ptr::null_mut(),
        );
        ensure!(
            queried != 0
                && installed.basic.flags & limits.basic.flags == limits.basic.flags
                && installed.process_memory == PDF_MEMORY as usize
                && installed.job_memory == PDF_MEMORY as usize
                && installed.basic.active == 1
                && installed.basic.process_time == limits.basic.process_time,
            "PDF worker resource limits could not be verified"
        );
    }
    Ok(ResourceLimits {
        memory_bytes: PDF_MEMORY,
        cpu_seconds: 20,
        memory_enforcement: "windows_job_object".into(),
    })
}
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn pdf_limits() -> Result<ResourceLimits> {
    #[repr(C)]
    struct Limit {
        current: u64,
        maximum: u64,
    }
    unsafe extern "C" {
        fn setrlimit(resource: i32, value: *const Limit) -> i32;
        fn getrlimit(resource: i32, value: *mut Limit) -> i32;
    }
    #[cfg(target_os = "linux")]
    let address_space = 9;
    #[cfg(target_os = "macos")]
    let address_space = 5;
    let memory = Limit {
        current: PDF_MEMORY,
        maximum: PDF_MEMORY,
    };
    let cpu = Limit {
        current: 20,
        maximum: 21,
    };
    // Hard limits remain local to this isolated native worker.
    unsafe {
        ensure!(
            setrlimit(address_space, &memory) == 0 && setrlimit(0, &cpu) == 0,
            "Cannot enforce PDF worker resource limits: {}",
            std::io::Error::last_os_error()
        );
        let mut installed_memory = Limit {
            current: 0,
            maximum: 0,
        };
        let mut installed_cpu = Limit {
            current: 0,
            maximum: 0,
        };
        ensure!(
            getrlimit(address_space, &mut installed_memory) == 0
                && getrlimit(0, &mut installed_cpu) == 0
                && installed_memory.current == PDF_MEMORY
                && installed_memory.maximum == PDF_MEMORY
                && installed_cpu.current == 20
                && installed_cpu.maximum == 21,
            "PDF worker resource limits could not be verified"
        );
    }
    Ok(ResourceLimits {
        memory_bytes: PDF_MEMORY,
        cpu_seconds: 20,
        memory_enforcement: "rlimit_as".into(),
    })
}
#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
fn pdf_limits() -> Result<ResourceLimits> {
    anyhow::bail!("Native PDF worker resource limits are unavailable on this platform")
}

struct LimitedText {
    bytes: Vec<u8>,
    limit: usize,
}
impl std::io::Write for LimitedText {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.bytes.len().saturating_add(bytes.len()) > self.limit {
            return Err(std::io::Error::other("PDF text capacity exceeded"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

struct PdfCodeSpace {
    width: usize,
    first: u32,
    last: u32,
}
struct PdfFont<'a> {
    encoding: lopdf::Encoding<'a>,
    spaces: Vec<PdfCodeSpace>,
}

fn pdf_code(bytes: &[u8]) -> u32 {
    bytes
        .iter()
        .fold(0u32, |code, byte| (code << 8) | u32::from(*byte))
}

fn pdf_code_spaces(plain: &[u8]) -> Result<Vec<PdfCodeSpace>> {
    let content = lopdf::content::Content::decode_strict(plain)?;
    ensure!(
        content.operations.len() <= 100_000,
        "PDF CMap operation capacity exceeded"
    );
    let mut pending = None;
    let mut spaces = Vec::new();
    for operation in content.operations {
        if operation.operator == "begincodespacerange" {
            ensure!(
                pending.is_none() && operation.operands.len() == 1,
                "Malformed PDF code-space declaration"
            );
            let count = usize::try_from(operation.operands[0].as_i64()?)?;
            ensure!(
                count > 0 && count <= 256,
                "PDF code-space capacity exceeded"
            );
            pending = Some(count);
        } else if operation.operator == "endcodespacerange" {
            let count = pending
                .take()
                .context("Unbalanced PDF code-space declaration")?;
            ensure!(
                operation.operands.len() == count * 2 && spaces.len() + count <= 256,
                "Malformed PDF code-space range count"
            );
            for pair in operation.operands.chunks_exact(2) {
                let first = pair[0].as_str()?;
                let last = pair[1].as_str()?;
                ensure!(
                    (1..=4).contains(&first.len()) && first.len() == last.len(),
                    "Unsupported PDF code-space width"
                );
                let first_code = pdf_code(first);
                let last_code = pdf_code(last);
                ensure!(first_code <= last_code, "Reversed PDF code-space range");
                spaces.push(PdfCodeSpace {
                    width: first.len(),
                    first: first_code,
                    last: last_code,
                });
            }
        } else {
            ensure!(pending.is_none(), "Interrupted PDF code-space declaration");
        }
    }
    ensure!(
        pending.is_none() && !spaces.is_empty(),
        "PDF ToUnicode requires explicit code spaces"
    );
    for (i, a) in spaces.iter().enumerate() {
        for b in spaces.iter().skip(i + 1) {
            let (short, long) = if a.width <= b.width { (a, b) } else { (b, a) };
            let shift = (long.width - short.width) * 8;
            let first_prefix = long.first >> shift;
            let last_prefix = long.last >> shift;
            ensure!(
                short.last < first_prefix || last_prefix < short.first,
                "Ambiguous or overlapping PDF code spaces"
            );
        }
    }
    spaces.sort_by_key(|space| (space.width, space.first));
    let mut merged: Vec<PdfCodeSpace> = Vec::new();
    for space in spaces {
        if let Some(previous) = merged.last_mut().filter(|previous| {
            previous.width == space.width && previous.last.checked_add(1) == Some(space.first)
        }) {
            previous.last = space.last;
        } else {
            merged.push(space);
        }
    }
    Ok(merged)
}

fn pdf_plain_stream(stream: &lopdf::Stream) -> Result<Vec<u8>> {
    if stream.dict.has(b"Filter") {
        stream.filters()?;
    }
    let content = stream.get_plain_content()?;
    ensure!(
        content.len() <= 8_000_000,
        "PDF expanded stream capacity exceeded"
    );
    Ok(content)
}

fn pdf_font<'a>(document: &'a lopdf::Document, font: &'a lopdf::Dictionary) -> Result<PdfFont<'a>> {
    use lopdf::Encoding;
    ensure!(font.has_type(b"Font"), "Invalid PDF font resource");
    let subtype = font.get(b"Subtype")?.as_name()?;
    ensure!(
        [b"Type0".as_slice(), b"Type1", b"TrueType", b"MMType1"].contains(&subtype),
        "Unsupported PDF font subtype"
    );
    if font.has(b"ToUnicode") {
        let stream = font.get_deref(b"ToUnicode", document)?.as_stream()?;
        let plain = pdf_plain_stream(stream)?;
        ensure!(!plain.is_empty(), "Empty PDF ToUnicode map");
        let spaces = pdf_code_spaces(&plain)?;
        // The library can fall back silently after an encoding error. Require
        // an actual Unicode map and validate every used mapping below instead.
        let mut unicode_font = font.clone();
        unicode_font.remove(b"Encoding");
        return match unicode_font.get_font_encoding(document)? {
            Encoding::UnicodeMapEncoding(map) => {
                let mut ranges = 0usize;
                for (index, mappings) in map.bf_ranges.iter().enumerate() {
                    for (range, _) in mappings.iter() {
                        ranges += 1;
                        ensure!(
                            ranges <= 4096
                                && spaces.iter().any(|space| space.width == index + 1
                                    && space.first <= *range.start()
                                    && *range.end() <= space.last),
                            "PDF ToUnicode mapping exceeds capacity or lies outside declared code spaces"
                        );
                    }
                }
                ensure!(ranges > 0, "PDF ToUnicode map has no mappings");
                Ok(PdfFont {
                    encoding: Encoding::UnicodeMapEncoding(map),
                    spaces,
                })
            }
            _ => anyhow::bail!("Malformed or unsupported PDF ToUnicode map"),
        };
    }
    let name = if font.has(b"Encoding") {
        font.get_deref(b"Encoding", document)?.as_name()?
    } else {
        ensure!(
            subtype == b"Type1",
            "PDF font requires an explicit supported encoding"
        );
        let base = font.get(b"BaseFont")?.as_name()?;
        ensure!(
            [
                b"Helvetica".as_slice(),
                b"Helvetica-Bold",
                b"Helvetica-Oblique",
                b"Helvetica-BoldOblique",
                b"Courier",
                b"Courier-Bold",
                b"Courier-Oblique",
                b"Courier-BoldOblique",
                b"Times-Roman",
                b"Times-Bold",
                b"Times-Italic",
                b"Times-BoldItalic"
            ]
            .contains(&base),
            "PDF embedded or symbolic font requires an explicit supported encoding"
        );
        b"StandardEncoding"
    };
    ensure!(
        [
            b"StandardEncoding".as_slice(),
            b"WinAnsiEncoding",
            b"MacRomanEncoding",
            b"MacExpertEncoding"
        ]
        .contains(&name),
        "Unsupported PDF font encoding; no fallback was performed"
    );
    let encoding = font.get_font_encoding(document)?;
    ensure!(
        matches!(encoding, Encoding::OneByteEncoding(_)),
        "Unsupported PDF byte encoding"
    );
    Ok(PdfFont {
        encoding,
        spaces: Vec::new(),
    })
}

fn pdf_write_text(font: &PdfFont<'_>, bytes: &[u8], output: &mut LimitedText) -> Result<()> {
    use lopdf::Encoding;
    match &font.encoding {
        Encoding::OneByteEncoding(map) => {
            for chunk in bytes.chunks(4096) {
                ensure!(
                    chunk.iter().all(|byte| map[usize::from(*byte)].is_some()),
                    "PDF text uses an unmapped font character"
                );
                let text = lopdf::Document::decode_text(&Encoding::OneByteEncoding(map), chunk)?;
                ensure!(
                    text.chars().count() == chunk.len(),
                    "PDF font character decoding was incomplete"
                );
                output.write_all(text.as_bytes())?;
            }
        }
        Encoding::UnicodeMapEncoding(map) => {
            let mut offset = 0usize;
            while offset < bytes.len() {
                let (width, code) = font
                    .spaces
                    .iter()
                    .find_map(|space| {
                        let end = offset.checked_add(space.width)?;
                        let code = pdf_code(bytes.get(offset..end)?);
                        (space.first <= code && code <= space.last).then_some((space.width, code))
                    })
                    .context("Truncated or invalid PDF ToUnicode source code space")?;
                let units = map
                    .get(code, width as u8)
                    .context("Unmapped PDF ToUnicode source code")?;
                ensure!(
                    !units.is_empty() && units.len() <= 200_000 && !units.contains(&0xfffd),
                    "Invalid or replacement PDF ToUnicode mapping"
                );
                let text = String::from_utf16(&units)?;
                output.write_all(text.as_bytes())?;
                offset += width;
            }
        }
        _ => anyhow::bail!("Unsupported PDF text encoding"),
    }
    Ok(())
}

fn pdf_resources(
    document: &lopdf::Document,
    page: lopdf::ObjectId,
) -> Result<Option<&lopdf::Dictionary>> {
    let mut current = page;
    let mut seen = BTreeSet::new();
    for _ in 0..128 {
        ensure!(seen.insert(current), "Cyclic PDF page resource inheritance");
        let dictionary = document.get_dictionary(current)?;
        if dictionary.has(b"Resources") {
            return Ok(Some(
                dictionary.get_deref(b"Resources", document)?.as_dict()?,
            ));
        }
        if !dictionary.has(b"Parent") {
            return Ok(None);
        }
        current = dictionary.get(b"Parent")?.as_reference()?;
    }
    anyhow::bail!("PDF page resource inheritance exceeds capacity")
}

fn pdf_content(document: &lopdf::Document, page: lopdf::ObjectId) -> Result<Vec<u8>> {
    let dictionary = document.get_dictionary(page)?;
    if !dictionary.has(b"Contents") {
        return Ok(Vec::new());
    }
    let object = document.dereference(dictionary.get(b"Contents")?)?.1;
    let streams: Vec<_> = match object {
        lopdf::Object::Array(items) => {
            ensure!(items.len() <= 512, "PDF page stream capacity exceeded");
            items
                .iter()
                .map(|item| {
                    document
                        .dereference(item)
                        .and_then(|(_, object)| object.as_stream())
                })
                .collect::<lopdf::Result<_>>()?
        }
        lopdf::Object::Stream(stream) => vec![stream],
        _ => anyhow::bail!("Invalid PDF page content stream"),
    };
    let mut content = Vec::new();
    for stream in streams {
        let plain = pdf_plain_stream(stream)?;
        ensure!(
            content.len().saturating_add(plain.len()).saturating_add(1) <= 8_000_000,
            "PDF page expanded content capacity exceeded"
        );
        content.extend(plain);
        content.push(b'\n');
    }
    Ok(content)
}

fn pdf_numbers(objects: &[lopdf::Object]) -> Result<()> {
    for object in objects {
        let number = object.as_float()?;
        ensure!(
            number.is_finite() && number.abs() <= 1e9,
            "Invalid PDF text-state number"
        );
    }
    Ok(())
}

fn pdf_newline(output: &mut LimitedText) -> Result<()> {
    if !output.bytes.is_empty() {
        output.write_all(b"\n")?;
    }
    Ok(())
}

fn pdf_page_text(
    document: &lopdf::Document,
    page: lopdf::ObjectId,
    output: &mut LimitedText,
) -> Result<(usize, bool, bool)> {
    let resources = pdf_resources(document, page)?;
    let available = document.get_page_fonts(page)?;
    let mut fonts = BTreeMap::new();
    if let Some(resources) = resources.filter(|resources| resources.has(b"Font")) {
        let declared = resources.get_deref(b"Font", document)?.as_dict()?;
        ensure!(declared.len() <= 1024, "PDF font capacity exceeded");
        for (name, value) in declared.iter() {
            let actual = document.dereference(value)?.1.as_dict()?;
            ensure!(actual.has_type(b"Font"), "Invalid PDF font resource type");
            let resolved = available
                .get(name)
                .context("Unsupported PDF font resource inheritance")?;
            ensure!(
                std::ptr::eq(actual, *resolved),
                "PDF font resource scope mismatch"
            );
            fonts.insert(name.clone(), actual);
        }
    }
    let content = lopdf::content::Content::decode_strict(&pdf_content(document, page)?)?;
    ensure!(
        content.operations.len() <= 100_000,
        "PDF page operation capacity exceeded"
    );
    let operations = content.operations.len();
    let mut encodings = BTreeMap::new();
    let mut selected: Option<Vec<u8>> = None;
    let mut stack = Vec::new();
    let mut inside = false;
    let mut forms = false;
    let mut images = false;
    for operation in content.operations {
        let args = &operation.operands;
        match operation.operator.as_str() {
            "BT" => {
                ensure!(args.is_empty() && !inside, "Invalid nested PDF text object");
                inside = true;
            }
            "ET" => {
                ensure!(args.is_empty() && inside, "Unbalanced PDF text object");
                inside = false;
                if output.bytes.last().is_some_and(|byte| *byte != b'\n') {
                    output.write_all(b"\n")?;
                }
            }
            "Tf" => {
                ensure!(inside && args.len() == 2, "Invalid PDF font selection");
                pdf_numbers(&args[1..])?;
                let name = args[0].as_name()?.to_vec();
                if !encodings.contains_key(&name) {
                    encodings.insert(
                        name.clone(),
                        pdf_font(document, fonts.get(&name).context("Unknown PDF text font")?)?,
                    );
                }
                selected = Some(name);
            }
            "Tj" | "TJ" | "'" | "\"" => {
                let quoted = operation.operator != "Tj" && operation.operator != "TJ";
                let count = if operation.operator == "\"" { 3 } else { 1 };
                ensure!(
                    inside && args.len() == count,
                    "Invalid PDF text-show operation"
                );
                let font = encodings
                    .get(selected.as_ref().context("PDF text has no selected font")?)
                    .context("PDF selected font is unavailable")?;
                if quoted {
                    pdf_numbers(&args[..count - 1])?;
                    pdf_newline(output)?;
                }
                if operation.operator == "TJ" {
                    let parts = args[0].as_array()?;
                    ensure!(parts.len() <= 100_000, "PDF text array capacity exceeded");
                    for part in parts {
                        match part {
                            lopdf::Object::String(bytes, _) => pdf_write_text(font, bytes, output)?,
                            lopdf::Object::Integer(_) | lopdf::Object::Real(_) => {
                                pdf_numbers(std::slice::from_ref(part))?;
                                // A large negative displacement separates words;
                                // this is an explicitly disclosed layout heuristic.
                                if part.as_float()? <= -120.0
                                    && output
                                        .bytes
                                        .last()
                                        .is_some_and(|byte| !byte.is_ascii_whitespace())
                                {
                                    output.write_all(b" ")?;
                                }
                            }
                            _ => anyhow::bail!("Invalid PDF text-array member"),
                        }
                    }
                } else {
                    pdf_write_text(font, args[count - 1].as_str()?, output)?;
                }
            }
            "T*" => {
                ensure!(inside && args.is_empty(), "Invalid PDF text line advance");
                pdf_newline(output)?;
            }
            "Td" | "TD" | "Tm" => {
                ensure!(
                    inside && args.len() == if operation.operator == "Tm" { 6 } else { 2 },
                    "Invalid PDF text positioning"
                );
                pdf_numbers(args)?;
                if operation.operator != "Tm" && args[1].as_float()? != 0.0 {
                    pdf_newline(output)?;
                }
            }
            "Tc" | "Tw" | "Tz" | "TL" | "Tr" | "Ts" => {
                ensure!(
                    inside && args.len() == 1,
                    "Invalid PDF text-state operation"
                );
                pdf_numbers(args)?;
            }
            "q" => {
                ensure!(
                    !inside && args.is_empty() && stack.len() < 256,
                    "Invalid PDF graphics-state save"
                );
                stack.push(selected.clone());
            }
            "Q" => {
                ensure!(
                    !inside && args.is_empty(),
                    "Invalid PDF graphics-state restore"
                );
                selected = stack.pop().context("Unbalanced PDF graphics state")?;
            }
            "Do" => {
                ensure!(!inside && args.len() == 1, "Invalid PDF XObject invocation");
                let name = args[0].as_name()?;
                let xobjects = resources
                    .context("PDF XObject has no resource dictionary")?
                    .get_deref(b"XObject", document)?
                    .as_dict()?;
                let stream = xobjects.get_deref(name, document)?.as_stream()?;
                match stream.dict.get(b"Subtype")?.as_name()? {
                    b"Form" => forms = true,
                    b"Image" => images = true,
                    _ => anyhow::bail!("Unsupported PDF XObject subtype"),
                }
            }
            "w" | "J" | "j" | "M" | "d" | "ri" | "i" | "cm" | "m" | "l" | "c" | "v" | "y" | "h"
            | "re" | "S" | "s" | "f" | "F" | "f*" | "B" | "B*" | "b" | "b*" | "n" | "W" | "W*"
            | "CS" | "cs" | "SC" | "SCN" | "sc" | "scn" | "G" | "g" | "RG" | "rg" | "K" | "k"
            | "sh" | "BMC" | "BDC" | "EMC" | "MP" | "DP" | "BX" | "EX" => {}
            _ => anyhow::bail!("Unsupported PDF content operation; extraction was not completed"),
        }
    }
    ensure!(!inside && stack.is_empty(), "Unbalanced PDF content state");
    Ok((operations, forms, images))
}
fn pdf_pages(document: &lopdf::Document) -> Result<BTreeMap<u32, lopdf::ObjectId>> {
    fn visit(
        document: &lopdf::Document,
        node: lopdf::ObjectId,
        parent: Option<lopdf::ObjectId>,
        visited: &mut BTreeSet<lopdf::ObjectId>,
        pages: &mut Vec<lopdf::ObjectId>,
        depth: usize,
    ) -> Result<usize> {
        ensure!(
            depth <= 128 && visited.len() < 8000,
            "PDF page-tree traversal capacity exceeded"
        );
        ensure!(
            visited.insert(node),
            "Cyclic or duplicated PDF page-tree child"
        );
        let dictionary = document
            .objects
            .get(&node)
            .context("Missing PDF page-tree child")?
            .as_dict()?;
        if let Some(parent) = parent {
            ensure!(
                dictionary.get(b"Parent")?.as_reference()? == parent,
                "PDF page-tree parent mismatch"
            );
        } else {
            ensure!(
                !dictionary.has(b"Parent"),
                "PDF page-tree root has an unexpected parent"
            );
        }
        match dictionary.get(b"Type")?.as_name()? {
            b"Page" => {
                ensure!(
                    parent.is_some() && !dictionary.has(b"Kids") && pages.len() < 2000,
                    "Invalid PDF leaf page or page capacity exceeded"
                );
                pages.push(node);
                Ok(1)
            }
            b"Pages" => {
                let count = usize::try_from(dictionary.get(b"Count")?.as_i64()?)?;
                ensure!(count <= 2000, "PDF declared page count exceeds capacity");
                let children = dictionary.get_deref(b"Kids", document)?.as_array()?;
                ensure!(
                    children.len() <= 2000,
                    "PDF page-tree child capacity exceeded"
                );
                let mut actual = 0usize;
                for child in children {
                    actual = actual
                        .checked_add(visit(
                            document,
                            child.as_reference()?,
                            Some(node),
                            visited,
                            pages,
                            depth + 1,
                        )?)
                        .context("PDF page count overflow")?;
                    ensure!(actual <= 2000, "PDF page capacity exceeded");
                }
                ensure!(
                    actual == count,
                    "PDF page-tree Count differs from its children"
                );
                Ok(actual)
            }
            _ => anyhow::bail!("Invalid PDF page-tree node type"),
        }
    }
    let catalog = document.trailer.get_deref(b"Root", document)?.as_dict()?;
    ensure!(catalog.has_type(b"Catalog"), "Invalid PDF document catalog");
    let root = catalog.get(b"Pages")?.as_reference()?;
    let mut pages = Vec::new();
    visit(document, root, None, &mut BTreeSet::new(), &mut pages, 0)?;
    ensure!(!pages.is_empty(), "PDF has no pages");
    Ok(pages
        .into_iter()
        .enumerate()
        .map(|(index, page)| ((index + 1) as u32, page))
        .collect())
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PdfMetadata {
    kind: String,
    format: String,
    pages: usize,
    native_text_available: bool,
    provider: String,
    version: String,
    extraction_scope: String,
    text_order: String,
    exhaustive: bool,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PdfSegment {
    page: usize,
    text: String,
    origin: String,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PdfResult {
    schema: u8,
    source_hash: String,
    metadata: PdfMetadata,
    segments: Vec<PdfSegment>,
    warnings: Vec<String>,
    resource_limits: ResourceLimits,
}

/// Must be dispatched in a dedicated native worker before Engine opens any store.
/// This function installs process-wide resource limits and never belongs in a
/// reusable application process or in the parent MCP request thread.
pub fn pdf_worker(input: &Path) -> Result<Value> {
    let resource_limits = pdf_limits()?;
    let meta = fs::symlink_metadata(input)?;
    ensure!(
        meta.is_file() && !is_link(&meta) && meta.len() <= PDF_INPUT as u64,
        "PDF worker input must be a regular file at most 10 MB"
    );
    let bytes = fs::read(input)?;
    ensure!(
        bytes.len() <= PDF_INPUT && bytes.len() as u64 == meta.len(),
        "PDF worker input changed or grew"
    );
    let source_hash = hash(&bytes);
    let document = lopdf::Document::load_mem_with_options(
        &bytes,
        lopdf::LoadOptions {
            strict: true,
            ..lopdf::LoadOptions::default()
        },
    )?;
    ensure!(
        !document.is_encrypted(),
        "Encrypted PDF requires an explicitly decrypted copy"
    );
    let pages = pdf_pages(&document)?;
    ensure!(
        !pages.is_empty() && pages.len() <= 2000,
        "PDF page capacity exceeded or no pages found"
    );
    let mut segments = Vec::with_capacity(pages.len());
    let mut total = 0usize;
    let mut warnings = Vec::new();
    let mut total_operations = 0usize;
    for (i, (&page, &object)) in pages.iter().enumerate() {
        ensure!(page as usize == i + 1, "PDF page numbering is inconsistent");
        let mut writer = LimitedText {
            bytes: Vec::new(),
            limit: 200_000.min(MAX_TEXT.saturating_sub(total)),
        };
        let (operations, forms, images) = pdf_page_text(&document, object, &mut writer)?;
        total_operations = total_operations
            .checked_add(operations)
            .context("PDF operation size overflow")?;
        ensure!(
            total_operations <= 1_000_000,
            "PDF total operation capacity exceeded"
        );
        let text = String::from_utf8(writer.bytes)?;
        ensure!(
            !obvious_secret(&text),
            "Extracted PDF text contains an obvious secret"
        );
        let mut page_warnings = Vec::new();
        if text.trim().is_empty() {
            page_warnings.push(
                "No direct native text was found; OCR and image interpretation were not performed.",
            );
        }
        if forms {
            page_warnings.push("Form XObject text was not traversed; extraction covers direct page content streams only.");
        }
        if images {
            page_warnings.push("Image XObjects were not interpreted or processed with OCR.");
        }
        if !page_warnings.is_empty() {
            warnings.push(format!("Page {page}: {}", page_warnings.join(" ")));
        }
        total = total
            .checked_add(text.len())
            .context("PDF text size overflow")?;
        ensure!(total <= MAX_TEXT, "PDF total text capacity exceeded");
        segments.push(PdfSegment {
            page: page as usize,
            text,
            origin: "native_pdf_text_not_ocr".into(),
        });
    }
    Ok(serde_json::to_value(PdfResult {
        schema: 1,
        source_hash,
        metadata: PdfMetadata {
            kind: "document".into(),
            format: "pdf".into(),
            pages: pages.len(),
            native_text_available: true,
            provider: "yoke-native-pdf-text-1".into(),
            version: "lopdf-0.42.0".into(),
            extraction_scope: "direct_page_content_streams".into(),
            text_order: "content_order_explicit_line_breaks_heuristic_tj_spaces".into(),
            exhaustive: false,
        },
        segments,
        warnings,
        resource_limits,
    })?)
}

fn read_worker_pipe(
    mut pipe: impl Read + Send + 'static,
    limit: usize,
    sender: std::sync::mpsc::SyncSender<(bool, Result<Vec<u8>>)>,
    stdout: bool,
) {
    std::thread::spawn(move || {
        let result = (|| -> Result<Vec<u8>> {
            let mut bytes = Vec::new();
            let mut buffer = [0u8; 8192];
            loop {
                let count = pipe.read(&mut buffer)?;
                if count == 0 {
                    break;
                }
                ensure!(
                    bytes.len().saturating_add(count) <= limit,
                    "Native PDF worker output capacity exceeded"
                );
                bytes.extend_from_slice(&buffer[..count]);
            }
            Ok(bytes)
        })();
        let _ = sender.send((stdout, result));
    });
}
pub fn inspect_with_worker(
    engine: &Engine,
    path: &str,
    worker_exe: &Path,
    deadline: &Deadline,
) -> Result<Value> {
    if !Path::new(path)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("pdf"))
    {
        return inspect(engine, path, deadline);
    }
    let (asset, bytes) = capture(engine, path, deadline)?;
    ensure!(
        bytes.len() <= PDF_INPUT,
        "Native PDF worker input exceeds 10 MB"
    );
    ensure!(
        worker_exe.is_absolute() && fs::metadata(worker_exe)?.is_file(),
        "Explicit absolute native worker executable required"
    );
    let temporary = tempfile::Builder::new()
        .prefix("yoke-media-pdf-")
        .tempdir()?;
    let copy = temporary.path().join("input.pdf");
    fs::write(&copy, &bytes)?;
    let mut permissions = fs::metadata(&copy)?.permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&copy, permissions)?;
    deadline.check()?;
    let mut command = std::process::Command::new(worker_exe);
    command
        .arg("media-pdf-worker")
        .arg("--input")
        .arg(&copy)
        .current_dir(temporary.path())
        .env_clear()
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    for key in ["SystemRoot", "WINDIR", "TEMP", "TMP", "LANG"] {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    let mut child = crate::process::ManagedChild::spawn(command)?;
    let stdout = child
        .stdout()
        .take()
        .context("Native PDF worker stdout missing")?;
    let stderr = child
        .stderr()
        .take()
        .context("Native PDF worker stderr missing")?;
    let (sender, receiver) = std::sync::mpsc::sync_channel(2);
    read_worker_pipe(stdout, PDF_OUTPUT, sender.clone(), true);
    read_worker_pipe(stderr, 64_000, sender, false);
    let mut output = None;
    let mut error_output = None;
    let mut status = None;
    while status.is_none() || output.is_none() || error_output.is_none() {
        deadline.check()?;
        status = child.try_wait()?.or(status);
        match receiver.recv_timeout(
            deadline
                .remaining()
                .min(std::time::Duration::from_millis(10)),
        ) {
            Ok((true, value)) => output = Some(value?),
            Ok((false, value)) => error_output = Some(value?),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => ensure!(
                output.is_some() && error_output.is_some(),
                "Native PDF worker pipes disconnected"
            ),
        }
    }
    ensure!(
        status
            .context("Native PDF worker status missing")?
            .success(),
        "Native PDF worker failed; malformed, encrypted or resource-limited PDF"
    );
    ensure!(
        hash(fs::read(&copy)?) == asset.source_hash,
        "Native PDF worker modified its immutable input copy"
    );
    let parsed: PdfResult =
        serde_json::from_slice(&output.context("Native PDF worker response missing")?)?;
    ensure!(
        parsed.schema == 1
            && parsed.source_hash == asset.source_hash
            && parsed.metadata.kind == "document"
            && parsed.metadata.format == "pdf"
            && parsed.metadata.native_text_available
            && parsed.metadata.provider == "yoke-native-pdf-text-1"
            && parsed.metadata.version == "lopdf-0.42.0"
            && parsed.metadata.extraction_scope == "direct_page_content_streams"
            && parsed.metadata.text_order
                == "content_order_explicit_line_breaks_heuristic_tj_spaces"
            && !parsed.metadata.exhaustive,
        "Native PDF worker source/format contract mismatch"
    );
    ensure!(
        parsed.metadata.pages > 0
            && parsed.metadata.pages <= 2000
            && parsed.segments.len() == parsed.metadata.pages
            && parsed.resource_limits.memory_bytes == PDF_MEMORY
            && parsed.resource_limits.cpu_seconds == 20
            && ["windows_job_object", "rlimit_as"]
                .contains(&parsed.resource_limits.memory_enforcement.as_str()),
        "Native PDF worker page/resource contract mismatch"
    );
    let mut total = 0usize;
    for (i, segment) in parsed.segments.iter().enumerate() {
        deadline.check()?;
        ensure!(
            segment.page == i + 1
                && segment.origin == "native_pdf_text_not_ocr"
                && segment.text.len() <= 200_000
                && !obvious_secret(&segment.text),
            "Native PDF worker segment contract mismatch"
        );
        total = total
            .checked_add(segment.text.len())
            .context("PDF text size overflow")?;
        ensure!(
            total <= MAX_TEXT,
            "Native PDF worker text capacity exceeded"
        );
    }
    ensure!(
        parsed.warnings.len() <= 2000
            && parsed
                .warnings
                .iter()
                .all(|warning| warning.len() <= 512 && !obvious_secret(warning)),
        "Native PDF worker warning capacity exceeded"
    );
    let data = json!({"kind":"media_inspection","workspaceId":asset.workspace_id,"assetId":asset.id,"path":asset.path,"sourceHash":asset.source_hash,"blobId":asset.blob_id,"metadata":parsed.metadata,"segments":parsed.segments,"warnings":parsed.warnings,"resourceLimits":parsed.resource_limits,"producer":PRODUCER,"captureConsistency":"stable_double_read","contentInterpretation":"native_pdf_text_only","limitations":["PDF text extraction reads native text operators and font mappings; it does not perform OCR or visual interpretation.","The native worker is resource-limited and copied-file-only, not a general OS sandbox."]});
    let mut result = persist(engine, &asset, &bytes, &data, deadline)?;
    result["metadata"] = data["metadata"].clone();
    result["contentInterpretation"] = data["contentInterpretation"].clone();
    Ok(result)
}
