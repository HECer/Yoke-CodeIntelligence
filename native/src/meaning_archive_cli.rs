//! Explicit host transport for complete checked Meaning archives.
use anyhow::{Context, Result, ensure};
use clap::{Args, Subcommand};
use serde_json::{Value, json};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};
use yoke::{
    Engine,
    capture::Deadline,
    graph_collections::{Registry, validate_registration_name},
    graph_view::View,
    graph_view_archive::{self as archive, Limits},
    model::valid_id,
};

#[derive(Args)]
pub struct Arguments {
    #[arg(long, global = true, default_value_t = 32_000_000)]
    max_wire_bytes: usize,
    #[arg(long, global = true, default_value_t = 64_000_000)]
    max_source_bytes: usize,
    #[command(subcommand)]
    operation: Operation,
}

#[derive(Subcommand)]
enum Operation {
    /// Export an already-published typed View to a fresh host file.
    Export {
        view_id: String,
        #[arg(long)]
        output: PathBuf,
    },
    /// Replay supplied bytes without opening a source Store or registry.
    Inspect {
        #[arg(long)]
        input: PathBuf,
    },
    /// Copy a fully checked archive into a host registry with exact CAS.
    Register {
        name: String,
        #[arg(long)]
        registry: PathBuf,
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        allow_registration: bool,
        #[arg(long, conflicts_with = "expect_absent")]
        expected: Option<String>,
        #[arg(long, conflicts_with = "expected")]
        expect_absent: bool,
    },
    /// Replay an existing typed registration; optionally write a fresh archive.
    Load {
        name: String,
        #[arg(long)]
        registry: PathBuf,
        /// Permit SQLite coordination sidecars, without registration permission.
        #[arg(long)]
        allow_registry_coordination: bool,
        #[arg(long)]
        output: Option<PathBuf>,
    },
}

impl Arguments {
    pub fn uses_source_store(&self) -> bool {
        matches!(self.operation, Operation::Export { .. })
    }

    pub fn preflight_permission(&self) -> Result<()> {
        match &self.operation {
            Operation::Register {
                name,
                allow_registration,
                expected,
                expect_absent,
                ..
            } => {
                ensure!(
                    *allow_registration,
                    "Meaning archive registration requires --allow-registration"
                );
                validate_registration_name(name)?;
                ensure!(
                    expected.is_some() != *expect_absent,
                    "Registration requires exactly --expected or --expect-absent"
                );
                ensure!(
                    expected.as_deref().is_none_or(valid_id),
                    "Invalid expected registration identity"
                );
            }
            Operation::Load { name, .. } => validate_registration_name(name)?,
            Operation::Export { view_id, .. } => {
                ensure!(valid_id(view_id), "Invalid published View identity");
            }
            Operation::Inspect { .. } => {}
        }
        Ok(())
    }

    fn label(&self) -> &'static str {
        match self.operation {
            Operation::Export { .. } => "export",
            Operation::Inspect { .. } => "inspect",
            Operation::Register { .. } => "register",
            Operation::Load { .. } => "load",
        }
    }

    fn destination(&self) -> Option<&Path> {
        match &self.operation {
            Operation::Export { output, .. } => Some(output),
            Operation::Load { output, .. } => output.as_deref(),
            _ => None,
        }
    }

    fn name(&self) -> Option<&str> {
        match &self.operation {
            Operation::Register { name, .. } | Operation::Load { name, .. } => Some(name),
            _ => None,
        }
    }
}

pub(super) fn host_path(path: &Path) -> Result<()> {
    let text = path.to_str().context("Host archive path must be Unicode")?;
    ensure!(
        path.is_absolute() && text.len() <= 4096 && !text.chars().any(char::is_control),
        "Archive path must be an absolute bounded host-selected path"
    );
    for component in path.components() {
        match component {
            Component::ParentDir | Component::CurDir => {
                anyhow::bail!("Archive host path must be canonical");
            }
            Component::Normal(name) => {
                yoke::paths::normalized_syntax(
                    name.to_str().context("Non-Unicode host component")?,
                )?;
            }
            #[cfg(windows)]
            Component::Prefix(prefix) => ensure!(
                matches!(
                    prefix.kind(),
                    std::path::Prefix::Disk(_) | std::path::Prefix::VerbatimDisk(_)
                ),
                "Network and device archive paths refused"
            ),
            _ => {}
        }
    }
    Ok(())
}

