use super::meaning_archive_cli::{directory_guards, read_input, write_new};
use anyhow::{Context, Result, ensure};
use clap::{Args, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};
use yoke::{
    Engine,
    capture::{self, Deadline},
    git_workflows::{HostGit, capture_commit_at},
    model::{Graph, Snapshot, hash, identity, valid_id},
    paths::is_link,
    process::{CaptureLimits, CapturedOutput, run_capture},
};

#[path = "git_graph_hooks_union.rs"]
mod union_registration;

pub(super) const INPUT: &str = "git_graph_hooks_input_invalid";
const CONFIG_BYTES: usize = 32_000;
const GRAPH_BYTES: usize = 50 * 1024 * 1024;
const LOCAL_GIT_ENV: &[&str] = &[
    "GIT_DIR",
    "GIT_COMMON_DIR",
    "GIT_WORK_TREE",
    "GIT_IMPLICIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_QUARANTINE_PATH",
    "GIT_NAMESPACE",
    "GIT_SHALLOW_FILE",
    "GIT_GRAFT_FILE",
    "GIT_REPLACE_REF_BASE",
    "GIT_PREFIX",
    "GIT_INTERNAL_SUPER_PREFIX",
    "GIT_CONFIG",
    "GIT_CONFIG_COUNT",
    "GIT_CONFIG_PARAMETERS",
];

#[derive(Args)]
pub(super) struct Arguments {
    #[command(subcommand)]
    command: Operation,
}

#[derive(Subcommand)]
enum Operation {
    /// Install or resume the three local Yoke hooks.
    Install {
        #[arg(long)]
        git: Option<PathBuf>,
        #[arg(long)]
        trust_git: bool,
        #[arg(long)]
        allow_hook_write: bool,
        /// Existing capture to use as the source scope. Otherwise capture once at install.
        #[arg(long)]
        origin: Option<String>,
        /// Required when the effective Git hook directory is outside .git.
        #[arg(long)]
        hooks_dir: Option<PathBuf>,
        /// Install the union driver for this exact tracked Graphify graph path.
        #[arg(long)]
        union_graph: Option<String>,
    },
    /// Read the installation receipt and verify the managed files.
    Status,
    /// Disable refresh and remove the exact registered hook scripts.
    Uninstall {
        #[arg(long)]
        allow_hook_write: bool,
    },
    #[command(hide = true)]
    Refresh { event: Hook },
    #[command(hide = true)]
    MergeUnion {
        installation: String,
        base: PathBuf,
        current: PathBuf,
        other: PathBuf,
        path: String,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum Hook {
    Commit,
    Checkout,
    Merge,
}

impl Hook {
    const ALL: [Self; 3] = [Self::Commit, Self::Checkout, Self::Merge];

    fn name(self) -> &'static str {
        match self {
            Self::Commit => "post-commit",
            Self::Checkout => "post-checkout",
            Self::Merge => "post-merge",
        }
    }

    fn argument(self) -> &'static str {
        match self {
            Self::Commit => "commit",
            Self::Checkout => "checkout",
            Self::Merge => "merge",
        }
    }
}

#[derive(PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Receipt {
    schema_version: u8,
    installation_id: String,
    root: PathBuf,
    hooks_dir: PathBuf,
    git: PathBuf,
    yoke: PathBuf,
    origin: String,
}

