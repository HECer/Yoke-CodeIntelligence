//! Sole stdin reader in a short-lived hook process; timeout exits the process.
use anyhow::{Result, ensure};
use clap::Args;
use std::io::{Read, Write};
#[derive(Args)]
pub struct Arguments {
    #[arg(long)]
    launch_json: String,
}
impl Arguments {
    pub fn run(&self, timeout: u64, max_bytes: u64) -> Result<bool> {
        let result = (|| -> Result<()> {
            ensure!(
                (10..=600_000).contains(&timeout) && (1..=65_536).contains(&max_bytes),
                "Invalid hook budgets"
            );
            let d = yoke::capture::Deadline::new(timeout);
            let (sender, receiver) = std::sync::mpsc::sync_channel(1);
            // A stalled host pipe cannot block the deadline. This command always
            // exits after this function; the bounded reader has no other authority.
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
            let output = yoke::host_install::claude_session_start(
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
        // No input, path, policy or transcript value is exposed in errors.
        result
            .map(|()| true)
            .map_err(|_| anyhow::anyhow!("claude_session_start_refused"))
    }
}
