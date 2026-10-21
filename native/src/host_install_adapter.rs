//! Explicit branded configuration selection; never inferred from source or a home directory.
use super::*;
pub const CODEX_PROFILE: &str = "codex-mcp-v1";
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JsonMcpClient {
    Cursor,
    ClaudeCodeExplicit,
    ClaudeDesktop,
}
pub struct JsonMcpSource<'a> {
    pub root: &'a Path,
    pub client: JsonMcpClient,
}
impl JsonMcpClient {
    fn profile(self) -> &'static str {
        match self {
            Self::Cursor => "cursor-mcp-v1",
            Self::ClaudeCodeExplicit => "claude-explicit-mcp-v1",
            Self::ClaudeDesktop => "claude-desktop-mcp-v1",
        }
    }
    fn filename(self) -> &'static str {
        match self {
            Self::Cursor => "mcp.json",
            Self::ClaudeCodeExplicit => ".mcp.json",
            Self::ClaudeDesktop => "claude_desktop_config.json",
        }
    }
}
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
    ClaudeSessionHooksV1 {
        source_root: DirectoryBinding,
        source_path: String,
        executable_path: String,
        policy_path: String,
    },
    JsonMcpV1 {
        client: JsonMcpClient,
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
        Some(AdapterBinding::ClaudeSessionHooksV1 { .. }) => claude_hooks::PROFILE,
        Some(AdapterBinding::JsonMcpV1 { client, .. }) => client.profile(),
    }
}
pub(super) fn filename(selection: &Selection) -> &'static str {
    match &selection.bindings.adapter {
        None => "AGENTS.md",
        Some(AdapterBinding::CodexMcpV1 { .. }) => "config.toml",
        Some(AdapterBinding::ClaudeSessionHooksV1 { .. }) => claude_hooks::FILENAME,
        Some(AdapterBinding::JsonMcpV1 { client, .. }) => client.filename(),
    }
}
impl Selection {
    /// Explicit JSON client configuration roots; no home/project discovery or writes.
    pub fn json_mcp(
        installation: &str,
        target_roots: &[PathBuf],
        executable: &Path,
        policy: &Path,
        state_root: &Path,
        source: JsonMcpSource<'_>,
        d: &Deadline,
    ) -> Result<Self> {
        let source_root = source.root;
        let client = source.client;
        for path in [executable, policy, source_root] {
            let path = path_text(path)?;
            ensure!(
                !path.contains("${") && !crate::paths::obvious_secret(&path),
                "JSON MCP launch paths cannot contain interpolation or obvious secret material"
            );
        }
        let mut selection = Self::codex_mcp(
            installation,
            target_roots,
            executable,
            policy,
            state_root,
            source_root,
            d,
        )?;
        let Some(AdapterBinding::CodexMcpV1 {
            source_root,
            source_path,
            executable_path,
            policy_path,
        }) = selection.bindings.adapter.take()
        else {
            bail!("JSON MCP launch binding preparation failed")
        };
        selection.bindings.adapter = Some(AdapterBinding::JsonMcpV1 {
            client,
            source_root,
            source_path,
            executable_path,
            policy_path,
        });
        let _held = guard_bindings(&selection, d)?;
        Ok(selection)
    }
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
    let Some(
        AdapterBinding::CodexMcpV1 {
            source_root,
            source_path,
            executable_path,
            policy_path,
        }
        | AdapterBinding::ClaudeSessionHooksV1 {
            source_root,
            source_path,
            executable_path,
            policy_path,
        }
        | AdapterBinding::JsonMcpV1 {
            source_root,
            source_path,
            executable_path,
            policy_path,
            ..
        },
    ) = &selection.bindings.adapter
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
    if matches!(
        selection.bindings.adapter,
        Some(AdapterBinding::ClaudeSessionHooksV1 { .. })
    ) {
        let authored = claude_hooks::body(selection)?;
        return host_config::claude_hook_settings_patch(
            bytes,
            install.then_some(authored.as_slice()),
            prior,
            d,
        );
    }
    if selection.bindings.adapter.is_none() {
        let authored = body(selection)?;
        return host_config::agents_patch(bytes, install.then_some(authored.as_slice()), prior, d);
    }
    if let Some(AdapterBinding::JsonMcpV1 {
        source_path,
        executable_path,
        policy_path,
        ..
    }) = &selection.bindings.adapter
    {
        let authored = serde_json::to_vec(
            &serde_json::json!({"command":executable_path,"args":["--root",source_path,
            "serve-session","--id",selection.installation,"--host-profile",policy_path,"--profile","native"],"env":{}}),
        )?;
        return host_config::json_mcp_patch(
            bytes,
            &server_name(selection),
            install.then_some(authored.as_slice()),
            prior,
            d,
        );
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