impl Arguments {
    pub(super) fn run(&self, root: &Path, timeout: u64, max_bytes: u64) -> Result<bool> {
        ensure!(
            (1..=600_000).contains(&timeout) && (1024..=4_000_000).contains(&max_bytes),
            INPUT
        );
        match &self.command {
            Operation::Install {
                allow_hook_write, ..
            }
            | Operation::Uninstall { allow_hook_write } => ensure!(
                *allow_hook_write,
                "Git hook changes require --allow-hook-write"
            ),
            _ => {}
        }
        let deadline = Deadline::new(timeout);
        let requested_root = std::path::absolute(root)?;
        let _root_guard = directory_guards(&requested_root, &deadline)?;
        let root = requested_root.canonicalize()?;
        let git_dir = root.join(".git");
        let _git_guard = directory_guards(&git_dir, &deadline)
            .context("Managed graph hooks need a conventional .git directory")?;
        let state = git_dir.join("yoke-graph-hooks");
        if !exists(&state)? {
            match &self.command {
                Operation::Status | Operation::Uninstall { .. } => {
                    return output(&json!({"state":"absent"}), max_bytes, &deadline);
                }
                Operation::Refresh { .. } => return Ok(true),
                Operation::MergeUnion { .. } => anyhow::bail!("Graph union installation is absent"),
                Operation::Install { .. } => {}
            }
        }
        let result = match &self.command {
            Operation::Install {
                git,
                trust_git,
                origin,
                hooks_dir,
                union_graph,
                ..
            } => {
                let host = crate::selected_git(git.clone(), *trust_git)?;
                let git = external_binary(&host.executable, &root, &deadline)?;
                let yoke = external_binary(&std::env::current_exe()?, &root, &deadline)?;
                let hooks =
                    checked_hooks_path(&root, &git_dir, &git, hooks_dir.as_deref(), &deadline)?;
                install(
                    &root,
                    &state,
                    &hooks,
                    git,
                    yoke,
                    origin.as_deref(),
                    union_graph.as_deref(),
                    &deadline,
                )?
            }
            Operation::Status => {
                let _state_guard = directory_guards(&state, &deadline)?;
                match load_receipt(&root, &state, &deadline)? {
                    Some(receipt) => status(&state, &receipt, &deadline)?,
                    None => json!({"state":"staging","ownedHooks":0}),
                }
            }
            Operation::Uninstall { .. } => {
                let _state_guard = directory_guards(&state, &deadline)?;
                let _lock = lock(&state, &deadline)?;
                let receipt = load_receipt(&root, &state, &deadline)?
                    .context("Git graph hook installation has no receipt")?;
                uninstall(&state, &receipt, &deadline)?
            }
            Operation::Refresh { event } => {
                let _state_guard = directory_guards(&state, &deadline)?;
                let _lock = lock(&state, &deadline)?;
                if let Some(receipt) = load_receipt(&root, &state, &deadline)?
                    && active(&state, &receipt, &deadline)?
                {
                    refresh(&state, &receipt, *event, &deadline)?;
                }
                return Ok(true);
            }
            Operation::MergeUnion {
                installation,
                base,
                current,
                other,
                path,
            } => {
                let _state_guard = directory_guards(&state, &deadline)?;
                let _lock = lock(&state, &deadline)?;
                let receipt = load_receipt(&root, &state, &deadline)?.context(INPUT)?;
                ensure!(
                    receipt.installation_id == *installation
                        && active(&state, &receipt, &deadline)?,
                    "Graph union installation is inactive or changed"
                );
                ensure!(std::env::current_dir()?.canonicalize()? == root, INPUT);
                union_registration::admit_merge(&state, &receipt, path, &deadline)?;
                return super::graph_union_cli::merge_scratch(base, current, other, &deadline);
            }
        };
        output(&result, max_bytes, &deadline)
    }
}

