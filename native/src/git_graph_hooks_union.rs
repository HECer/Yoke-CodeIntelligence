use super::{
    CONFIG_BYTES, Receipt, directory_guards, exists, external_binary, git_command, read_input,
    shell_quote, write_new,
};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};
use yoke::{capture::Deadline, model::hash, paths::normalized_syntax};

const DRIVER: &str = "yoke-graphify";
const KEY: &str = "merge.yoke-graphify.driver";
const ATTRIBUTE_BYTES: usize = 1_000_000;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Registration {
    version: u8,
    installation_id: String,
    path: String,
    attributes_existed: bool,
    original_attributes_hash: String,
}

impl Registration {
    fn block(&self) -> Result<Vec<u8>> {
        let mut pattern = String::from("/");
        for character in self.path.chars() {
            if matches!(character, '*' | '?' | '[' | ']' | '\\') {
                pattern.push('\\');
            }
            pattern.push(character);
        }
        let pattern = serde_json::to_string(&pattern)?;
        let marker = self.marker();
        Ok(format!("# {marker} begin\n{pattern} merge={DRIVER}\n# {marker} end\n").into_bytes())
    }

    fn marker(&self) -> String {
        format!("Yoke graph union {}", self.installation_id)
    }

    fn position(&self, attributes: &[u8], block: &[u8]) -> Result<Option<usize>> {
        let marker = self.marker();
        let marker_count = attributes
            .windows(marker.len())
            .filter(|window| *window == marker.as_bytes())
            .count();
        if marker_count == 0 {
            return Ok(None);
        }
        ensure!(marker_count == 2, "Managed Git attribute markings changed");
        let offset = attributes
            .windows(block.len())
            .position(|window| window == block)
            .context("Managed Git attribute block changed")?;
        ensure!(
            offset == 0 || attributes[offset - 1] == b'\n',
            "Managed Git attribute boundary changed"
        );
        Ok(Some(offset))
    }
}

fn load(state: &Path, receipt: &Receipt, deadline: &Deadline) -> Result<Option<Registration>> {
    let path = state.join("union.json");
    if !exists(&path)? {
        return Ok(None);
    }
    let registry: Registration = yoke::json::decode(yoke::json::parse(
        &read_input(&path, CONFIG_BYTES, deadline)?,
        CONFIG_BYTES,
    )?)?;
    ensure!(
        registry.version == 1 && registry.installation_id == receipt.installation_id,
        "Git graph union registration belongs to a different installation"
    );
    validate_path(&registry.path)?;
    Ok(Some(registry))
}

fn validate_path(path: &str) -> Result<()> {
    ensure!(
        path.len() <= 4096
            && normalized_syntax(path)? == path
            && !path
                .split('/')
                .any(|part| part.eq_ignore_ascii_case(".git")),
        "Graph union requires an exact repository-relative path"
    );
    Ok(())
}

fn attribute_path(receipt: &Receipt, deadline: &Deadline) -> Result<PathBuf> {
    let info = receipt.root.join(".git/info");
    if exists(&info)? {
        directory_guards(&info, deadline)?;
    }
    Ok(info.join("attributes"))
}

fn attributes(path: &Path, deadline: &Deadline) -> Result<Option<Vec<u8>>> {
    exists(path)?
        .then(|| read_input(path, ATTRIBUTE_BYTES, deadline))
        .transpose()
}

fn replace_attributes(
    path: &Path,
    original: Option<&[u8]>,
    output: Option<&[u8]>,
    deadline: &Deadline,
) -> Result<()> {
    let parent = path
        .parent()
        .context("Git attribute file requires a parent")?;
    let _held = directory_guards(parent, deadline)?;
    if original == output {
        return Ok(());
    }
    let pending = if let Some(bytes) = output {
        ensure!(
            bytes.len() <= ATTRIBUTE_BYTES,
            "Git attribute byte limit exceeded"
        );
        let mut pending = tempfile::NamedTempFile::new_in(parent)?;
        pending.write_all(bytes)?;
        pending.as_file().sync_all()?;
        Some(pending)
    } else {
        None
    };
    ensure!(
        attributes(path, deadline)?.as_deref() == original,
        "Git attribute file changed during registration"
    );
    deadline.check()?;
    match (pending, original) {
        (Some(pending), Some(_)) => {
            pending.persist(path).map_err(|error| error.error)?;
        }
        (Some(pending), None) => {
            pending
                .persist_noclobber(path)
                .map_err(|error| error.error)?;
        }
        (None, Some(_)) => fs::remove_file(path)?,
        (None, None) => {}
    }
    Ok(())
}