pub(super) fn directory_guards(path: &Path, deadline: &Deadline) -> Result<Vec<File>> {
    host_path(path)?;
    let mut held = Vec::new();
    for ancestor in path.ancestors() {
        deadline.check()?;
        let metadata = fs::symlink_metadata(ancestor)?;
        ensure!(
            metadata.is_dir() && !yoke::paths::is_link(&metadata),
            "Archive ancestor must be an existing nonlinked directory"
        );
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            // Hold each directory against replacement; open the reparse object
            // itself so a racing link cannot silently select another parent.
            let file = OpenOptions::new()
                .read(true)
                .share_mode(3)
                .custom_flags(0x0200_0000 | 0x0020_0000)
                .open(ancestor)?;
            let opened = file.metadata()?;
            ensure!(
                opened.is_dir() && !yoke::paths::is_link(&opened),
                "Archive ancestor changed"
            );
            held.push(file);
        }
        #[cfg(not(windows))]
        held.push(File::open(ancestor)?);
    }
    Ok(held)
}

pub(super) fn fresh_destination(path: &Path, deadline: &Deadline) -> Result<Vec<File>> {
    host_path(path)?;
    let parent = path
        .parent()
        .context("Archive destination needs a parent")?;
    let held = directory_guards(parent, deadline)?;
    ensure!(
        fs::symlink_metadata(path).is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound),
        "Archive output must be a fresh file; existing files, directories and links are preserved"
    );
    Ok(held)
}

fn source_database_uri(path: &Path, parameters: &str) -> Result<String> {
    let text = path
        .to_str()
        .context("Source store path must be Unicode")?
        .replace('\\', "/");
    // Windows canonical paths carry a native verbatim prefix, not URI syntax.
    let text = if let Some(unc) = text.strip_prefix("//?/UNC/") {
        format!("//{unc}")
    } else {
        text.strip_prefix("//?/").unwrap_or(&text).to_owned()
    };
    let mut uri = String::from(if text.starts_with("//") {
        "file://"
    } else {
        "file:"
    });
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"/:._-".contains(&byte) {
            uri.push(char::from(byte));
        } else {
            use std::fmt::Write;
            write!(uri, "%{byte:02X}")?;
        }
    }
    uri.push('?');
    uri.push_str(parameters);
    Ok(uri)
}

// An ordinary read-only WAL connection can create its WAL/shared-memory files.
// For a checkpointed store, validate the main file without entering that path.
fn checkpointed_source_schema(path: &Path, deadline: &Deadline) -> Result<()> {
    deadline.check()?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // Immutable reads require stable bytes; temporarily exclude writers and
        // replacements. Release this handle before the normal Store opens.
        options.share_mode(1).custom_flags(0x0020_0000);
    }
    let mut guard = options.open(path)?;
    let metadata = guard.metadata()?;
    ensure!(
        metadata.is_file() && !yoke::paths::is_link(&metadata),
        "Existing checkpointed store changed"
    );
    let mut header = [0u8; 100];
    guard.read_exact(&mut header)?;
    ensure!(
        &header[..16] == b"SQLite format 3\0" && header[18..20] == [2, 2],
        "Meaning archive export refuses source stores requiring journal initialization"
    );
    deadline.check()?;
    let db = rusqlite::Connection::open_with_flags(
        source_database_uri(path, "immutable=1&mode=ro")?,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )?;
    db.busy_timeout(deadline.remaining().min(std::time::Duration::from_secs(10)))?;
    db.set_limit(rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH, 65_536)?;
    let progress_deadline = deadline.clone();
    db.progress_handler(1_000, Some(move || progress_deadline.check().is_err()));
    // Immutable mode reports a synthetic journal mode; the persisted header
    // above is authoritative for WAL admission. Reuse the complete validator.
    super::validate_existing_batch_schema(&db, deadline)?;
    deadline.check()?;
    Ok(())
}

