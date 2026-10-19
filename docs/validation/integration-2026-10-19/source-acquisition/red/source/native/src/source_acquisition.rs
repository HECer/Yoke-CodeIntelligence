//! Received HTTP representations are immutable data, never host authority.
use crate::capture::Deadline;
use anyhow::Result;
use serde_json::Value;

pub struct ReceivedSource;
impl ReceivedSource {
    pub fn from_response(_metadata: &Value, _body: &[u8], _deadline: &Deadline) -> Result<Self> {
        anyhow::bail!("source_acquisition_unavailable")
    }
    pub fn as_value(&self) -> Value {
        serde_json::json!({})
    }
    pub fn body(&self) -> &[u8] {
        &[]
    }
    pub fn encode_archive(&self, _deadline: &Deadline) -> Result<Vec<u8>> {
        anyhow::bail!("source_acquisition_unavailable")
    }
    pub fn load_archive(_bytes: &[u8], _deadline: &Deadline) -> Result<Self> {
        anyhow::bail!("source_acquisition_unavailable")
    }
}
