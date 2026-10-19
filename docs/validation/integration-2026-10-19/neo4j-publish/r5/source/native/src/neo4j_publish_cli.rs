//! Explicit host Neo4j workflow; plans/configuration never grant capabilities.
use anyhow::{Result, ensure};
use clap::{Args, Subcommand};
use serde_json::{Value, json};
use std::{
    io::{IsTerminal, Read, Write},
    path::{Path, PathBuf},
};
use yoke::{
    capture::Deadline,
    neo4j_publish::{self as publish, Permissions, Plan, Target},
};
#[derive(Args)]
pub struct Arguments {
    #[command(subcommand)]
    operation: Operation,
}
#[derive(Args)]
struct Connection {
    #[arg(long)]
    allow_network: bool,
    #[arg(long)]
    allow_credential_lookup: bool,
    #[arg(long)]
    allow_loopback_http: bool,
}
impl Connection {
    fn permissions(&self) -> Result<Permissions> {
        ensure!(
            self.allow_network && self.allow_credential_lookup,
            "neo4j_publish_network_and_credential_permissions_required"
        );
        Ok(Permissions {
            allow_network: self.allow_network,
            allow_credential_lookup: self.allow_credential_lookup,
            allow_loopback_http: self.allow_loopback_http,
            ..Default::default()
        })
    }
}
#[derive(Subcommand)]
enum Operation {
    /// Prepare schema/bootstrap data offline. Does not connect or change schema.
    BootstrapPlan {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        allow_output: bool,
    },
    /// Prepare an existing published source snapshot; never recaptures or executes it.
    Prepare {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        source_root: PathBuf,
        #[arg(long)]
        snapshot_id: String,
        #[arg(long)]
        expected_revision: String,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        limits: Option<PathBuf>,
        #[arg(long)]
        allow_output: bool,
        #[arg(long)]
        allow_source_coordination: bool,
    },
    /// Inspect supplied plan bytes offline, without opening a source Store.
    Inspect {
        #[arg(long)]
        plan: PathBuf,
    },
    /// Read only the fixed schema and namespace from an explicitly selected server.
    Probe {
        #[arg(long)]
        plan: PathBuf,
        #[command(flatten)]
        connection: Connection,
    },
    /// Record an exact reviewed plan and bounded expiry in a fresh host journal.
    Approve {
        #[arg(long)]
        plan: PathBuf,
        #[arg(long)]
        expected_plan_id: String,
        #[arg(long)]
        journal: PathBuf,
        #[arg(long, default_value_t = 600000)]
        valid_for_ms: u64,
        #[arg(long)]
        allow_approval: bool,
        #[arg(long)]
        allow_journal: bool,
    },
    /// Apply fixed approved operations. Interrupted results require read recovery.
    Apply {
        #[arg(long)]
        plan: PathBuf,
        #[arg(long)]
        expected_plan_id: String,
        #[arg(long)]
        journal: PathBuf,
        #[command(flatten)]
        connection: Connection,
        #[arg(long)]
        allow_write: bool,
        #[arg(long)]
        allow_schema: bool,
        #[arg(long)]
        allow_journal: bool,
    },
    /// Reconcile a pending intent using fixed reads; never resends a write.
    Recover {
        #[arg(long)]
        plan: PathBuf,
        #[arg(long)]
        expected_plan_id: String,
        #[arg(long)]
        journal: PathBuf,
        #[command(flatten)]
        connection: Connection,
        #[arg(long)]
        allow_recovery: bool,
        #[arg(long)]
        allow_journal: bool,
    },
    /// Read the guarded local journal without a server connection.
    Journal {
        #[arg(long)]
        plan: PathBuf,
        #[arg(long)]
        journal: PathBuf,
        #[arg(long)]
        allow_journal: bool,
    },
    /// Import exact private stdin bytes, create-only, into the bound OS store.
    ImportPassword {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        allow_credential_create: bool,
    },
}
fn target(path: &Path, d: &Deadline) -> Result<Target> {
    let bytes = super::meaning_archive_cli::read_input(path, 400000, d)?;
    let value: Target = yoke::json::decode(yoke::json::parse(&bytes, 400000)?)
        .map_err(|_| anyhow::anyhow!("neo4j_publish_target_invalid"))?;
    value.validate()?;
    Ok(value)
}
fn plan(path: &Path, d: &Deadline) -> Result<Plan> {
    Plan::decode(
        &super::meaning_archive_cli::read_input(path, 32_000_000, d)?,
        d,
    )
}
fn effect_reply(
    plan: &Plan,
    journal: &Path,
    operation: &str,
    cap: usize,
    d: &Deadline,
) -> Result<Vec<u8>> {
    yoke::json::encode_bounded(
        &json!({"kind":"native_neo4j_host_result_v1","operation":operation,"planId":plan.id(),"journal":journal,"operationReturnedSuccess":true,"serverAttestation":"unverified","sourceAuthority":false,"complete":false,"modelCalls":0}),
        cap,
        d,
    )
}
fn after<T>(result: Result<T>, plan: &Plan, journal: &Path) -> Result<T> {
    result.map_err(|e| {
        anyhow::anyhow!(
            "neo4j_publish_host_operation_failed planId={} journal={} cause={e}",
            plan.id(),
            journal.display()
        )
    })
}
pub fn run(args: &Arguments, timeout: u64, max_bytes: u64) -> Result<bool> {
    ensure!(
        (10..=600000).contains(&timeout) && (1024..=32_000_000).contains(&max_bytes),
        "neo4j_publish_host_budget_invalid"
    );
    let d = Deadline::new(timeout);
    let cap = max_bytes as usize - 1;
    let mut committed = None;
    let mut identity_hint = None;
    let value = match &args.operation {
        Operation::BootstrapPlan {
            config,
            output,
            allow_output,
        } => {
            ensure!(*allow_output, "neo4j_publish_output_permission_required");
            let held = super::meaning_archive_cli::fresh_destination(output, &d)?;
            let target = target(config, &d)?;
            let plan = Plan::bootstrap(target, &d)?;
            let bytes = plan.encode(&d)?;
            let value = json!({"planId":plan.id(),"output":output,"planBytes":bytes.len(),"outputCreated":true,"networkConnections":0,"sourceAuthority":false,"complete":false});
            let reply = yoke::json::encode_bounded(&value, cap, &d)?;
            super::meaning_archive_cli::write_new(output, &bytes, &d)?;
            drop(held);
            committed = Some(reply);
            identity_hint = Some(format!("planId={} output={}", plan.id(), output.display()));
            value
        }
        Operation::Prepare {
            config,
            source_root,
            snapshot_id,
            expected_revision,
            output,
            limits,
            allow_output,
            allow_source_coordination,
        } => {
            ensure!(
                *allow_output && *allow_source_coordination,
                "neo4j_publish_output_and_source_coordination_permissions_required"
            );
            ensure!(
                yoke::model::valid_id(snapshot_id)
                    && (expected_revision == "absent" || yoke::model::valid_id(expected_revision)),
                "neo4j_publish_snapshot_or_precondition_invalid"
            );
            let held = super::meaning_archive_cli::fresh_destination(output, &d)?;
            let target = target(config, &d)?;
            let limits = if let Some(path) = limits {
                let bytes = super::meaning_archive_cli::read_input(path, 8192, &d)?;
                yoke::json::decode(yoke::json::parse(&bytes, 8192)?)?
            } else {
                Default::default()
            };
            yoke::json::encode_bounded(
                &json!({"planId":"f".repeat(64),"output":output,"planBytes":usize::MAX,"outputCreated":true,"networkConnections":0,"sourceAuthority":false,"complete":false}),
                cap,
                &d,
            )?;
            let source_root = source_root.canonicalize()?;
            let state = super::meaning_archive_cli::existing_source_state(&source_root, &d)?;
            let store = yoke::store::Store::open_existing_read_only(&source_root, &d)?;
            store.set_deadline(&d)?;
            let snapshot = store.snapshot(snapshot_id)?;
            let graph = store.graph(snapshot_id)?;
            let plan = Plan::prepare(
                &store,
                &snapshot,
                &graph,
                target,
                expected_revision,
                &limits,
                &d,
            )?;
            drop(store);
            drop(state);
            let bytes = plan.encode(&d)?;
            let value = json!({"planId":plan.id(),"output":output,"planBytes":bytes.len(),"outputCreated":true,"networkConnections":0,"sourceAuthority":false,"complete":false});
            let reply = yoke::json::encode_bounded(&value, cap, &d)?;
            super::meaning_archive_cli::write_new(output, &bytes, &d)?;
            drop(held);
            committed = Some(reply);
            identity_hint = Some(format!("planId={} output={}", plan.id(), output.display()));
            value
        }
        Operation::Inspect { plan: path } => plan(path, &d)?.inspect(&d)?,
        Operation::Probe {
            plan: path,
            connection,
        } => {
            let permissions = connection.permissions()?;
            let plan = plan(path, &d)?;
            publish::inspect_server(&plan, &permissions, &d)?
        }
        Operation::Approve {
            plan: path,
            expected_plan_id,
            journal,
            valid_for_ms,
            allow_approval,
            allow_journal,
        } => {
            ensure!(
                *allow_approval && *allow_journal,
                "neo4j_publish_approval_and_journal_permissions_required"
            );
            ensure!(
                (100..=3_600_000).contains(valid_for_ms),
                "neo4j_publish_approval_expiry_invalid"
            );
            let plan = plan(path, &d)?;
            let reply = effect_reply(&plan, journal, "approve", cap, &d)?;
            let now = u64::try_from(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)?
                    .as_millis(),
            )?;
            let value = publish::approve(
                &plan,
                expected_plan_id,
                journal,
                now.checked_add(*valid_for_ms)
                    .ok_or_else(|| anyhow::anyhow!("neo4j_publish_clock_invalid"))?,
                &Permissions {
                    allow_approval: true,
                    allow_journal: true,
                    ..Default::default()
                },
                &d,
            )?;
            committed = Some(reply);
            identity_hint = Some(format!(
                "planId={} journal={}",
                plan.id(),
                journal.display()
            ));
            value
        }
        Operation::Apply {
            plan: path,
            expected_plan_id,
            journal,
            connection,
            allow_write,
            allow_schema,
            allow_journal,
        } => {
            ensure!(
                *allow_write && *allow_journal,
                "neo4j_publish_write_and_journal_permissions_required"
            );
            let mut permissions = connection.permissions()?;
            permissions.allow_write = *allow_write;
            permissions.allow_schema = *allow_schema;
            permissions.allow_journal = *allow_journal;
            let plan = plan(path, &d)?;
            let reply = effect_reply(&plan, journal, "apply", cap, &d)?;
            let value = after(
                publish::apply(&plan, expected_plan_id, journal, &permissions, &d),
                &plan,
                journal,
            )?;
            committed = Some(reply);
            identity_hint = Some(format!(
                "planId={} journal={}",
                plan.id(),
                journal.display()
            ));
            value
        }
        Operation::Recover {
            plan: path,
            expected_plan_id,
            journal,
            connection,
            allow_recovery,
            allow_journal,
        } => {
            ensure!(
                *allow_recovery && *allow_journal,
                "neo4j_publish_recovery_and_journal_permissions_required"
            );
            let mut permissions = connection.permissions()?;
            permissions.allow_recovery = *allow_recovery;
            permissions.allow_journal = *allow_journal;
            let plan = plan(path, &d)?;
            let reply = effect_reply(&plan, journal, "recover", cap, &d)?;
            let value = after(
                publish::recover(&plan, expected_plan_id, journal, &permissions, &d),
                &plan,
                journal,
            )?;
            committed = Some(reply);
            identity_hint = Some(format!(
                "planId={} journal={}",
                plan.id(),
                journal.display()
            ));
            value
        }
        Operation::Journal {
            plan: path,
            journal,
            allow_journal,
        } => {
            ensure!(*allow_journal, "neo4j_publish_journal_permission_required");
            publish::inspect_journal(
                &plan(path, &d)?,
                journal,
                &Permissions {
                    allow_journal: true,
                    ..Default::default()
                },
                &d,
            )?
        }
        Operation::ImportPassword {
            config,
            allow_credential_create,
        } => {
            ensure!(
                *allow_credential_create,
                "neo4j_publish_credential_create_permission_required"
            );
            ensure!(
                !std::io::stdin().is_terminal(),
                "neo4j_publish_private_stdin_required"
            );
            let binding = target(config, &d)?.credential_binding()?;
            let reply = yoke::json::encode_bounded(
                &json!({"credentialBinding":binding,"credentialType":"neo4j","privateMaterial":"os_credential_store_only"}),
                cap,
                &d,
            )?;
            let mut secret = zeroize::Zeroizing::new(Vec::new());
            std::io::stdin().take(2561).read_to_end(&mut secret)?;
            d.check()?;
            let value = yoke::host_credentials::import_neo4j_password(&binding, &secret)?;
            committed = Some(reply);
            identity_hint = Some(format!("credentialBinding={binding}"));
            value
        }
    };
    let mut bytes = if let Some(reply) = committed {
        reply
    } else {
        yoke::json::encode_bounded(&value, cap, &d)?
    };
    bytes.push(b'\n');
    if std::io::stdout().lock().write_all(&bytes).is_err() {
        if let Some(hint) = identity_hint {
            anyhow::bail!("neo4j_publish_delivery_failed_after_effect {hint}")
        }
        anyhow::bail!("neo4j_publish_delivery_failed")
    }
    Ok(true)
}
