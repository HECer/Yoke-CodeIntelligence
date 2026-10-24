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
        HistoryArchiveAccess, HistoryArchivePlan, ProjectRoot, RecoveryDirection, WriteAuthority,
    },
};

#[derive(Args)]
pub struct Arguments {
    #[arg(long)]
    from_selection: PathBuf,
    #[arg(long)]
    to_selection: PathBuf,
    #[arg(long, default_value_t = 512)]
    history_record_budget: usize,
    #[command(subcommand)]
    operation: Operation,
}
#[derive(Subcommand)]
enum Operation {
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
        } = &self.operation
        {
            ensure!(
                *allow_host_writes,
                "{} requires --allow-host-writes",
                if matches!(
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
        | Operation::HistoryRecover { expected_event, .. } = &self.operation
        {
            ensure!(
                yoke::model::valid_id(expected_event),
                "Explicit migration event review required"
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
    let from = access(&args.from_selection, &d)?;
    let to = access(&args.to_selection, &d)?;
    match &args.operation {
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
}
