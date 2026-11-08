//! Explicit host selection. Captured source/configuration never grants execution.
use crate::{
    capture::Deadline,
    model::{identity, valid_id},
    paths::{is_link, normalized_syntax, obvious_secret},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
};

const MODEL_NAMES: &[&str] = &[
    "config.json",
    "model.bin",
    "tokenizer.json",
    "vocabulary.txt",
    "vocabulary.json",
    "preprocessor_config.json",
];

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelFile {
    pub name: String,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Limits {
    pub max_audio_milliseconds: u64,
    pub max_segments: usize,
    pub max_transcript_bytes: usize,
    pub max_jobs: usize,
    pub threads: u32,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_audio_milliseconds: 600_000,
            max_segments: 512,
            max_transcript_bytes: 1_000_000,
            max_jobs: 1000,
            threads: 2,
        }
    }
}
fn isolated_args() -> Vec<String> {
    vec!["-I".into()]
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Spec {
    pub executable: PathBuf,
    pub executable_sha256: String,
    #[serde(default = "isolated_args")]
    pub args: Vec<String>,
    pub model_root: PathBuf,
    pub model_files: Vec<ModelFile>,
    pub library_version: String,
    pub language: String,
    #[serde(default)]
    pub limits: Limits,
}

/// This type cannot be deserialized from repository/provider data.
pub struct Host {
    pub(super) spec: Spec,
    pub(super) executable: PathBuf,
    pub(super) binding: String,
}

fn literal_absolute(path: &Path) -> Result<()> {
    ensure!(
        path.is_absolute() && path.as_os_str().len() <= 4096,
        "ASR requires literal absolute host paths"
    );
    for part in path.components() {
        match part {
            Component::Normal(name) => {
                normalized_syntax(name.to_str().context("ASR host path must be UTF-8")?)?;
            }
            Component::ParentDir | Component::CurDir => {
                anyhow::bail!("ASR host path traversal refused")
            }
            #[cfg(windows)]
            Component::Prefix(prefix) => ensure!(
                matches!(prefix.kind(), std::path::Prefix::Disk(_)),
                "ASR host path alias refused"
            ),
            _ => {}
        }
    }
    Ok(())
}

fn unlinked_ancestors(path: &Path, d: &Deadline) -> Result<()> {
    for ancestor in path.ancestors() {
        d.check()?;
        ensure!(
            !is_link(&fs::symlink_metadata(ancestor)?),
            "ASR host resource links refused"
        );
    }
    Ok(())
}

fn file_hash(path: &Path, maximum: u64, d: &Deadline) -> Result<(u64, String)> {
    d.check()?;
    let before = fs::symlink_metadata(path)?;
    ensure!(
        before.is_file() && !is_link(&before) && before.len() > 0 && before.len() <= maximum,
        "ASR host file unavailable or over capacity"
    );
    let mut file = fs::File::open(path)?;
    let opened = file.metadata()?;
    ensure!(
        opened.is_file()
            && opened.len() == before.len()
            && opened.modified()? == before.modified()?,
        "ASR host file changed before binding"
    );
    let mut digest = Sha256::new();
    let mut length = 0u64;
    let mut buffer = [0u8; 64_000];
    loop {
        d.check()?;
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        length += count as u64;
        ensure!(length <= before.len(), "ASR host file grew during binding");
        digest.update(&buffer[..count]);
    }
    let after = fs::symlink_metadata(path)?;
    ensure!(
        length == before.len()
            && after.is_file()
            && !is_link(&after)
            && after.len() == before.len()
            && after.modified()? == before.modified()?,
        "ASR host file changed during binding"
    );
    Ok((length, hex::encode(digest.finalize())))
}

impl Spec {
    pub fn from_host_input(bytes: &[u8]) -> Result<Self> {
        let spec: Self = (|| crate::json::decode(crate::json::parse(bytes, 64_000)?))()
            .map_err(|_: anyhow::Error| anyhow::anyhow!("ASR host configuration invalid"))?;
        spec.validate()?;
        Ok(spec)
    }
    fn validate(&self) -> Result<()> {
        literal_absolute(&self.executable)?;
        literal_absolute(&self.model_root)?;
        ensure!(
            valid_id(&self.executable_sha256) && self.library_version == "1.2.1",
            "ASR host identity or library version unsupported"
        );
        ensure!(
            self.language.len() >= 2
                && self.language.len() <= 3
                && self.language.bytes().all(|c| c.is_ascii_lowercase()),
            "ASR language selection invalid"
        );
        ensure!(
            self.args.len() <= 16
                && self.args.iter().all(|arg| arg.len() <= 4096
                    && !arg.chars().any(char::is_control)
                    && !obvious_secret(arg))
                && self.args.iter().map(String::len).sum::<usize>() <= 16_000,
            "ASR host argument capacity or content invalid"
        );
        validate_recorded_profile(
            &self.library_version,
            &self.language,
            &self.limits,
            &self.model_files,
        )
    }
}

pub(super) fn validate_recorded_profile(
    library: &str,
    language: &str,
    limits: &Limits,
    model_files: &[ModelFile],
) -> Result<()> {
    ensure!(
        library == "1.2.1"
            && (2..=3).contains(&language.len())
            && language.bytes().all(|c| c.is_ascii_lowercase()),
        "ASR recorded profile invalid"
    );
    ensure!(
        limits.max_audio_milliseconds <= 600_000
            && limits.max_segments <= 512
            && limits.max_transcript_bytes <= 1_000_000
            && limits.max_jobs <= 1000
            && (1..=4).contains(&limits.threads),
        "ASR host capacity invalid"
    );
    ensure!(
        limits.max_jobs > 0 && limits.max_audio_milliseconds > 0,
        "ASR invocation budget is zero"
    );
    ensure!(
        (4..=6).contains(&model_files.len()),
        "ASR model manifest incomplete or over capacity"
    );
    let mut names = BTreeSet::new();
    let mut total = 0u64;
    for file in model_files {
        ensure!(
            MODEL_NAMES.contains(&file.name.as_str())
                && names.insert(file.name.as_str())
                && valid_id(&file.sha256)
                && file.bytes > 0,
            "ASR model manifest member invalid"
        );
        let maximum = if file.name == "model.bin" {
            4_000_000_000
        } else {
            20_000_000
        };
        ensure!(file.bytes <= maximum, "ASR model member over capacity");
        total = total
            .checked_add(file.bytes)
            .context("ASR model capacity overflow")?;
    }
    ensure!(
        total <= 4_000_000_000
            && ["config.json", "model.bin", "tokenizer.json"]
                .iter()
                .all(|name| names.contains(name))
            && ["vocabulary.txt", "vocabulary.json"]
                .iter()
                .any(|name| names.contains(name)),
        "ASR local model bundle incomplete or over capacity"
    );
    Ok(())
}

impl Host {
    /// The caller supplies an independent host execution grant, never a JSON flag.
    pub fn select(spec: Spec, execution_authorized: bool, d: &Deadline) -> Result<Self> {
        ensure!(
            execution_authorized,
            "ASR requires explicit host worker trust"
        );
        spec.validate()?;
        unlinked_ancestors(&spec.executable, d)?;
        unlinked_ancestors(&spec.model_root, d)?;
        let executable = spec.executable.canonicalize()?;
        let model_root = spec.model_root.canonicalize()?;
        ensure!(model_root.is_dir(), "ASR model root unavailable");
        let (_, executable_hash) = file_hash(&executable, 128_000_000, d)?;
        ensure!(
            executable_hash == spec.executable_sha256,
            "ASR executable binding is stale"
        );
        let names: BTreeSet<_> = spec
            .model_files
            .iter()
            .map(|file| file.name.as_str())
            .collect();
        for name in MODEL_NAMES {
            d.check()?;
            match fs::symlink_metadata(model_root.join(name)) {
                Ok(metadata) => ensure!(
                    metadata.is_file() && !is_link(&metadata) && names.contains(name),
                    "ASR model input is unlisted or linked"
                ),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
        for file in &spec.model_files {
            let path = model_root.join(&file.name);
            let (bytes, hash) = file_hash(&path, file.bytes, d)?;
            ensure!(
                bytes == file.bytes && hash == file.sha256,
                "ASR model binding is stale"
            );
        }
        let binding = identity(&("native_asr_host_v1", &spec, &executable, &model_root));
        Ok(Self {
            spec,
            executable,
            binding,
        })
    }
    pub(super) fn unchanged(&self, d: &Deadline) -> Result<()> {
        let current = Self::select(self.spec.clone(), true, d)?;
        ensure!(current.binding == self.binding, "ASR host binding changed");
        Ok(())
    }
}

pub fn read_host_spec(path: &Path, d: &Deadline) -> Result<Spec> {
    literal_absolute(path)?;
    unlinked_ancestors(path, d)?;
    let before = file_hash(path, 64_000, d)?;
    let mut file = fs::File::open(path)?;
    let mut bytes = Vec::new();
    file.by_ref().take(64_001).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= 64_000
            && crate::model::hash(&bytes) == before.1
            && file_hash(path, 64_000, d)? == before,
        "ASR host configuration changed"
    );
    Spec::from_host_input(&bytes)
}