pub(super) fn existing_source_state(root: &Path, deadline: &Deadline) -> Result<Vec<File>> {
    let state = root.join(".yoke");
    let mut held = directory_guards(&state, deadline)
        .context("Meaning archive export requires a complete existing source store")?;
    // Check all constructor-created artifacts before opening SQLite. A valid
    // basic database alone is not permission to initialize its missing state.
    for directory in ["blobs", "trees"] {
        held.extend(
            directory_guards(&state.join(directory), deadline)
                .context("Meaning archive export refuses incomplete source store directories")?,
        );
    }
    for name in ["maintenance.lock", "clients.lock", "index.sqlite"] {
        deadline.check()?;
        let path = state.join(name);
        let metadata = fs::symlink_metadata(&path)
            .context("Meaning archive export refuses missing existing store artifacts")?;
        ensure!(
            metadata.is_file() && !yoke::paths::is_link(&metadata),
            "Existing store artifact must be a nonlinked file"
        );
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(3).custom_flags(0x0020_0000);
        }
        let file = options.open(path)?;
        let opened = file.metadata()?;
        ensure!(
            opened.is_file() && !yoke::paths::is_link(&opened),
            "Existing store artifact changed"
        );
        held.push(file);
    }
    let mut wal_present = false;
    let mut shm_present = false;
    for suffix in ["-wal", "-shm", "-journal"] {
        deadline.check()?;
        let path = state.join(format!("index.sqlite{suffix}"));
        match fs::symlink_metadata(&path) {
            Ok(metadata) => {
                ensure!(
                    metadata.is_file() && !yoke::paths::is_link(&metadata),
                    "Existing store sidecar must not be linked"
                );
                let mut options = OpenOptions::new();
                options.read(true);
                #[cfg(windows)]
                {
                    use std::os::windows::fs::OpenOptionsExt;
                    options.share_mode(3).custom_flags(0x0020_0000);
                }
                let file = options.open(path)?;
                let opened = file.metadata()?;
                ensure!(
                    opened.is_file() && !yoke::paths::is_link(&opened),
                    "Existing store sidecar changed"
                );
                ensure!(
                    suffix != "-journal" || opened.len() == 0,
                    "Archive export refuses source stores requiring journal recovery"
                );
                wal_present |= suffix == "-wal";
                shm_present |= suffix == "-shm";
                held.push(file);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    ensure!(
        wal_present == shm_present,
        "Archive export requires both existing WAL sidecars or a checkpointed store"
    );
    if !wal_present {
        checkpointed_source_schema(&state.join("index.sqlite"), deadline)
            .context("Meaning archive export requires an existing initialized workspace schema")?;
        return Ok(held);
    }
    // READ_ONLY alone may initialize or rewrite shared memory before schema
    // refusal. The pinned VFS parameter keeps this preflight's SHM read-only;
    // both existing sidecar handles above prevent its OPEN_ALWAYS creating one.
    let db = rusqlite::Connection::open_with_flags(
        source_database_uri(&state.join("index.sqlite"), "mode=ro&readonly_shm=1")?,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )?;
    db.busy_timeout(deadline.remaining().min(std::time::Duration::from_secs(10)))?;
    db.set_limit(rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH, 65_536)?;
    let progress_deadline = deadline.clone();
    db.progress_handler(1_000, Some(move || progress_deadline.check().is_err()));
    let mode: String = db.pragma_query_value(None, "journal_mode", |row| row.get(0))?;
    ensure!(
        mode == "wal",
        "Meaning archive export refuses source stores requiring journal initialization"
    );
    deadline.check()?;
    // Keep schema admission on this same protected connection. A generic
    // read-only reopen could initialize malformed preexisting shared memory.
    super::validate_existing_batch_schema(&db, deadline)
        .context("Meaning archive export requires an existing initialized workspace schema")?;
    deadline.check()?;
    Ok(held)
}

pub(super) fn read_input(path: &Path, cap: usize, deadline: &Deadline) -> Result<Vec<u8>> {
    host_path(path)?;
    let _held = directory_guards(
        path.parent().context("Archive input needs a parent")?,
        deadline,
    )?;
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_file() && !yoke::paths::is_link(&metadata) && metadata.len() <= cap as u64,
        "Archive input size/type exceeds selected capacity"
    );
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(1).custom_flags(0x0020_0000);
    }
    let mut file = options.open(path)?;
    let opened = file.metadata()?;
    ensure!(
        opened.is_file() && !yoke::paths::is_link(&opened) && opened.len() == metadata.len(),
        "Archive input changed before opening"
    );
    let mut bytes = Vec::new();
    let mut chunk = [0; 65536];
    loop {
        deadline.check()?;
        let count = file.read(&mut chunk)?;
        if count == 0 {
            break;
        }
        ensure!(
            bytes.len().checked_add(count).is_some_and(|n| n <= cap),
            "Archive input grew beyond capacity"
        );
        bytes.extend_from_slice(&chunk[..count]);
    }
    ensure!(
        bytes.len() as u64 == opened.len() && file.metadata()?.len() == opened.len(),
        "Archive input size changed"
    );
    Ok(bytes)
}

