//! Invocation-local CLI transport accounting. Never persists result data.
use anyhow::{Result, bail, ensure};
use serde_json::{Value, json};
use yoke::capture::Deadline;

pub(super) struct Batch {
    results: Vec<Value>,
    result_bytes: usize,
    limit: usize,
}

fn reply(results: Vec<Value>, external_effects: bool) -> Value {
    let mut value = json!({"results":results,"sourceWritten":false,"atomicBatch":false});
    if external_effects {
        value["sourceWritten"] = Value::Null;
        value["activationExternalEffectsPossible"] = json!(true);
    }
    value
}

fn encode(value: &Value, limit: usize, deadline: &Deadline) -> Result<Vec<u8>> {
    match yoke::json::encode_bounded(value, limit, deadline) {
        Ok(bytes) => Ok(bytes),
        Err(_) => {
            deadline.check()?;
            bail!("Session reply byte budget exceeded")
        }
    }
}

impl Batch {
    pub(super) fn new(max_bytes: u64, deadline: &Deadline) -> Result<Self> {
        deadline.check()?;
        ensure!(
            (1024..=4_000_000).contains(&max_bytes),
            "Session reply budget must be 1024..4000000 bytes"
        );
        Ok(Self {
            results: Vec::new(),
            result_bytes: 0,
            // Charge the final newline as well as every byte of the JSON value.
            limit: max_bytes as usize - 1,
        })
    }

    fn remaining(&self, external_effects: bool, deadline: &Deadline) -> Result<usize> {
        let wrapper = encode(&reply(Vec::new(), external_effects), self.limit, deadline)?.len();
        let used = self.result_bytes + usize::from(!self.results.is_empty());
        let Some(remaining) = self.limit.checked_sub(wrapper + used) else {
            bail!("Session reply byte budget exceeded");
        };
        Ok(remaining)
    }

    pub(super) fn admit_next(&self, external_effects: bool, deadline: &Deadline) -> Result<()> {
        // Even the smallest possible JSON value needs one byte. Do not invoke
        // another operation when its answer cannot possibly fit.
        ensure!(
            self.remaining(external_effects, deadline)? > 0,
            "Session reply byte budget exceeded"
        );
        Ok(())
    }

    pub(super) fn push(
        &mut self,
        value: Value,
        external_effects: bool,
        deadline: &Deadline,
    ) -> Result<()> {
        let remaining = self.remaining(external_effects, deadline)?;
        let bytes = encode(&value, remaining, deadline)?;
        self.result_bytes += usize::from(!self.results.is_empty()) + bytes.len();
        self.results.push(value);
        Ok(())
    }

    pub(super) fn finish(self, external_effects: bool, deadline: &Deadline) -> Result<Vec<u8>> {
        let mut bytes = encode(&reply(self.results, external_effects), self.limit, deadline)?;
        deadline.check()?;
        bytes.push(b'\n');
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_utf8_escaped_json_wrapper_and_newline_boundary() {
        let deadline = Deadline::new(10_000);
        let row = json!({"text":format!("π🦀e\u{301}\r\n\"\\{}", "x".repeat(1200))});
        for effects in [false, true] {
            let mut expected = serde_json::to_vec(&reply(vec![row.clone()], effects)).unwrap();
            expected.push(b'\n');
            let limit = expected.len() as u64;
            let mut exact = Batch::new(limit, &deadline).unwrap();
            exact.push(row.clone(), effects, &deadline).unwrap();
            assert_eq!(exact.finish(effects, &deadline).unwrap(), expected);
            let mut short = Batch::new(limit - 1, &deadline).unwrap();
            assert!(short.push(row.clone(), effects, &deadline).is_err());
        }
    }

    #[test]
    fn array_separators_and_later_activation_metadata_are_charged() {
        let deadline = Deadline::new(10_000);
        let row = json!({"text":"x".repeat(1200)});
        let limit = serde_json::to_vec(&reply(vec![row.clone(), row.clone()], false))
            .unwrap()
            .len() as u64
            + 1;
        let mut batch = Batch::new(limit, &deadline).unwrap();
        batch.push(row.clone(), false, &deadline).unwrap();
        assert!(batch.push(row.clone(), true, &deadline).is_err());
        batch.push(row, false, &deadline).unwrap();
        assert_eq!(batch.finish(false, &deadline).unwrap().len() as u64, limit);
        let mut small = Batch::new(1024, &deadline).unwrap();
        for _ in 0..1000 {
            if small.push(Value::Null, false, &deadline).is_err() {
                let bytes = small.finish(false, &deadline).unwrap();
                assert!(bytes.len() <= 1024);
                return;
            }
        }
        panic!("Array commas were not charged");
    }

    #[test]
    fn exactly_full_successful_prefix_refuses_the_next_operation() {
        let deadline = Deadline::new(10_000);
        let row = json!("x".repeat(1200));
        let limit = serde_json::to_vec(&reply(vec![row.clone()], false))
            .unwrap()
            .len() as u64
            + 1;
        let mut batch = Batch::new(limit, &deadline).unwrap();
        batch.admit_next(false, &deadline).unwrap();
        batch.push(row, false, &deadline).unwrap();
        assert!(batch.admit_next(false, &deadline).is_err());
        assert_eq!(batch.finish(false, &deadline).unwrap().len() as u64, limit);
    }

    #[test]
    fn invalid_or_expired_budgets_refuse_before_accumulation() {
        let deadline = Deadline::new(10_000);
        for limit in [0, 1023, 4_000_001, u64::MAX] {
            assert!(Batch::new(limit, &deadline).is_err());
        }
        let expired = Deadline::new(0);
        assert!(Batch::new(1024, &expired).is_err());
        let mut batch = Batch::new(1024, &deadline).unwrap();
        assert!(batch.push(Value::Null, false, &expired).is_err());
        assert!(batch.finish(false, &expired).is_err());
    }
}
