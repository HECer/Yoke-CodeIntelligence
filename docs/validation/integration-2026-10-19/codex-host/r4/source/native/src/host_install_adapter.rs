//! Explicit branded configuration selection; never inferred from source or a home directory.
use super::*;
pub const CODEX_PROFILE: &str = "codex-mcp-v1";
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum AdapterBinding {
    CodexMcpV1 {
        source_root: DirectoryBinding,
        source_path: String,
        executable_path: String,
        policy_path: String,
    },
}
pub(super) fn profile(selection: &Selection) -> &'static str {
    match &selection.bindings.adapter {
        None => PROFILE_ID,
        Some(AdapterBinding::CodexMcpV1 { .. }) => CODEX_PROFILE,
    }
}
pub(super) fn filename(selection: &Selection) -> &'static str {
    if selection.bindings.adapter.is_none() {
        "AGENTS.md"
    } else {
        "config.toml"
    }
}
impl Selection {
    /// Select existing explicit Codex configuration roots and a distinct source directory.
    /// No client process, model, source Store or configuration write is started here.
    pub fn codex_mcp(
        installation: &str,
        target_roots: &[PathBuf],
        executable: &Path,
        policy: &Path,
        state_root: &Path,
        source_root: &Path,
        d: &Deadline,
    ) -> Result<Self> {
        let mut selection = Self::new(
            installation,
            target_roots,
            executable,
            policy,
            state_root,
            d,
        )?;
        input_path(source_root)?;
        let source = io::guarded_directory(source_root, d)?;
        for root in selection
            .bindings
            .roots
            .iter()
            .chain(std::iter::once(&selection.bindings.state_root))
        {
            let path = Path::new(&root.path);
            ensure!(
                !source.path.starts_with(path) && !path.starts_with(&source.path),
                "Codex source and configuration/state roots must be distinct and nonnested"
            );
        }
        selection.bindings.adapter = Some(AdapterBinding::CodexMcpV1 {
            source_root: DirectoryBinding {
                path: path_text(&source.path)?,
                identity: source.identity,
            },
            source_path: path_text(source_root)?,
            executable_path: path_text(executable)?,
            policy_path: path_text(policy)?,
        });
        let _held = guard_bindings(&selection, d)?;
        Ok(selection)
    }
    pub fn profile_id(&self) -> &'static str {
        profile(self)
    }
    pub fn target_filename(&self) -> &'static str {
        filename(self)
    }
}
pub(super) fn guard(
    selection: &Selection,
    dirs: &mut Vec<io::GuardedDirectory>,
    files: &mut Vec<io::HeldFile>,
    d: &Deadline,
) -> Result<()> {
    let Some(AdapterBinding::CodexMcpV1 {
        source_root,
        source_path,
        executable_path,
        policy_path,
    }) = &selection.bindings.adapter
    else {
        return Ok(());
    };
    for path in [source_path, executable_path, policy_path] {
        input_path(Path::new(path))?;
    }
    let source = io::guarded_directory(Path::new(source_path), d)?;
    ensure!(
        source.identity == source_root.identity && path_text(&source.path)? == source_root.path,
        "Codex source directory identity changed"
    );
    for root in selection
        .bindings
        .roots
        .iter()
        .chain(std::iter::once(&selection.bindings.state_root))
    {
        let path = Path::new(&root.path);
        ensure!(
            !source.path.starts_with(path) && !path.starts_with(&source.path),
            "Codex source and configuration/state roots must be distinct and nonnested"
        );
    }
    dirs.push(source);
    for (path, expected, is_executable) in [
        (executable_path, &selection.bindings.executable, true),
        (policy_path, &selection.bindings.policy, false),
    ] {
        let file = if is_executable {
            io::read_executable_binding(Path::new(path), 512_000_000, d)?
        } else {
            io::read_file(Path::new(path), MAX_FILE, false, d)?
        }
        .context("Codex launch authority missing")?;
        ensure!(
            file.identity == expected.identity
                && file.hash == expected.hash
                && file.byte_length == expected.bytes,
            "Codex launch path no longer matches host authority"
        );
        files.push(file);
    }
    Ok(())
}
fn server_name(selection: &Selection) -> String {
    format!("yoke_{}", selection.installation)
}
fn toml_body(selection: &Selection) -> Result<Vec<u8>> {
    let Some(AdapterBinding::CodexMcpV1 {
        source_path,
        executable_path,
        policy_path,
        ..
    }) = &selection.bindings.adapter
    else {
        bail!("Codex adapter selection required")
    };
    // JSON string/list encoding is the common subset of TOML basic strings/arrays
    // for these validated paths and fixed ASCII arguments. Reparse the result below.
    let args = vec![
        "--root",
        source_path,
        "serve-session",
        "--id",
        &selection.installation,
        "--host-profile",
        policy_path,
        "--profile",
        "native",
    ];
    let tools = [
        "ci_find",
        "ci_read",
        "ci_trace",
        "ci_context",
        "ci_retrieve",
        "ci_graph",
    ];
    let body = format!(
        "command = {}\nargs = {}\ncwd = {}\nenabled = true\nenv_vars = []\nenabled_tools = {}\nstartup_timeout_sec = 30\ntool_timeout_sec = 120\n",
        serde_json::to_string(executable_path)?,
        serde_json::to_string(&args)?,
        serde_json::to_string(source_path)?,
        serde_json::to_string(&tools)?
    );
    let parsed: toml::Table =
        toml::from_str(&body).context("Codex configuration encoding failed")?;
    ensure!(
        parsed["command"].as_str() == Some(executable_path)
            && parsed["cwd"].as_str() == Some(source_path),
        "Codex configuration encoding changed launch paths"
    );
    Ok(body.into_bytes())
}
pub(super) fn patch(
    selection: &Selection,
    bytes: &[u8],
    install: bool,
    prior: Option<&OwnedFragment>,
    d: &Deadline,
) -> Result<Patch> {
    if selection.bindings.adapter.is_none() {
        let authored = body(selection)?;
        return host_config::agents_patch(bytes, install.then_some(authored.as_slice()), prior, d);
    }
    let authored = toml_body(selection)?;
    host_config::toml_patch(
        bytes,
        &["mcp_servers".into(), server_name(selection)],
        install.then_some(authored.as_slice()),
        prior,
        d,
    )
}