pub(super) fn write_new(path: &Path, bytes: &[u8], deadline: &Deadline) -> Result<()> {
    let _held = fresh_destination(path, deadline)?;
    let mut temporary =
        tempfile::NamedTempFile::new_in(path.parent().context("Archive output parent absent")?)?;
    for chunk in bytes.chunks(65536) {
        deadline.check()?;
        temporary.write_all(chunk)?;
    }
    temporary.as_file().sync_all()?;
    deadline.check()?;
    temporary
        .persist_noclobber(path)
        .map_err(|error| error.error)?;
    // Publication has completed. Delivery failures must retain its identity.
    Ok(())
}

fn shape(
    args: &Arguments,
    archive_id: &str,
    view_id: &str,
    snapshot: &str,
    workspace: &str,
    bytes: usize,
    meaning: Value,
) -> Value {
    let mut value = json!({"kind":"typed_meaning_view","profile":"meaning_view_archive_v1",
        "operation":args.label(),"archiveId":archive_id,"viewId":view_id,
        "snapshotId":snapshot,"workspaceId":workspace,"wireBytes":bytes,
        "meaning":meaning,"executionAuthority":"none"});
    if let Some(name) = args.name() {
        value["name"] = json!(name);
    }
    if matches!(args.operation, Operation::Register { .. }) {
        value["registrationId"] = json!("f".repeat(64));
    }
    if let Some(output) = args.destination() {
        value["output"] = json!(output);
        value["outputCreated"] = json!(true);
        value["contentHash"] = json!(archive_id);
    }
    value
}

fn encode(value: &Value, max: usize, deadline: &Deadline) -> Result<Vec<u8>> {
    yoke::json::encode_bounded(value, max - 1, deadline)
        .context("Meaning archive metadata exceeds output budget")
}

fn preadmit_output(args: &Arguments, max: usize, deadline: &Deadline) -> Result<()> {
    let largest = usize::MAX;
    let meaning = json!({"selectionCount":largest,"packetCount":largest,
        "conceptCount":largest,"relationCount":largest,"hyperedgeCount":largest,
        "unknownCostPacketCount":largest,"unknownUsagePacketCount":largest,
        "providerAttestation":"unverified"});
    let id = "f".repeat(64);
    encode(
        &shape(args, &id, &id, &id, &id, largest, meaning),
        max,
        deadline,
    )?;
    Ok(())
}

fn metadata(
    args: &Arguments,
    view: &View,
    archive_id: &str,
    bytes: usize,
    deadline: &Deadline,
) -> Result<Value> {
    let layer = view
        .meaning
        .as_ref()
        .context("Typed Meaning layer absent")?;
    let sum = |length: fn(&yoke::meaning_view::MeaningPacket) -> usize| -> Result<usize> {
        layer.packets.iter().try_fold(0usize, |total, packet| {
            deadline.check()?;
            total
                .checked_add(length(packet))
                .context("Meaning summary count overflow")
        })
    };
    let meaning = json!({"selectionCount":layer.selections.len(),"packetCount":layer.packets.len(),
        "conceptCount":sum(|p|p.concepts.len())?,"relationCount":sum(|p|p.relations.len())?,
        "hyperedgeCount":sum(|p|p.hyperedges.len())?,"providerAttestation":"unverified",
        "unknownCostPacketCount":layer.packets.iter().filter(|p|p.provenance.provider_reported_cost_micros.is_none()).count(),
        "unknownUsagePacketCount":layer.packets.iter().filter(|p|p.provenance.provider_reported_usage.is_none()).count()});
    Ok(shape(
        args,
        archive_id,
        &view.id,
        &view.snapshot_id,
        &view.workspace_id,
        bytes,
        meaning,
    ))
}

fn deliver(bytes: &[u8], deadline: &Deadline) -> Result<()> {
    deadline.check()?;
    let mut stdout = std::io::stdout().lock();
    stdout.write_all(bytes)?;
    stdout.write_all(b"\n")?;
    stdout.flush()?;
    Ok(())
}

