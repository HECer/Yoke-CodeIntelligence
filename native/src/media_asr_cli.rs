use super::MediaOperation;
use anyhow::{Result, ensure};
use serde_json::Value;
use std::path::Path;
use yoke::{capture::Deadline, media_asr, store::Store};

pub(super) fn execute(
    command: &MediaOperation,
    root: &Path,
    timeout: u64,
) -> Result<Option<Value>> {
    match command {
        MediaOperation::Inspect { .. } | MediaOperation::Transcript { .. } => Ok(None),
        MediaOperation::Transcribe {
            asset,
            config,
            trust_worker,
        } => {
            ensure!(*trust_worker, "ASR requires explicit host worker trust");
            let deadline = Deadline::new(timeout);
            let spec = media_asr::read_host_spec(config, &deadline)?;
            let existing = Store::open_existing_read_only(root, &deadline)?;
            media_asr::validate_audio(&existing, asset, &spec.limits, &deadline)?;
            let host = media_asr::Host::select(spec, true, &deadline)?;
            let store = Store::open_with_timeout(root, deadline.remaining())?;
            ensure!(
                store.workspace_id == existing.workspace_id,
                "ASR source workspace changed"
            );
            Ok(Some(media_asr::transcribe(
                &store, asset, &host, &deadline,
            )?))
        }
        MediaOperation::AsrStatus { job } => {
            let deadline = Deadline::new(timeout);
            let store = Store::open_existing_read_only(root, &deadline)?;
            Ok(Some(media_asr::status(&store, job, &deadline)?))
        }
        MediaOperation::AsrRead { job } => {
            let deadline = Deadline::new(timeout);
            let store = Store::open_existing_read_only(root, &deadline)?;
            Ok(Some(media_asr::read(&store, job, &deadline)?))
        }
        MediaOperation::AsrJobs {
            asset,
            offset,
            limit,
        } => {
            let deadline = Deadline::new(timeout);
            let store = Store::open_existing_read_only(root, &deadline)?;
            Ok(Some(media_asr::jobs(
                &store, asset, *offset, *limit, &deadline,
            )?))
        }
    }
}
