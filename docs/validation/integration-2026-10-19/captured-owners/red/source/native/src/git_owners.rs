//! Captured per-path authored owner candidates over unchanged impact evidence.
use crate::{capture::Deadline,store::Store};
use anyhow::{Result,bail};
use serde_json::Value;
pub fn derive(_store:&Store,_report:&str,_d:&Deadline)->Result<Value>{
    bail!("git_owners_unavailable")
}