fn command(receipt: &Receipt) -> Result<String> {
    let yoke = shell_quote(&receipt.yoke)?.replace('%', "%%");
    let root = shell_quote(&receipt.root)?.replace('%', "%%");
    Ok(format!(
        "{yoke} --root {root} --timeout 15000 git graph-hooks merge-union {} %O %A %B -- %P",
        receipt.installation_id
    ))
}

fn config_is_owned(receipt: &Receipt, expected: &str, deadline: &Deadline) -> Result<bool> {
    let result = git_command(
        &receipt.git,
        &receipt.root,
        &["config", "--null", "--get-all", KEY],
        deadline,
    )?;
    if result.exit_code == Some(1) && result.stdout.is_empty() && result.stderr.is_empty() {
        return Ok(false);
    }
    ensure!(
        result.exit_code == Some(0),
        "Git union driver config could not be read"
    );
    let mut expected = expected.as_bytes().to_vec();
    expected.push(0);
    ensure!(
        result.stdout == expected,
        "Git union driver setting is foreign or duplicated"
    );
    Ok(true)
}

fn config_write(receipt: &Receipt, args: &[&str], deadline: &Deadline) -> Result<()> {
    let result = git_command(&receipt.git, &receipt.root, args, deadline)?;
    ensure!(
        result.exit_code == Some(0),
        "Git refused the local union driver configuration change"
    );
    Ok(())
}

fn check_attribute(
    receipt: &Receipt,
    path: &str,
    assignment: &str,
    deadline: &Deadline,
) -> Result<()> {
    let result = git_command(
        &receipt.git,
        &receipt.root,
        &["check-attr", "-z", "merge", "--", path],
        deadline,
    )?;
    ensure!(
        result.exit_code == Some(0)
            && result.stdout == format!("{path}\0merge\0{assignment}\0").as_bytes(),
        "The selected graph has a different effective Git merge attribute"
    );
    Ok(())
}

pub(super) fn install(
    state: &Path,
    receipt: &Receipt,
    selection: Option<&str>,
    deadline: &Deadline,
) -> Result<()> {
    let existing = load(state, receipt, deadline)?;
    let Some(path) = selection
        .map(str::to_owned)
        .or_else(|| existing.as_ref().map(|current| current.path.clone()))
    else {
        return Ok(());
    };
    let path = path.as_str();
    external_binary(&receipt.git, &receipt.root, deadline)?;
    validate_path(path)?;
    let attributes_path = attribute_path(receipt, deadline)?;
    let current = attributes(&attributes_path, deadline)?;
    let command = command(receipt)?;
    let has_configuration = config_is_owned(receipt, &command, deadline)?;
    let registration = match existing {
        Some(current) => {
            ensure!(
                current.path == path,
                "Git union was registered for a different graph path"
            );
            current
        }
        None => {
            ensure!(
                !has_configuration,
                "Git union config already exists without its registration"
            );
            let graph = receipt.root.join(path);
            directory_guards(
                graph.parent().context("Git graph requires a parent")?,
                deadline,
            )?;
            let meta =
                fs::symlink_metadata(&graph).context("The selected Git graph does not exist")?;
            ensure!(
                meta.is_file() && !yoke::paths::is_link(&meta),
                "The selected Git graph must be an ordinary file"
            );
            let tracked = git_command(
                &receipt.git,
                &receipt.root,
                &["ls-files", "-z", "--error-unmatch", "--", path],
                deadline,
            )?;
            ensure!(
                tracked.exit_code == Some(0) && tracked.stdout == format!("{path}\0").as_bytes(),
                "Graphify union registration requires one tracked file"
            );
            check_attribute(receipt, path, "unspecified", deadline)?;
            let registration = Registration {
                version: 1,
                installation_id: receipt.installation_id.clone(),
                path: path.to_owned(),
                attributes_existed: current.is_some(),
                original_attributes_hash: hash(current.as_deref().unwrap_or_default()),
            };
            let bytes = yoke::json::encode_bounded(&registration, CONFIG_BYTES, deadline)?;
            write_new(&state.join("union.json"), &bytes, deadline)?;
            registration
        }
    };
    let block = registration.block()?;
    let before = current.as_deref().unwrap_or_default();
    let missing = registration.position(before, &block)?.is_none();
    if missing {
        check_attribute(receipt, &registration.path, "unspecified", deadline)?;
    }
    if !has_configuration {
        config_write(
            receipt,
            &["config", "--local", "--add", KEY, &command],
            deadline,
        )?;
    }
    ensure!(
        config_is_owned(receipt, &command, deadline)?,
        "Git union configuration publication was not observed"
    );
    if missing {
        let mut next = before.to_vec();
        if !next.is_empty() && !next.ends_with(b"\n") {
            next.push(b'\n');
        }
        next.extend_from_slice(&block);
        let parent = attributes_path
            .parent()
            .context("Git attribute file requires a parent")?;
        if !exists(parent)? {
            fs::create_dir(parent)?;
        }
        replace_attributes(&attributes_path, current.as_deref(), Some(&next), deadline)?;
    }
    check_attribute(receipt, &registration.path, DRIVER, deadline)
}