fn exists(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            ensure!(!is_link(&metadata), "Git graph hook link refused");
            Ok(true)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

fn external_binary(path: &Path, root: &Path, deadline: &Deadline) -> Result<PathBuf> {
    ensure!(path.is_absolute(), INPUT);
    directory_guards(path.parent().context(INPUT)?, deadline)?;
    let metadata = fs::symlink_metadata(path)?;
    ensure!(metadata.is_file() && !is_link(&metadata), INPUT);
    let path = path.canonicalize()?;
    ensure!(
        !path.starts_with(root),
        "Git and Yoke executables must be outside the selected repository"
    );
    Ok(path)
}

fn git_command(
    binary: &Path,
    root: &Path,
    args: &[&str],
    deadline: &Deadline,
) -> Result<CapturedOutput> {
    let mut command = Command::new(binary);
    for name in LOCAL_GIT_ENV {
        command.env_remove(name);
    }
    command
        .current_dir(root)
        .env("GIT_TERMINAL_PROMPT", "0")
        .args([
            "--no-replace-objects",
            "--no-lazy-fetch",
            "--no-pager",
            "--literal-pathspecs",
            "-c",
            "core.fsmonitor=false",
        ])
        .args(args);
    let obtained = run_capture(
        command,
        &[],
        deadline,
        CaptureLimits {
            stdin_bytes: 0,
            stdout_bytes: 8192,
            stderr_bytes: 8192,
        },
    )?;
    ensure!(
        !obtained.timed_out
            && !obtained.output_exceeded
            && obtained.output_complete
            && obtained.stdin_complete
            && !obtained.io_error,
        "Git graph hook metadata request failed"
    );
    Ok(obtained)
}

fn git_query(binary: &Path, root: &Path, args: &[&str], deadline: &Deadline) -> Result<String> {
    let obtained = git_command(binary, root, args, deadline)?;
    ensure!(
        obtained.exit_code == Some(0),
        "Git graph hook metadata request failed"
    );
    let value = String::from_utf8(obtained.stdout)?;
    let value = value.trim_end_matches(['\r', '\n']);
    ensure!(
        !value.is_empty() && !value.chars().any(char::is_control),
        INPUT
    );
    Ok(value.to_owned())
}

fn checked_hooks_path(
    root: &Path,
    git_dir: &Path,
    git: &Path,
    selected: Option<&Path>,
    deadline: &Deadline,
) -> Result<PathBuf> {
    let actual_git = git_query(git, root, &["rev-parse", "--absolute-git-dir"], deadline)?;
    ensure!(
        Path::new(&actual_git).canonicalize()? == git_dir.canonicalize()?,
        INPUT
    );
    let effective = git_query(
        git,
        root,
        &["rev-parse", "--path-format=absolute", "--git-path", "hooks"],
        deadline,
    )?;
    directory_guards(Path::new(&effective), deadline)?;
    let effective = Path::new(&effective).canonicalize()?;
    if let Some(selected) = selected {
        directory_guards(selected, deadline)?;
        ensure!(
            selected.canonicalize()? == effective,
            "Selected Git hook directory differs from Git's effective directory"
        );
    } else {
        ensure!(
            effective.starts_with(git_dir),
            "An external hook directory requires --hooks-dir"
        );
    }
    Ok(effective)
}

fn lock(state: &Path, deadline: &Deadline) -> Result<File> {
    let file = state.join("writer");
    exists(&file)?;
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x0020_0000).share_mode(3);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options.open(file)?;
    let metadata = file.metadata()?;
    ensure!(metadata.is_file() && !is_link(&metadata), INPUT);
    loop {
        deadline.check()?;
        match fs2::FileExt::try_lock_exclusive(&file) {
            Ok(()) => return Ok(file),
            Err(error)
                if error.kind() == io::ErrorKind::WouldBlock
                    || error.raw_os_error() == fs2::lock_contended_error().raw_os_error() =>
            {
                std::thread::sleep(deadline.remaining().min(Duration::from_millis(20)))
            }
            Err(error) => return Err(error).context("Cannot lock managed graph hooks"),
        }
    }
}

fn load_receipt(root: &Path, state: &Path, deadline: &Deadline) -> Result<Option<Receipt>> {
    let path = state.join("installation.json");
    if !exists(&path)? {
        return Ok(None);
    }
    let receipt: Receipt = yoke::json::decode(yoke::json::parse(
        &read_input(&path, CONFIG_BYTES, deadline)?,
        CONFIG_BYTES,
    )?)?;
    ensure!(
        receipt.schema_version == 1
            && receipt.root == root
            && valid_id(&receipt.origin)
            && valid_id(&receipt.installation_id),
        INPUT
    );
    Ok(Some(receipt))
}

