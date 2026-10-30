//! Explicit host workflow; dashboard requests cannot supply these selections.
use anyhow::{Context, Result, ensure};
use clap::{Args, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    io::{Read, Write},
    path::{Path, PathBuf},
};
use yoke::{
    capture::Deadline,
    host_install::documents::{
        self, Access, BindingMigrationAuthority, BindingMigrationPlan, Config,
        HistoryArchiveAccess, HistoryArchivePlan, HistoryArchivePurgeAccess,
        HistoryArchivePurgePermission, HistoryArchivePurgePlan, OwnerRetirementPlan, ProjectRoot,
        RecoveryDirection, WriteAuthority,
    },
};

#[derive(Args)]
pub struct Arguments {
    #[arg(long)]
    from_selection: PathBuf,
    #[arg(long)]
    to_selection: Option<PathBuf>,
    #[arg(long, default_value_t = 512)]
    history_record_budget: usize,
    #[command(subcommand)]
    operation: Operation,
}
#[derive(Subcommand)]
enum Operation {
    #[command(name = "owner-retirement-plan")]
    OwnerRetirementPlan {
        #[arg(long)]
        document: String,
    },
    #[command(name = "owner-retirement-inspect")]
    OwnerRetirementInspect {
        #[arg(long)]
        plan: PathBuf,
    },
    #[command(name = "owner-retirement-apply")]
    OwnerRetirementApply {
        #[arg(long)]
        plan: PathBuf,
        #[arg(long)]
        allow_host_writes: bool,
    },
    #[command(name = "owner-retirement-recover")]
    OwnerRetirementRecover {
        #[arg(long)]
        plan: PathBuf,
        #[arg(long)]
        expected_event: String,
        #[arg(long, value_enum)]
        direction: Direction,
        #[arg(long)]
        allow_host_writes: bool,
    },
    #[command(name = "history-purge-plan")]
    HistoryPurgePlan {
        #[arg(long)]
        archive_plan: PathBuf,
        #[arg(long)]
        archive_root: PathBuf,
    },
    #[command(name = "history-purge-inspect")]
    HistoryPurgeInspect {
        #[arg(long)]
        plan: PathBuf,
        #[arg(long)]
        archive_root: PathBuf,
    },
    #[command(name = "history-purge-apply")]
    HistoryPurgeApply {
        #[arg(long)]
        plan: PathBuf,
        #[arg(long)]
        archive_root: PathBuf,
        #[arg(long)]
        confirmed_plan: String,
        #[arg(long)]
        allow_host_writes: bool,
        #[arg(long)]
        allow_state_deletion: bool,
    },
    #[command(name = "history-purge-recover")]
    HistoryPurgeRecover {
        #[arg(long)]
        plan: PathBuf,
        #[arg(long)]
        archive_root: PathBuf,
        #[arg(long)]
        expected_event: String,
        #[arg(long, value_enum)]
        direction: Direction,
        #[arg(long)]
        allow_host_writes: bool,
        #[arg(long)]
        allow_state_deletion: bool,
    },
    #[command(name = "history-read")]
    HistoryRead {
        #[arg(long)]
        plan: PathBuf,
        #[arg(long)]
        archive_root: PathBuf,
        #[arg(long)]
        record: String,
    },
    #[command(name = "history-plan")]
    HistoryPlan {
        #[arg(long)]
        migration: PathBuf,
        #[arg(long)]
        archive_root: PathBuf,
    },
    #[command(name = "history-inspect")]
    HistoryInspect {
        #[arg(long)]
        plan: PathBuf,
        #[arg(long)]
        archive_root: PathBuf,
    },
    #[command(name = "history-apply")]
    HistoryApply {
        #[arg(long)]
        plan: PathBuf,
        #[arg(long)]
        archive_root: PathBuf,
        #[arg(long)]
        allow_host_writes: bool,
    },
    #[command(name = "history-recover")]
    HistoryRecover {
        #[arg(long)]
        plan: PathBuf,
        #[arg(long)]
        archive_root: PathBuf,
        #[arg(long)]
        expected_event: String,
        #[arg(long, value_enum)]
        direction: Direction,
        #[arg(long)]
        allow_host_writes: bool,
    },
    #[command(name = "binding-plan")]
    Plan {
        #[arg(long)]
        from_document: String,
        #[arg(long)]
        to_document: String,
    },
    #[command(name = "binding-inspect")]
    Inspect {
        #[arg(long)]
        plan: PathBuf,
    },
    #[command(name = "binding-apply")]
    Apply {
        #[arg(long)]
        plan: PathBuf,
        #[arg(long)]
        allow_host_writes: bool,
    },
    #[command(name = "binding-recover")]
    Recover {
        #[arg(long)]
        plan: PathBuf,
        #[arg(long)]
        expected_event: String,
        #[arg(long, value_enum)]
        direction: Direction,
        #[arg(long)]
        allow_host_writes: bool,
    },
}
#[derive(Clone, Copy, ValueEnum)]
enum Direction {
    Resume,
    Rollback,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SelectionFile {
    documents: Config,
    projects: Vec<RegisteredProject>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RegisteredProject {
    name: String,
    root: PathBuf,
}
impl Arguments {
    pub fn preflight_permission(&self) -> Result<()> {
        if let Operation::HistoryPurgeApply {
            allow_host_writes,
            allow_state_deletion,
            ..
        }
        | Operation::HistoryPurgeRecover {
            allow_host_writes,
            allow_state_deletion,
            ..
        } = &self.operation
        {
            ensure!(
                *allow_host_writes,
                "Host history purge requires --allow-host-writes"
            );
            ensure!(
                *allow_state_deletion,
                "Host history purge requires separate --allow-state-deletion"
            );
        }
        if let Operation::HistoryPurgeApply { confirmed_plan, .. } = &self.operation {
            ensure!(
                yoke::model::valid_id(confirmed_plan),
                "Exact reviewed purge --confirmed-plan required"
            );
        }
        if let Operation::Apply {
            allow_host_writes, ..
        }
        | Operation::Recover {
            allow_host_writes, ..
        }
        | Operation::HistoryApply {
            allow_host_writes, ..
        }
        | Operation::HistoryRecover {
            allow_host_writes, ..
        }
        | Operation::OwnerRetirementApply {
            allow_host_writes, ..
        }
        | Operation::OwnerRetirementRecover {
            allow_host_writes, ..
        } = &self.operation
        {
            ensure!(
                *allow_host_writes,
                "{} requires --allow-host-writes",
                if matches!(
                    self.operation,
                    Operation::OwnerRetirementApply { .. }
                        | Operation::OwnerRetirementRecover { .. }
                ) {
                    "Host owner retirement"
                } else if matches!(
                    self.operation,
                    Operation::HistoryApply { .. } | Operation::HistoryRecover { .. }
                ) {
                    "Host history archival"
                } else {
                    "Host binding migration"
                }
            );
        }
        if let Operation::Recover { expected_event, .. }
        | Operation::HistoryRecover { expected_event, .. }
        | Operation::OwnerRetirementRecover { expected_event, .. } = &self.operation
        {
            ensure!(
                yoke::model::valid_id(expected_event),
                "{}",
                if matches!(self.operation, Operation::OwnerRetirementRecover { .. }) {
                    "Explicit owner retirement event review required"
                } else {
                    "Explicit migration event review required"
                }
            );
        }
        if let Operation::HistoryPurgeRecover { expected_event, .. } = &self.operation {
            ensure!(
                yoke::model::valid_id(expected_event),
                "Explicit current purge event review required"
            );
        }
        Ok(())
    }
}
fn read<T: serde::de::DeserializeOwned>(path: &Path, maximum: usize, d: &Deadline) -> Result<T> {
    d.check()?;
    let metadata = std::fs::symlink_metadata(path).context("Cannot inspect explicit host input")?;
    ensure!(
        metadata.is_file()
            && !metadata.file_type().is_symlink()
            && metadata.len() <= maximum as u64,
        "Explicit host input is not a bounded regular file"
    );
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(maximum as u64 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= maximum, "Host input capacity exceeded");
    d.check()?;
    yoke::json::decode(yoke::json::parse(&bytes, maximum)?)
}
fn access(path: &Path, d: &Deadline) -> Result<Access> {
    let selection: SelectionFile = read(path, 131072, d)?;
    ensure!(
        (1..=32).contains(&selection.projects.len()),
        "Supply the complete registered analyzed-root set, including the primary project"
    );
    let mut names = BTreeSet::new();
    let mut roots = Vec::new();
    for project in selection.projects {
        ensure!(
            !project.name.is_empty()
                && project.name.len() <= 200
                && !project.name.chars().any(char::is_control)
                && names.insert(project.name.clone()),
            "Invalid registered project selection"
        );
        ensure!(
            project.root.is_absolute(),
            "Registered analyzed roots must be absolute"
        );
        roots.push(ProjectRoot {
            name: project.name,
            path: project.root,
        });
    }
    Access::open(&selection.documents, &roots, d)
}
fn print<T: Serialize>(value: &T, maximum: usize, d: &Deadline) -> Result<()> {
    let bytes = yoke::json::encode_bounded(value, maximum - 1, d)?;
    d.check()?;
    let mut stdout = std::io::stdout().lock();
    stdout.write_all(&bytes)?;
    stdout.write_all(b"\n")?;
    stdout.flush()?;
    Ok(())
}
pub fn run(args: &Arguments, timeout: u64, max_bytes: u64) -> Result<()> {
    args.preflight_permission()?;
    let maximum = usize::try_from(max_bytes)?;
    ensure!(
        (256..=16_000_000).contains(&maximum),
        "Host migration output budget refused"
    );
    let d = Deadline::new(timeout);
    let retirement = matches!(
        args.operation,
        Operation::OwnerRetirementPlan { .. }
            | Operation::OwnerRetirementInspect { .. }
            | Operation::OwnerRetirementApply { .. }
            | Operation::OwnerRetirementRecover { .. }
    );
    ensure!(
        retirement || args.to_selection.is_some(),
        "Binding migration and history operations require --to-selection"
    );
    ensure!(
        !retirement || args.to_selection.is_none(),
        "Standalone owner retirement accepts only the original --from-selection"
    );
    let from = access(&args.from_selection, &d)?;
    match &args.operation {
        Operation::OwnerRetirementPlan { document } => {
            return print(
                &documents::plan_owner_retirement(&from, document, &d)?,
                maximum,
                &d,
            );
        }
        Operation::OwnerRetirementInspect { plan } => {
            let p: OwnerRetirementPlan = read(plan, 32000, &d)?;
            return print(
                &documents::inspect_owner_retirement(&from, &p, &d)?,
                maximum,
                &d,
            );
        }
        Operation::OwnerRetirementApply { plan, .. }
        | Operation::OwnerRetirementRecover { plan, .. } => {
            let p: OwnerRetirementPlan = read(plan, 32000, &d)?;
            let authority = WriteAuthority::new(&from)?;
            let result = match &args.operation {
                Operation::OwnerRetirementApply { .. } => {
                    let approval = authority.approve_owner_retirement(&from, &p, &d)?;
                    documents::apply_owner_retirement(&from, &authority, &approval, &p, &d)
                }
                Operation::OwnerRetirementRecover {
                    expected_event,
                    direction,
                    ..
                } => documents::recover_owner_retirement(
                    &from,
                    &authority,
                    &p,
                    expected_event,
                    match direction {
                        Direction::Resume => RecoveryDirection::Resume,
                        Direction::Rollback => RecoveryDirection::Rollback,
                    },
                    &d,
                ),
                _ => unreachable!(),
            };
            let receipt = result.with_context(|| format!("Owner retirement {} did not confirm completion; preserve the fixed owner and original journals and inspect before retrying", p.id))?;
            return print(&receipt, maximum, &d).with_context(|| {
                format!(
                    "Owner retirement {} completed durably; output delivery failed",
                    p.id
                )
            });
        }
        _ => {}
    }
    let to = access(
        args.to_selection
            .as_deref()
            .context("Destination selection required")?,
        &d,
    )?;
    match &args.operation {
        Operation::HistoryPurgePlan {
            archive_plan,
            archive_root,
        } => {
            let archive: HistoryArchivePlan = read(archive_plan, 512000, &d)?;
            ensure!(
                archive.record_budget == args.history_record_budget,
                "Explicit history record budget differs from selected archive"
            );
            let a = WriteAuthority::new(&from)?;
            let b = WriteAuthority::new(&to)?;
            let access =
                HistoryArchivePurgeAccess::new(&from, &to, &archive, archive_root, &a, &b, &d)?;
            print(
                &documents::plan_history_archive_purge(&from, &to, &access, &archive, &d)?,
                maximum,
                &d,
            )
        }
        Operation::HistoryPurgeInspect { plan, archive_root }
        | Operation::HistoryPurgeApply {
            plan, archive_root, ..
        }
        | Operation::HistoryPurgeRecover {
            plan, archive_root, ..
        } => {
            let plan: HistoryArchivePurgePlan = read(plan, 512000, &d)?;
            ensure!(
                plan.archive.record_budget == args.history_record_budget,
                "Explicit history record budget differs from selected archive"
            );
            let a = WriteAuthority::new(&from)?;
            let b = WriteAuthority::new(&to)?;
            let access = HistoryArchivePurgeAccess::new(
                &from,
                &to,
                &plan.archive,
                archive_root,
                &a,
                &b,
                &d,
            )?;
            let result = match &args.operation {
                Operation::HistoryPurgeInspect { .. } => {
                    documents::inspect_history_archive_purge(&from, &to, &access, &plan, &d)
                }
                Operation::HistoryPurgeApply {
                    confirmed_plan,
                    allow_state_deletion,
                    ..
                } => {
                    ensure!(
                        *confirmed_plan == plan.id,
                        "Exact reviewed purge plan confirmation differs"
                    );
                    let permission = HistoryArchivePurgePermission::new(*allow_state_deletion)?;
                    let approved = access.approve(&from, &to, &plan, &d)?;
                    documents::apply_history_archive_purge(
                        &from,
                        &to,
                        &access,
                        &approved,
                        &permission,
                        &plan,
                        maximum,
                        &d,
                    )
                }
                Operation::HistoryPurgeRecover {
                    expected_event,
                    direction,
                    allow_state_deletion,
                    ..
                } => {
                    let permission = HistoryArchivePurgePermission::new(*allow_state_deletion)?;
                    documents::recover_history_archive_purge(
                        &from,
                        &to,
                        &access,
                        &permission,
                        &plan,
                        expected_event,
                        match direction {
                            Direction::Resume => RecoveryDirection::Resume,
                            Direction::Rollback => RecoveryDirection::Rollback,
                        },
                        maximum,
                        &d,
                    )
                }
                _ => unreachable!(),
            };
            let receipt = result.with_context(|| format!("History archive purge {} did not confirm completion; preserve retained receipts and inspect before event-reviewed recovery", plan.id))?;
            print(&receipt, maximum, &d).with_context(|| format!("History archive purge {} receipt output delivery failed; inspect retained evidence", plan.id))
        }
        Operation::HistoryPlan {
            migration,
            archive_root,
        } => {
            let migration: BindingMigrationPlan = read(migration, 32000, &d)?;
            let a = WriteAuthority::new(&from)?;
            let b = WriteAuthority::new(&to)?;
            let access =
                HistoryArchiveAccess::new(&from, &to, &migration, archive_root, &a, &b, &d)?
                    .with_record_budget(args.history_record_budget)?;
            print(
                &documents::plan_history_archive(&from, &to, &access, &migration, &d)?,
                maximum,
                &d,
            )
        }
        Operation::HistoryInspect { plan, archive_root }
        | Operation::HistoryRead {
            plan, archive_root, ..
        }
        | Operation::HistoryApply {
            plan, archive_root, ..
        }
        | Operation::HistoryRecover {
            plan, archive_root, ..
        } => {
            let plan: HistoryArchivePlan = read(plan, 512000, &d)?;
            let a = WriteAuthority::new(&from)?;
            let b = WriteAuthority::new(&to)?;
            let access =
                HistoryArchiveAccess::new(&from, &to, &plan.migration, archive_root, &a, &b, &d)?
                    .with_record_budget(args.history_record_budget)?;
            let result = match &args.operation {
                Operation::HistoryRead { record, .. } => {
                    return print(
                        &documents::read_history_archive_record(
                            &from, &to, &access, &plan, record, &d,
                        )?,
                        maximum,
                        &d,
                    );
                }
                Operation::HistoryInspect { .. } => {
                    documents::inspect_history_archive(&from, &to, &access, &plan, &d)
                }
                Operation::HistoryApply { .. } => {
                    let approved = access.approve(&from, &to, &plan, &d)?;
                    documents::apply_history_archive(&from, &to, &access, &approved, &plan, &d)
                }
                Operation::HistoryRecover {
                    expected_event,
                    direction,
                    ..
                } => documents::recover_history_archive(
                    &from,
                    &to,
                    &access,
                    &plan,
                    expected_event,
                    match direction {
                        Direction::Resume => RecoveryDirection::Resume,
                        Direction::Rollback => RecoveryDirection::Rollback,
                    },
                    &d,
                ),
                _ => unreachable!(),
            };
            let receipt = result.with_context(|| format!("History archive {} did not confirm completion; preserve the archive and original journals and inspect before retrying", plan.id))?;
            print(&receipt, maximum, &d).with_context(|| {
                format!(
                    "History archive {} completed; output delivery failed",
                    plan.id
                )
            })
        }
        Operation::Plan {
            from_document,
            to_document,
        } => {
            let plan =
                documents::plan_binding_migration(&from, &to, from_document, to_document, &d)?;
            print(&plan, maximum, &d)
        }
        Operation::Inspect { plan } => {
            let plan: BindingMigrationPlan = read(plan, 32000, &d)?;
            print(
                &documents::inspect_binding_migration(&from, &to, &plan, &d)?,
                maximum,
                &d,
            )
        }
        Operation::Apply { plan, .. } | Operation::Recover { plan, .. } => {
            let plan: BindingMigrationPlan = read(plan, 32000, &d)?;
            let a = WriteAuthority::new(&from)?;
            let b = WriteAuthority::new(&to)?;
            let authority = BindingMigrationAuthority::new(
                &from,
                &to,
                &plan.from_document_id,
                &plan.to_document_id,
                &a,
                &b,
            )?;
            let result = match &args.operation {
                Operation::Apply { .. } => {
                    let approval = authority.approve(&from, &to, &plan, &d)?;
                    documents::apply_binding_migration(&from, &to, &authority, &approval, &plan, &d)
                }
                Operation::Recover {
                    expected_event,
                    direction,
                    ..
                } => documents::recover_binding_migration(
                    &from,
                    &to,
                    &authority,
                    &plan,
                    expected_event,
                    match direction {
                        Direction::Resume => RecoveryDirection::Resume,
                        Direction::Rollback => RecoveryDirection::Rollback,
                    },
                    &d,
                ),
                _ => unreachable!(),
            };
            let receipt=result.with_context(||format!("Binding migration {} did not confirm completion; inspect its original journals before retrying",plan.id))?;
            print(&receipt, maximum, &d).with_context(|| {
                format!(
                    "Binding migration {} completed durably; output delivery failed",
                    plan.id
                )
            })
        }
        Operation::OwnerRetirementPlan { .. }
        | Operation::OwnerRetirementInspect { .. }
        | Operation::OwnerRetirementApply { .. }
        | Operation::OwnerRetirementRecover { .. } => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    #[derive(Parser)]
    struct Cli {
        #[command(flatten)]
        args: Arguments,
    }
    #[test]
    fn mutation_permission_precedes_even_missing_host_inputs() {
        let event = "a".repeat(64);
        for operation in [
            vec!["binding-apply", "--plan", "missing.json"],
            vec![
                "binding-recover",
                "--plan",
                "missing.json",
                "--expected-event",
                &event,
                "--direction",
                "rollback",
            ],
        ] {
            let mut input = vec![
                "owned",
                "--from-selection",
                "missing-old.json",
                "--to-selection",
                "missing-new.json",
            ];
            input.extend(operation);
            let parsed = Cli::try_parse_from(input).unwrap();
            let error = run(&parsed.args, 10000, 64000).unwrap_err();
            assert!(error.to_string().contains("--allow-host-writes"));
        }
    }
    #[test]
    fn history_mutation_permission_precedes_missing_selection_plan_and_archive_paths() {
        let event = "a".repeat(64);
        for operation in [
            vec![
                "history-apply",
                "--plan",
                "missing-plan.json",
                "--archive-root",
                "missing-archive",
            ],
            vec![
                "history-recover",
                "--plan",
                "missing-plan.json",
                "--archive-root",
                "missing-archive",
                "--expected-event",
                &event,
                "--direction",
                "resume",
            ],
        ] {
            let mut input = vec![
                "owned",
                "--from-selection",
                "missing-old.json",
                "--to-selection",
                "missing-new.json",
            ];
            input.extend(operation);
            let parsed = Cli::try_parse_from(input).unwrap();
            assert!(
                run(&parsed.args, 10000, 64000)
                    .unwrap_err()
                    .to_string()
                    .contains("Host history archival requires --allow-host-writes")
            );
        }
    }
    #[test]
    fn purge_permissions_and_current_event_review_precede_missing_host_inputs() {
        let id = "a".repeat(64);
        for operation in ["history-purge-apply", "history-purge-recover"] {
            for grant in 0..2 {
                let mut args = vec![
                    "owned",
                    "--from-selection",
                    "missing-old.json",
                    "--to-selection",
                    "missing-new.json",
                    operation,
                    "--plan",
                    "missing.json",
                    "--archive-root",
                    "missing-root",
                ];
                if operation == "history-purge-apply" {
                    args.extend(["--confirmed-plan", &id]);
                } else {
                    args.extend(["--expected-event", &id, "--direction", "resume"]);
                }
                if grant == 1 {
                    args.push("--allow-host-writes");
                }
                let parsed = Cli::try_parse_from(args).unwrap();
                let error = run(&parsed.args, 10_000, 64_000).unwrap_err().to_string();
                assert!(error.contains(if grant == 0 {
                    "--allow-host-writes"
                } else {
                    "--allow-state-deletion"
                }));
            }
        }
        for (operation, review) in [
            ("history-purge-apply", "--confirmed-plan"),
            ("history-purge-recover", "--expected-event"),
        ] {
            let mut args = vec![
                "owned",
                "--from-selection",
                "missing-old.json",
                "--to-selection",
                "missing-new.json",
                operation,
                "--plan",
                "missing.json",
                "--archive-root",
                "missing-root",
                review,
                "../not-an-id",
                "--allow-host-writes",
                "--allow-state-deletion",
            ];
            if operation == "history-purge-recover" {
                args.extend(["--direction", "resume"]);
            }
            let parsed = Cli::try_parse_from(args).unwrap();
            assert!(
                run(&parsed.args, 10_000, 64_000)
                    .unwrap_err()
                    .to_string()
                    .contains("review")
            );
        }
    }
    #[test]
    fn owner_retirement_permission_and_event_review_precede_missing_host_inputs() {
        let event = "a".repeat(64);
        for operation in [
            vec!["owner-retirement-apply", "--plan", "missing-plan.json"],
            vec![
                "owner-retirement-recover",
                "--plan",
                "missing-plan.json",
                "--expected-event",
                &event,
                "--direction",
                "rollback",
            ],
        ] {
            let mut input = vec!["owned", "--from-selection", "missing-original.json"];
            input.extend(operation);
            let parsed = Cli::try_parse_from(input).unwrap();
            assert!(parsed.args.to_selection.is_none());
            assert!(
                run(&parsed.args, 10000, 64000)
                    .unwrap_err()
                    .to_string()
                    .contains("Host owner retirement requires --allow-host-writes")
            );
        }
        let parsed = Cli::try_parse_from([
            "owned",
            "--from-selection",
            "missing-original.json",
            "owner-retirement-recover",
            "--plan",
            "missing-plan.json",
            "--expected-event",
            "../foreign",
            "--direction",
            "resume",
            "--allow-host-writes",
        ])
        .unwrap();
        assert!(
            run(&parsed.args, 10000, 64000)
                .unwrap_err()
                .to_string()
                .contains("Explicit owner retirement event review required")
        );
    }

    #[test]
    fn standalone_retirement_refuses_a_second_selection_and_other_operations_still_require_it() {
        let parsed = Cli::try_parse_from([
            "owned",
            "--from-selection",
            "missing-original.json",
            "--to-selection",
            "unexpected-second.json",
            "owner-retirement-plan",
            "--document",
            "owned",
        ])
        .unwrap();
        assert!(
            run(&parsed.args, 10000, 64000)
                .unwrap_err()
                .to_string()
                .contains("only the original --from-selection")
        );
        let parsed = Cli::try_parse_from([
            "owned",
            "--from-selection",
            "missing-original.json",
            "binding-plan",
            "--from-document",
            "owned",
            "--to-document",
            "owned",
        ])
        .unwrap();
        assert!(
            run(&parsed.args, 10000, 64000)
                .unwrap_err()
                .to_string()
                .contains("require --to-selection")
        );
    }
}
