//! Bounded Graph serialization for retrieval correlation, never source authority.

use crate::{
    capture::Deadline,
    retrieval_scopes::{RetrievalHardError, check_deadline},
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::io::{self, Write};

pub(crate) const MAX_GRAPH_BYTES: usize = 64_000_000;

struct HashWriter<'a> {
    deadline: &'a Deadline,
    bytes: usize,
    digest: Sha256,
    failure: Option<RetrievalHardError>,
}
impl Write for HashWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let admission = check_deadline(self.deadline).and_then(|()| {
            if bytes.len() > MAX_GRAPH_BYTES.saturating_sub(self.bytes) {
                Err(RetrievalHardError::Capacity("graph_json_bytes"))
            } else {
                Ok(())
            }
        });
        if let Err(error) = admission {
            self.failure.get_or_insert(error);
            return Err(io::Error::other("bounded graph stream refused"));
        }
        self.digest.update(bytes);
        self.bytes += bytes.len();
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        if let Err(error) = check_deadline(self.deadline) {
            self.failure.get_or_insert(error);
            return Err(io::Error::other("bounded graph stream refused"));
        }
        Ok(())
    }
}

// Required coupling on later promotion: begin must apply this to its supplied
// original Graph BEFORE loading/cloning the stored Graph or opening any source.
// A digest is correlation data; this function cannot create a checked run.
pub(crate) fn bounded_graph_fingerprint<T: Serialize + ?Sized>(
    value: &T,
    deadline: &Deadline,
) -> Result<String, RetrievalHardError> {
    check_deadline(deadline)?;
    let mut writer = HashWriter {
        deadline,
        bytes: 0,
        digest: Sha256::new(),
        failure: None,
    };
    let result = serde_json::to_writer(&mut writer, value);
    check_deadline(deadline)?;
    if let Some(error) = writer.failure {
        return Err(error);
    }
    result.map_err(|_| RetrievalHardError::Internal("graph_serialization"))?;
    Ok(hex::encode(writer.digest.finalize()))
}

#[cfg(test)]
#[path = "retrieval_capacity_tests.rs"]
mod tests;