fn active(state: &Path, receipt: &Receipt, deadline: &Deadline) -> Result<bool> {
    let token = state.join("active");
    if !exists(&token)? {
        return Ok(false);
    }
    ensure!(
        read_input(&token, 64, deadline)? == identity(receipt).as_bytes(),
        "Git graph hook activation marker differs"
    );
    Ok(true)
}

fn shell_quote(path: &Path) -> Result<String> {
    let path = path.to_str().context(INPUT)?;
    #[cfg(windows)]
    let path = path
        .strip_prefix(r"\\?\")
        .unwrap_or(path)
        .replace('\\', "/");
    ensure!(!path.chars().any(char::is_control), INPUT);
    Ok(format!("'{}'", path.replace('\'', "'\"'\"'")))
}

fn script(receipt: &Receipt, hook: Hook) -> Result<Vec<u8>> {
    let checkout = if matches!(hook, Hook::Checkout) {
        "[ \"$3\" = 1 ] || exit 0\n"
    } else {
        ""
    };
    Ok(format!("#!/bin/sh\n# Yoke installation {}\n{checkout}unset {}\nexec {} --root {} --timeout 15000 git graph-hooks refresh {}\n",
        receipt.installation_id, LOCAL_GIT_ENV.join(" "), shell_quote(&receipt.yoke)?, shell_quote(&receipt.root)?, hook.argument()).into_bytes())
}

struct RegisteredHook {
    path: PathBuf,
    expected: Vec<u8>,
    present: bool,
}

fn registered_hooks(receipt: &Receipt, deadline: &Deadline) -> Result<Vec<RegisteredHook>> {
    let _held = directory_guards(&receipt.hooks_dir, deadline)?;
    Hook::ALL
        .into_iter()
        .map(|hook| {
            let path = receipt.hooks_dir.join(hook.name());
            let expected = script(receipt, hook)?;
            let present = exists(&path)?;
            if present {
                ensure!(
                    read_input(&path, CONFIG_BYTES, deadline)? == expected,
                    "Git hook is not owned by this installation: {}",
                    hook.name()
                );
            }
            Ok(RegisteredHook {
                path,
                expected,
                present,
            })
        })
        .collect()
}

#[allow(clippy::too_many_arguments)] // Host-selected roots and executables precede creation of the receipt.
fn install(
    root: &Path,
    state: &Path,
    hooks: &Path,
    git: PathBuf,
    yoke: PathBuf,
    origin: Option<&str>,
    union_graph: Option<&str>,
    deadline: &Deadline,
) -> Result<Value> {
    ensure!(origin.is_none_or(valid_id), INPUT);
    let _held = directory_guards(hooks, deadline)?;
    if !exists(state)? {
        for hook in Hook::ALL {
            ensure!(
                !exists(&hooks.join(hook.name()))?,
                "Git hook already exists: {}",
                hook.name()
            );
        }
        match fs::create_dir(state) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
    }
    let _state_guard = directory_guards(state, deadline)?;
    let _lock = lock(state, deadline)?;
    let receipt = match load_receipt(root, state, deadline)? {
        Some(saved) => {
            ensure!(
                saved.git == git
                    && saved.yoke == yoke
                    && saved.hooks_dir == hooks
                    && origin.is_none_or(|id| id == saved.origin),
                "Git graph hook installation selection differs"
            );
            saved
        }
        None => {
            for hook in Hook::ALL {
                ensure!(
                    !exists(&hooks.join(hook.name()))?,
                    "Git hook already exists: {}",
                    hook.name()
                );
            }
            let mut engine = Engine::open_with_timeout(root, deadline.remaining())?;
            let origin = match origin {
                Some(id) => engine.store.snapshot(id)?,
                None => engine.index(deadline)?.0,
            };
            capture::source_scope_policy(&engine.store, &origin)?;
            let mut installation = [0; 32];
            getrandom::fill(&mut installation)
                .map_err(|_| anyhow::anyhow!("Git graph hook installation identity unavailable"))?;
            let next = Receipt {
                schema_version: 1,
                installation_id: hex::encode(installation),
                root: root.to_owned(),
                hooks_dir: hooks.to_owned(),
                git,
                yoke,
                origin: origin.id,
            };
            let bytes = yoke::json::encode_bounded(&next, CONFIG_BYTES, deadline)?;
            write_new(&state.join("installation.json"), &bytes, deadline)?;
            next
        }
    };
    let enrolled = active(state, &receipt, deadline)?;
    let registered = registered_hooks(&receipt, deadline)?;
    union_registration::install(state, &receipt, union_graph, deadline)?;
    for RegisteredHook {
        path,
        expected,
        present,
    } in registered
    {
        if !present {
            let mut file = tempfile::NamedTempFile::new_in(hooks)?;
            file.write_all(&expected)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                file.as_file()
                    .set_permissions(fs::Permissions::from_mode(0o700))?;
            }
            file.as_file().sync_all()?;
            deadline.check()?;
            file.persist_noclobber(path).map_err(|error| error.error)?;
        }
    }
    if !enrolled {
        write_new(
            &state.join("active"),
            identity(&receipt).as_bytes(),
            deadline,
        )?;
    }
    status(state, &receipt, deadline)
}

