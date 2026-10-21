use anyhow::{Context, Result, ensure};
use clap::{Args, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use yoke::{
    capture::Deadline,
    host_install::{self, InitializationReceipt, Plan, Receipt, Selection, WriteAuthority},
};

#[derive(Args)]
pub struct Arguments {
    #[command(subcommand)]
    operation: Operation,
}
#[derive(Subcommand)]
enum Operation {
    Plan {
        #[arg(long)]
        selection: PathBuf,
    },
    UninstallPlan {
        #[arg(long)]
        selection: PathBuf,
    },
    Status {
        #[arg(long)]
        selection: PathBuf,
    },
    Apply {
        #[arg(long)]
        selection: PathBuf,
        #[arg(long)]
        plan: PathBuf,
        #[arg(long)]
        allow_host_writes: bool,
    },
    RecoverInitialization {
        #[arg(long)]
        selection: PathBuf,
        #[arg(long)]
        plan: PathBuf,
        #[arg(long)]
        allow_host_writes: bool,
    },
    Recover {
        #[arg(long)]
        selection: PathBuf,
        #[arg(long)]
        journal: String,
        #[arg(long, value_enum)]
        mode: RecoveryMode,
        #[arg(long)]
        allow_host_writes: bool,
    },
}
#[derive(Clone, Copy, ValueEnum)]
enum RecoveryMode {
    Resume,
    Rollback,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SelectionFile {
    installation: String,
    target_roots: Vec<PathBuf>,
    executable: PathBuf,
    policy: PathBuf,
    state_root: PathBuf,
    #[serde(default)]
    adapter: Option<AdapterSelection>,
}
#[derive(Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
enum AdapterSelection {
    CodexMcpV1 {
        source_root: PathBuf,
    },
    JsonMcpV1 {
        source_root: PathBuf,
        client: host_install::JsonMcpClient,
    },
}
impl Arguments {
    pub fn preflight_permission(&self) -> Result<()> {
        if let Operation::Apply {
            allow_host_writes, ..
        }
        | Operation::RecoverInitialization {
            allow_host_writes, ..
        }
        | Operation::Recover {
            allow_host_writes, ..
        } = &self.operation
        {
            ensure!(
                *allow_host_writes,
                "Host installation writes require --allow-host-writes"
            );
        }
        if let Operation::Recover { journal, .. } = &self.operation {
            ensure!(
                yoke::model::valid_id(journal),
                "Invalid host recovery journal identity"
            );
        }
        Ok(())
    }
    fn selection_path(&self) -> &Path {
        match &self.operation {
            Operation::Plan { selection }
            | Operation::UninstallPlan { selection }
            | Operation::Status { selection }
            | Operation::Apply { selection, .. }
            | Operation::RecoverInitialization { selection, .. }
            | Operation::Recover { selection, .. } => selection,
        }
    }
}
fn now_ms() -> Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_millis()
        .try_into()?)
}
fn print<T: Serialize>(value: &T, max_bytes: usize, deadline: &Deadline) -> Result<()> {
    let bytes = yoke::json::encode_bounded(value, max_bytes - 1, deadline)?;
    deadline.check()?;
    let mut output = std::io::stdout().lock();
    output.write_all(&bytes)?;
    output.write_all(b"\n")?;
    output.flush()?;
    Ok(())
}
fn receipt_summary(receipt: &Receipt) -> Value {
    json!({"id":receipt.id,"planId":receipt.plan_id,
        "installationId":receipt.installation_id,"operation":receipt.operation,
        "status":receipt.status,"changedPaths":receipt.changed_paths,"journalId":receipt.journal_id})
}
fn print_receipt(receipt: &Receipt, max_bytes: usize, deadline: &Deadline) -> Result<()> {
    print(&receipt_summary(receipt), max_bytes, deadline).with_context(|| {
        format!(
            "Host installation receipt {} is durable (plan {}, journal {}); output delivery failed",
            receipt.id, receipt.plan_id, receipt.journal_id
        )
    })
}
fn print_initialization_receipt(
    receipt: &InitializationReceipt,
    max_bytes: usize,
    deadline: &Deadline,
) -> Result<()> {
    print(receipt, max_bytes, deadline).with_context(|| {
        format!(
            "Host initialization receipt {} is durable (plan {}, token {}); output delivery failed",
            receipt.id, receipt.plan_id, receipt.token
        )
    })
}
fn admit_receipt(paths: Vec<String>, max_bytes: usize, deadline: &Deadline) -> Result<()> {
    // Receipts and inverse fragments remain in the explicit private host state.
    // Admit the largest public summary before changing any configuration file.
    let id = "f".repeat(64);
    let largest = json!({"id":id,"planId":id,"installationId":id,
        "operation":"uninstall","status":"recovery_required","changedPaths":paths,"journalId":id});
    yoke::json::encode_bounded(&largest, max_bytes - 1, deadline)?;
    Ok(())
}
pub fn run(args: &Arguments, timeout: u64, max_bytes: u64) -> Result<bool> {
    args.preflight_permission()?;
    ensure!(
        (10..=600_000).contains(&timeout),
        "Invalid host installation timeout"
    );
    ensure!(
        (1024..=4_000_000).contains(&max_bytes),
        "Host output budget must be 1024..4000000 bytes"
    );
    let max_bytes = max_bytes as usize;
    let deadline = Deadline::new(timeout);
    let file: SelectionFile =
        yoke::json::decode(super::input_json(args.selection_path(), 128_000)?)?;
    deadline.check()?;
    let selection = match &file.adapter {
        Some(AdapterSelection::CodexMcpV1 { source_root }) => Selection::codex_mcp(
            &file.installation,
            &file.target_roots,
            &file.executable,
            &file.policy,
            &file.state_root,
            source_root,
            &deadline,
        )?,
        Some(AdapterSelection::JsonMcpV1 {
            source_root,
            client,
        }) => Selection::json_mcp(
            &file.installation,
            &file.target_roots,
            &file.executable,
            &file.policy,
            &file.state_root,
            host_install::JsonMcpSource {
                root: source_root,
                client: *client,
            },
            &deadline,
        )?,
        None => Selection::new(
            &file.installation,
            &file.target_roots,
            &file.executable,
            &file.policy,
            &file.state_root,
            &deadline,
        )?,
    };
    match &args.operation {
        Operation::Plan { .. } => print(
            &host_install::plan_install(&selection, now_ms()?, &deadline)?,
            max_bytes,
            &deadline,
        )?,
        Operation::UninstallPlan { .. } => print(
            &host_install::plan_uninstall(&selection, now_ms()?, &deadline)?,
            max_bytes,
            &deadline,
        )?,
        Operation::Status { .. } => print(
            &host_install::status(&selection, &deadline)?,
            max_bytes,
            &deadline,
        )?,
        Operation::Apply { plan, .. } => {
            let plan: Plan = yoke::json::decode(super::input_json(plan, 8_000_000)?)?;
            ensure!(plan.files.len() <= 16, "Host plan file capacity exceeded");
            let paths = plan
                .files
                .iter()
                .map(|file| -> Result<String> {
                    let root = plan
                        .bindings
                        .roots
                        .get(file.root_index)
                        .context("Host plan root absent")?;
                    Ok(Path::new(&root.path)
                        .join(&file.path)
                        .to_str()
                        .context("Non-UTF8 host output path")?
                        .to_owned())
                })
                .collect::<Result<Vec<_>>>()?;
            admit_receipt(paths, max_bytes, &deadline)?;
            let authority = WriteAuthority::new(&selection);
            let receipt = host_install::apply(&selection, &authority, &plan, now_ms()?, &deadline)?;
            print_receipt(&receipt, max_bytes, &deadline)?;
        }
        Operation::RecoverInitialization { plan, .. } => {
            let plan: Plan = yoke::json::decode(super::input_json(plan, 8_000_000)?)?;
            ensure!(plan.files.len() <= 16, "Host plan file capacity exceeded");
            let token = host_install::initialization_token(&plan);
            // Admit the complete public result before initializing private host state.
            let largest = InitializationReceipt {
                schema_version: 1,
                id: "f".repeat(64),
                token: token.clone(),
                plan_id: plan.id.clone(),
                installation_id: plan.installation_id.clone(),
                state_root: plan.bindings.state_root.clone(),
                status: "initialized".into(),
            };
            yoke::json::encode_bounded(&largest, max_bytes - 1, &deadline)?;
            let authority = WriteAuthority::new(&selection);
            let receipt = host_install::recover_initialization(
                &selection,
                &authority,
                &plan,
                &token,
                now_ms()?,
                &deadline,
            )?;
            print_initialization_receipt(&receipt, max_bytes, &deadline)?;
        }
        Operation::Recover { journal, mode, .. } => {
            let paths = file
                .target_roots
                .iter()
                .map(|root| -> Result<String> {
                    let root = root.canonicalize()?;
                    Ok(root
                        .join(selection.target_filename())
                        .to_str()
                        .context("Non-UTF8 host output path")?
                        .to_owned())
                })
                .collect::<Result<Vec<_>>>()?;
            admit_receipt(paths, max_bytes, &deadline)?;
            let authority = WriteAuthority::new(&selection);
            let mode = match mode {
                RecoveryMode::Resume => host_install::RecoveryMode::Resume,
                RecoveryMode::Rollback => host_install::RecoveryMode::Rollback,
            };
            let receipt =
                host_install::recover(&selection, &authority, journal, mode, now_ms()?, &deadline)?;
            print_receipt(&receipt, max_bytes, &deadline)?;
        }
    }
    Ok(true)
}
