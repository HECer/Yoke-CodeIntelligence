//! Bounded RESP2 bytes. This module opens no connection and grants no authority.
use crate::capture::Deadline;
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::io::{Cursor, Read};
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    Simple(Vec<u8>),
    Error(Vec<u8>),
    Integer(i64),
    Bulk(Option<Vec<u8>>),
    Array(Option<Vec<Value>>),
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Limits {
    pub max_bytes: usize,
    pub max_bulk_bytes: usize,
    pub max_values: usize,
    pub max_depth: usize,
    pub max_line_bytes: usize,
    pub max_arguments: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_bytes: 32_000_000,
            max_bulk_bytes: 32_000_000,
            max_values: 100000,
            max_depth: 16,
            max_line_bytes: 4096,
            max_arguments: 16,
        }
    }
}
impl Limits {
    pub fn validate(&self) -> Result<()> {
        let h = Self::default();
        ensure!(
            self.max_bytes > 0
                && self.max_bytes <= h.max_bytes
                && self.max_bulk_bytes <= h.max_bulk_bytes
                && self.max_values > 0
                && self.max_values <= h.max_values
                && self.max_depth <= h.max_depth
                && self.max_line_bytes > 0
                && self.max_line_bytes <= h.max_line_bytes
                && self.max_arguments > 0
                && self.max_arguments <= h.max_arguments,
            "falkor_resp_limits_invalid"
        );
        Ok(())
    }
}
/// Commands may contain credentials; keep the returned buffer private.
pub fn encode(
    args: &[&[u8]],
    limits: &Limits,
    d: &Deadline,
) -> Result<zeroize::Zeroizing<Vec<u8>>> {
    d.check()?;
    limits.validate()?;
    ensure!(
        !args.is_empty() && args.len() <= limits.max_arguments,
        "falkor_resp_capacity_exceeded"
    );
    let header = format!("*{}\r\n", args.len());
    let mut size = header.len();
    for value in args {
        d.check()?;
        ensure!(
            value.len() <= limits.max_bulk_bytes,
            "falkor_resp_capacity_exceeded"
        );
        size = size
            .checked_add(format!("${}\r\n", value.len()).len())
            .and_then(|n| n.checked_add(value.len()))
            .and_then(|n| n.checked_add(2))
            .ok_or_else(|| anyhow::anyhow!("falkor_resp_capacity_exceeded"))?;
        ensure!(size <= limits.max_bytes, "falkor_resp_capacity_exceeded");
    }
    let mut out = zeroize::Zeroizing::new(Vec::with_capacity(size));
    out.extend_from_slice(header.as_bytes());
    for value in args {
        d.check()?;
        out.extend_from_slice(format!("${}\r\n", value.len()).as_bytes());
        out.extend_from_slice(value);
        out.extend_from_slice(b"\r\n");
    }
    d.check()?;
    Ok(out)
}
struct Decoder<'a, R> {
    reader: &'a mut R,
    limits: &'a Limits,
    deadline: &'a Deadline,
    bytes: usize,
    values: usize,
}
impl<R: Read> Decoder<'_, R> {
    fn exact(&mut self, out: &mut [u8]) -> Result<()> {
        ensure!(
            self.bytes
                .checked_add(out.len())
                .is_some_and(|n| n <= self.limits.max_bytes),
            "falkor_resp_capacity_exceeded"
        );
        let mut offset = 0;
        while offset < out.len() {
            self.deadline.check()?;
            let n = match self.reader.read(&mut out[offset..]) {
                Ok(0) => anyhow::bail!("falkor_resp_incomplete"),
                Ok(n) => n,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => anyhow::bail!("falkor_resp_read_failed"),
            };
            ensure!(n <= out.len() - offset, "falkor_resp_invalid");
            offset += n;
            self.bytes += n;
            self.deadline.check()?;
        }
        Ok(())
    }
    fn line(&mut self) -> Result<Vec<u8>> {
        let mut bytes = Vec::new();
        loop {
            let mut byte = [0];
            self.exact(&mut byte)?;
            if byte[0] == b'\r' {
                let mut last = [0];
                self.exact(&mut last)?;
                ensure!(last == [b'\n'], "falkor_resp_invalid");
                return Ok(bytes);
            }
            ensure!(
                byte[0] != b'\n' && bytes.len() < self.limits.max_line_bytes,
                "falkor_resp_invalid"
            );
            bytes.push(byte[0]);
        }
    }
    fn number(bytes: &[u8]) -> Result<i64> {
        let text =
            std::str::from_utf8(bytes).map_err(|_| anyhow::anyhow!("falkor_resp_invalid"))?;
        let n = text
            .parse::<i64>()
            .map_err(|_| anyhow::anyhow!("falkor_resp_invalid"))?;
        ensure!(n.to_string() == text, "falkor_resp_invalid");
        Ok(n)
    }
    fn value(&mut self, depth: usize) -> Result<Value> {
        self.deadline.check()?;
        ensure!(
            depth <= self.limits.max_depth && self.values < self.limits.max_values,
            "falkor_resp_capacity_exceeded"
        );
        self.values += 1;
        let mut prefix = [0];
        self.exact(&mut prefix)?;
        let header = self.line()?;
        match prefix[0] {
            b'+' => Ok(Value::Simple(header)),
            b'-' => Ok(Value::Error(header)),
            b':' => Ok(Value::Integer(Self::number(&header)?)),
            b'$' => {
                let n = Self::number(&header)?;
                if n == -1 {
                    return Ok(Value::Bulk(None));
                }
                ensure!(n >= 0, "falkor_resp_invalid");
                let n = usize::try_from(n)
                    .map_err(|_| anyhow::anyhow!("falkor_resp_capacity_exceeded"))?;
                ensure!(
                    n <= self.limits.max_bulk_bytes
                        && self
                            .bytes
                            .checked_add(n)
                            .and_then(|v| v.checked_add(2))
                            .is_some_and(|v| v <= self.limits.max_bytes),
                    "falkor_resp_capacity_exceeded"
                );
                let mut bytes = vec![0; n];
                self.exact(&mut bytes)?;
                let mut end = [0; 2];
                self.exact(&mut end)?;
                ensure!(end == *b"\r\n", "falkor_resp_invalid");
                Ok(Value::Bulk(Some(bytes)))
            }
            b'*' => {
                let n = Self::number(&header)?;
                if n == -1 {
                    return Ok(Value::Array(None));
                }
                ensure!(n >= 0, "falkor_resp_invalid");
                let n = usize::try_from(n)
                    .map_err(|_| anyhow::anyhow!("falkor_resp_capacity_exceeded"))?;
                ensure!(
                    n <= self.limits.max_values - self.values,
                    "falkor_resp_capacity_exceeded"
                );
                let mut values = Vec::with_capacity(n);
                for _ in 0..n {
                    values.push(self.value(depth + 1)?)
                }
                Ok(Value::Array(Some(values)))
            }
            _ => anyhow::bail!("falkor_resp_invalid"),
        }
    }
}
/// Reads one frame. The I/O owner must bound every underlying read itself.
pub fn read(reader: &mut impl Read, limits: &Limits, d: &Deadline) -> Result<Value> {
    d.check()?;
    limits.validate()?;
    let mut decoder = Decoder {
        reader,
        limits,
        deadline: d,
        bytes: 0,
        values: 0,
    };
    let value = decoder.value(0)?;
    d.check()?;
    Ok(value)
}
pub fn decode(bytes: &[u8], limits: &Limits, d: &Deadline) -> Result<Value> {
    d.check()?;
    limits.validate()?;
    ensure!(
        bytes.len() <= limits.max_bytes,
        "falkor_resp_capacity_exceeded"
    );
    let mut reader = Cursor::new(bytes);
    let value = read(&mut reader, limits, d)?;
    ensure!(
        reader.position() == bytes.len() as u64,
        "falkor_resp_trailing_bytes"
    );
    Ok(value)
}