pub(super) fn status(
    state: &Path,
    receipt: &Receipt,
    deadline: &Deadline,
) -> Result<Option<Value>> {
    let Some(registration) = load(state, receipt, deadline)? else {
        return Ok(None);
    };
    external_binary(&receipt.git, &receipt.root, deadline)?;
    let current = attributes(&attribute_path(receipt, deadline)?, deadline)?.unwrap_or_default();
    let block = registration.block()?;
    let attribute_owned = registration.position(&current, &block)?.is_some();
    let configuration_owned = config_is_owned(receipt, &command(receipt)?, deadline)?;
    if attribute_owned && configuration_owned {
        check_attribute(receipt, &registration.path, DRIVER, deadline)?;
    }
    Ok(Some(
        json!({"path":registration.path, "attributeOwned":attribute_owned,
        "driverOwned":configuration_owned}),
    ))
}

pub(super) fn uninstall(state: &Path, receipt: &Receipt, deadline: &Deadline) -> Result<()> {
    let Some(registration) = load(state, receipt, deadline)? else {
        return Ok(());
    };
    external_binary(&receipt.git, &receipt.root, deadline)?;
    let command = command(receipt)?;
    let configured = config_is_owned(receipt, &command, deadline)?;
    let path = attribute_path(receipt, deadline)?;
    let current = attributes(&path, deadline)?;
    let before = current.as_deref().unwrap_or_default();
    let block = registration.block()?;
    if let Some(offset) = registration.position(before, &block)? {
        let (mut prefix, suffix) = (&before[..offset], &before[offset + block.len()..]);
        if suffix.is_empty()
            && prefix.ends_with(b"\n")
            && hash(&prefix[..prefix.len() - 1]) == registration.original_attributes_hash
        {
            prefix = &prefix[..prefix.len() - 1];
        }
        let next = [prefix, suffix].concat();
        let next = (registration.attributes_existed || !next.is_empty()).then_some(next.as_slice());
        replace_attributes(&path, current.as_deref(), next, deadline)?;
    }
    if configured {
        config_write(
            receipt,
            &[
                "config",
                "--local",
                "--fixed-value",
                "--unset-all",
                KEY,
                &command,
            ],
            deadline,
        )?;
        ensure!(
            !config_is_owned(receipt, &command, deadline)?,
            "Git union configuration still exists"
        );
    }
    let registration = state.join("union.json");
    fs::remove_file(registration)?;
    Ok(())
}

pub(super) fn admit_merge(
    state: &Path,
    receipt: &Receipt,
    path: &str,
    deadline: &Deadline,
) -> Result<()> {
    let registration =
        load(state, receipt, deadline)?.context("Graph union has no selected file")?;
    if path != registration.path {
        bail!("Graph union was invoked for an unselected path");
    }
    let current = attributes(&attribute_path(receipt, deadline)?, deadline)?.unwrap_or_default();
    ensure!(
        registration
            .position(&current, &registration.block()?)?
            .is_some(),
        "Graph union attribute registration is inactive"
    );
    Ok(())
}
