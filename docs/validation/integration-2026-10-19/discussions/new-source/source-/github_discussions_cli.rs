//! Host-file and network admission precede credentials or any source Store.
use anyhow::{Result, ensure};
use clap::{Args, Subcommand};
use std::{io::Write, path::PathBuf};
use yoke::{
    capture::Deadline,
    github::discussions::{self, Capture, Profile, Request},
};

#[derive(Args)]
pub struct Arguments {
    #[command(subcommand)]
    operation: Operation,
}
#[derive(Subcommand)]
enum Operation {
    Capture {
        #[arg(long)]
        profile: PathBuf,
        #[arg(long)]
        request: PathBuf,
        #[arg(long)]
        allow_network: bool,
        #[arg(long)]
        allow_discussion_text: bool,
    },
    Inspect {
        #[arg(long)]
        capture: PathBuf,
    },
}
impl Arguments {
    pub fn run(&self, timeout: u64, max_bytes: u64) -> Result<bool> {
        ensure!(
            (10..=600_000).contains(&timeout)
                && (1024..=Profile::default().max_output_bytes as u64).contains(&max_bytes),
            "github_discussions_capacity_exceeded"
        );
        let d = Deadline::new(timeout);
        let capture = match &self.operation {
            Operation::Capture {
                profile,
                request,
                allow_network,
                allow_discussion_text,
            } => {
                ensure!(
                    *allow_network && *allow_discussion_text,
                    "github_discussions_host_permission_required"
                );
                ensure!(
                    profile.is_absolute() && request.is_absolute(),
                    "github_discussions_input_invalid"
                );
                let mut profile =
                    Profile::parse(&crate::feedback_cli::host_bytes(profile, 32_000, &d)?)?;
                profile.max_output_bytes = profile.max_output_bytes.min(max_bytes as usize - 1);
                let request = Request::parse(&crate::feedback_cli::host_bytes(request, 8192, &d)?)?;
                discussions::capture_host(&profile, &request, &d)?
            }
            Operation::Inspect { capture } => {
                ensure!(capture.is_absolute(), "github_discussions_input_invalid");
                Capture::parse(
                    &crate::feedback_cli::host_bytes(
                        capture,
                        Profile::default().max_output_bytes,
                        &d,
                    )?,
                    &d,
                )?
            }
        };
        let mut raw = yoke::json::encode_bounded(&capture, max_bytes as usize - 1, &d)?;
        raw.push(b'\n');
        d.check()?;
        let mut stdout = std::io::stdout().lock();
        stdout.write_all(&raw)?;
        stdout.flush()?;
        Ok(true)
    }
}
