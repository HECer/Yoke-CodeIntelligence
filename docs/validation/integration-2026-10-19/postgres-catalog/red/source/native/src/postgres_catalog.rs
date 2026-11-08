//! Explicit catalog acquisition and data-only replay; never canonical source.
use crate::capture::Deadline;
use anyhow::{Result,bail};
use serde_json::Value;

pub struct Catalog;
impl Catalog {
    pub fn from_response(_bytes:&[u8],_d:&Deadline)->Result<Self>{bail!("postgres_catalog_unavailable")}
    pub fn as_value(&self)->Value{serde_json::json!({})}
}