fn status(state: &Path, receipt: &Receipt, deadline: &Deadline) -> Result<Value> {
    let hooks = registered_hooks(receipt, deadline)?
        .into_iter()
        .filter(|item| item.present)
        .count();
    let active = active(state, receipt, deadline)?;
    let installation = match (active, hooks) {
        (true, 3) => {
            let git = external_binary(&receipt.git, &receipt.root, deadline)?;
            let effective = git_query(
                &git,
                &receipt.root,
                &["rev-parse", "--path-format=absolute", "--git-path", "hooks"],
                deadline,
            )?;
            if Path::new(&effective)
                .canonicalize()
                .is_ok_and(|path| path == receipt.hooks_dir)
            {
                "active"
            } else {
                "hook-directory-changed"
            }
        }
        (false, 0) => "inactive",
        _ => "incomplete",
    };
    let mut result = json!({"state":installation,"ownedHooks":hooks,"originSnapshotId":receipt.origin,
        "graph":state.join("graph.json"),"hasGraph":exists(&state.join("graph.json"))?});
    if let Some(union) = union_registration::status(state, receipt, deadline)? {
        result["unionDriver"] = union;
    }
    Ok(result)
}

fn uninstall(state: &Path, receipt: &Receipt, deadline: &Deadline) -> Result<Value> {
    let _held = directory_guards(&receipt.hooks_dir, deadline)?;
    let hooks = registered_hooks(receipt, deadline)?;
    union_registration::uninstall(state, receipt, deadline)?;
    if active(state, receipt, deadline)? {
        fs::remove_file(state.join("active"))?;
    }
    for RegisteredHook {
        path,
        expected,
        present,
    } in hooks
    {
        if present {
            ensure!(
                read_input(&path, CONFIG_BYTES, deadline)? == expected,
                "Registered hook changed before removal"
            );
            fs::remove_file(path)?;
        }
    }
    status(state, receipt, deadline)
}

fn head(receipt: &Receipt, deadline: &Deadline) -> Result<String> {
    let oid = git_query(
        &receipt.git,
        &receipt.root,
        &["rev-parse", "--verify", "HEAD^{commit}"],
        deadline,
    )?;
    ensure!(oid.len() == 40 || oid.len() == 64, INPUT);
    ensure!(
        oid.bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        INPUT
    );
    Ok(oid)
}