pub fn run(
    args: &Arguments,
    source_root: &Path,
    timeout_ms: u64,
    max_json_bytes: u64,
) -> Result<bool> {
    args.preflight_permission()?;
    ensure!(
        (10..=600_000).contains(&timeout_ms),
        "Invalid meaning archive timeout"
    );
    ensure!(
        (1024..=4_000_000).contains(&max_json_bytes),
        "Meaning archive output budget must be 1024..4000000 bytes"
    );
    ensure!(
        (1..=32_000_000).contains(&args.max_wire_bytes)
            && (1..=64_000_000).contains(&args.max_source_bytes),
        "Invalid meaning archive capacity"
    );
    ensure!(
        args.uses_source_store() || source_root == Path::new("."),
        "Meaning archive reads/registration do not select a source root or provider"
    );
    let max = usize::try_from(max_json_bytes)?;
    let deadline = Deadline::new(timeout_ms);
    let limits = Limits {
        max_wire_bytes: args.max_wire_bytes,
        max_source_bytes: args.max_source_bytes,
    };
    preadmit_output(args, max, &deadline)?;
    let _destination_guards = args
        .destination()
        .map(|path| fresh_destination(path, &deadline))
        .transpose()?;
    match &args.operation {
        Operation::Export { view_id, output } => {
            let source_root = std::path::absolute(source_root)?;
            let _source_guards = directory_guards(&source_root, &deadline)
                .context("Meaning archive export requires an existing source workspace")?;
            let _state_guards = existing_source_state(&source_root, &deadline)?;
            let engine = Engine::open_with_timeout(&source_root, deadline.remaining())?;
            engine.store.set_deadline(&deadline)?;
            let prepared = archive::prepare_published(&engine.store, view_id, limits, &deadline)?;
            let id = prepared.archive_id();
            let value = metadata(args, prepared.view(), id, prepared.bytes().len(), &deadline)?;
            let summary = encode(&value, max, &deadline)?;
            write_new(output, prepared.bytes(), &deadline)?;
            deliver(&summary, &deadline).with_context(|| {
                format!(
                    "Meaning archive {id} is durable at {}; output delivery failed",
                    output.display()
                )
            })?;
        }
        Operation::Inspect { input } => {
            let raw = read_input(input, limits.max_wire_bytes, &deadline)?;
            let checked = archive::parse(&raw, limits, &deadline)?;
            let summary = metadata(
                args,
                checked.view(),
                checked.archive_id(),
                checked.bytes().len(),
                &deadline,
            )?;
            deliver(&encode(&summary, max, &deadline)?, &deadline)?;
        }
        Operation::Register {
            name,
            registry,
            input,
            expected,
            ..
        } => {
            let _registry_guards = directory_guards(registry, &deadline)?;
            if expected.is_some() {
                let metadata = fs::symlink_metadata(registry.join("yoke-collection.sqlite"))
                    .context("Expected replacement requires an existing collection registry")?;
                ensure!(
                    metadata.is_file() && !yoke::paths::is_link(&metadata),
                    "Existing registry database must not be linked"
                );
            }
            let raw = read_input(input, limits.max_wire_bytes, &deadline)?;
            let checked = archive::parse(&raw, limits, &deadline)?;
            let mut value = metadata(
                args,
                checked.view(),
                checked.archive_id(),
                checked.bytes().len(),
                &deadline,
            )?;
            // Encoding the exact shape before opening writable state admits all
            // output fields; registration replaces only the fixed-width ID.
            encode(&value, max, &deadline)?;
            let registry = Registry::open(registry)?;
            let id = registry.register_meaning_archive_cas(
                name,
                &checked,
                expected.as_deref(),
                &deadline,
            )?;
            let mut deliver_committed = || -> Result<()> {
                value["registrationId"] = json!(id);
                deliver(&encode(&value, max, &deadline)?, &deadline)
            };
            deliver_committed().with_context(|| {
                format!(
                    "Meaning registration {id} is durable for archive {}; output delivery failed",
                    checked.archive_id()
                )
            })?;
        }
        Operation::Load {
            name,
            registry,
            output,
            allow_registry_coordination,
        } => {
            let _registry_guards = directory_guards(registry, &deadline)?;
            let registry = Registry::open_existing_for_host(
                registry,
                *allow_registry_coordination,
                &deadline,
            )?;
            let checked = registry.load_meaning_archive(name, limits, &deadline)?;
            let value = metadata(
                args,
                checked.view(),
                checked.archive_id(),
                checked.bytes().len(),
                &deadline,
            )?;
            let summary = encode(&value, max, &deadline)?;
            if let Some(output) = output {
                write_new(output, checked.bytes(), &deadline)?;
                deliver(&summary, &deadline).with_context(|| {
                    format!(
                        "Meaning archive {} is durable at {}; output delivery failed",
                        checked.archive_id(),
                        output.display()
                    )
                })?;
            } else {
                deliver(&summary, &deadline)?;
            }
        }
    }
    Ok(true)
}
