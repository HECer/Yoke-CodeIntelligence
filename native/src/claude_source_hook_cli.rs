//! Finite, short-lived callback process. Stalled stdin cannot hold the host turn.
use anyhow::{Result, ensure};
use clap::{Args, ValueEnum};
use std::io::{Read, Write};
#[derive(Clone, Copy, ValueEnum)]
enum Mode {
    SessionStart,
    PromptSubmit,
    PostEdit,
    Status,
}
#[derive(Args)]
pub struct Arguments {
    mode: Mode,
    #[arg(long)]
    launch_json: String,
}
impl Arguments {
    pub fn run(&self, timeout: u64, max_bytes: u64) -> Result<bool> {
        let result = (|| -> Result<()> {
            ensure!(
                (10..=600_000).contains(&timeout) && (1..=65_536).contains(&max_bytes),
                "Invalid source hook budgets"
            );
            let d = yoke::capture::Deadline::new(timeout);
            let (sender, receiver) = std::sync::mpsc::sync_channel(1);
            std::thread::spawn(move || {
                let mut bytes = Vec::new();
                let result = std::io::stdin()
                    .lock()
                    .take(65_537)
                    .read_to_end(&mut bytes)
                    .map(|_| bytes);
                let _ = sender.send(result);
            });
            let input = receiver.recv_timeout(d.remaining())??;
            use yoke::host_install::SourceHookMode as Hook;
            let mode = match self.mode {
                Mode::SessionStart => Hook::SessionStart,
                Mode::PromptSubmit => Hook::PromptSubmit,
                Mode::PostEdit => Hook::PostEdit,
                Mode::Status => Hook::Status,
            };
            let output = yoke::host_install::claude_source_hook(
                mode,
                self.launch_json.as_bytes(),
                &input,
                max_bytes as usize,
                &d,
            )?;
            let mut stdout = std::io::stdout().lock();
            stdout.write_all(&output)?;
            stdout.flush()?;
            Ok(())
        })();
        result
            .map(|()| true)
            .map_err(|_| anyhow::anyhow!("claude_source_hook_refused"))
    }
}