fn refresh(state: &Path, receipt: &Receipt, event: Hook, deadline: &Deadline) -> Result<()> {
    ensure!(
        LOCAL_GIT_ENV
            .iter()
            .all(|name| std::env::var_os(name).is_none()),
        INPUT
    );
    ensure!(
        registered_hooks(receipt, deadline)?
            .iter()
            .all(|item| item.present),
        "Managed Git hook set is incomplete"
    );
    let git = external_binary(&receipt.git, &receipt.root, deadline)?;
    let commit = head(receipt, deadline)?;
    let mut engine = Engine::open_with_timeout(&receipt.root, deadline.remaining())?;
    let origin = engine.store.snapshot(&receipt.origin)?;
    let (snapshot, graph) = capture_commit_at(
        engine.capture_epoch,
        &mut engine,
        &HostGit { executable: git },
        &origin,
        &commit,
        deadline,
    )?;
    let graph = graphify(
        &snapshot,
        &graph,
        &commit,
        &receipt.installation_id,
        event,
        deadline,
    )?;
    let bytes = yoke::json::encode_bounded(&graph, GRAPH_BYTES - 1, deadline)?;
    let path = state.join("graph.json");
    if exists(&path)? {
        let previous = yoke::json::parse(&read_input(&path, GRAPH_BYTES, deadline)?, GRAPH_BYTES)?;
        ensure!(
            previous["graph"]["refreshInstallationId"] == receipt.installation_id,
            "The existing graph belongs to a different installation"
        );
    }
    let mut pending = tempfile::NamedTempFile::new_in(state)?;
    for chunk in bytes.chunks(65536) {
        deadline.check()?;
        pending.write_all(chunk)?;
    }
    pending.write_all(b"\n")?;
    pending.as_file().sync_all()?;
    ensure!(
        head(receipt, deadline)? == commit,
        "Git HEAD changed while refreshing the graph"
    );
    deadline.check()?;
    pending.persist(&path).map_err(|error| error.error)?;
    Ok(())
}

fn graphify(
    snapshot: &Snapshot,
    graph: &Graph,
    commit: &str,
    installation_id: &str,
    event: Hook,
    deadline: &Deadline,
) -> Result<Value> {
    let mut nodes = Vec::new();
    for entity in &graph.entities {
        deadline.check()?;
        nodes.push(json!({
            "id":entity.id,"label":entity.name,"file_type":"code","kind":entity.kind,"source_file":entity.path,
            "source_location":format!("L{}", entity.start_line),"startByte":entity.start_byte,"endByte":entity.end_byte,
            "producer":entity.producer
        }));
    }
    let mut links = Vec::new();
    for (key, edge) in graph.observations.iter().enumerate() {
        deadline.check()?;
        let unresolved;
        let target = match edge.target.as_deref() {
            Some(id) => id,
            None => {
                unresolved = format!("ref_{}", hash(edge.target_label.as_bytes()));
                &unresolved
            }
        };
        links.push(json!({"key":key,"source":edge.source,"target":target,"_src":edge.source,"_tgt":target,
            "id":edge.id,"targetLabel":edge.target_label,"relation":edge.relation,"resolution":edge.resolution,
            "evidenceIds":edge.evidence_ids,"producer":edge.producer}));
    }
    Ok(
        json!({"directed":true,"multigraph":true,"nodes":nodes,"links":links,
        "graph":{"sourceCommit":commit,"snapshotId":snapshot.id,"nativeGraphSha256":identity(graph),
            "refreshInstallationId":installation_id,
            "event":event.name(),"evidence":graph.evidence,"coverage":graph.coverage}}),
    )
}

fn output(result: &Value, max_bytes: u64, deadline: &Deadline) -> Result<bool> {
    let bytes = yoke::json::encode_bounded(result, max_bytes as usize - 1, deadline)?;
    let mut stdout = io::stdout().lock();
    stdout.write_all(&bytes)?;
    stdout.write_all(b"\n")?;
    stdout.flush()?;
    Ok(true)
}
