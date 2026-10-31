//! Explicit global tool policy; it grants no runtime, source or provider authority.
use crate::capture::Deadline;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub const VERSION: &str = "host_tool_budgets_v1";
pub const MAX_TIMEOUT_MS: u64 = 600_000;
pub const MAX_ANSWER_CHARS: u64 = 4_000_000;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct Config {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_answer_chars: Option<u64>,
}
impl Config {
    pub(crate) fn validate(&self) -> Result<()> {
        ensure!(
            self.timeout_ms
                .is_none_or(|n| (1..=MAX_TIMEOUT_MS).contains(&n)),
            "tool_budget_timeout_invalid"
        );
        ensure!(
            self.default_answer_chars
                .is_none_or(|n| n <= MAX_ANSWER_CHARS),
            "tool_budget_answer_chars_invalid"
        );
        Ok(())
    }
    fn empty(&self) -> bool {
        self.timeout_ms.is_none() && self.default_answer_chars.is_none()
    }
}
pub(crate) fn unselected(value: &Option<Config>) -> bool {
    value.as_ref().is_none_or(Config::empty)
}
fn contract() -> &'static Value {
    static CONTRACT: std::sync::LazyLock<Value> = std::sync::LazyLock::new(|| {
        serde_json::from_str(include_str!("tool_budgets_contract.json"))
            .expect("Compiled global tool contract")
    });
    &CONTRACT
}
fn field_value(key: &str, value: &Value) -> Result<Option<u64>> {
    if value.is_null() {
        return Ok(None);
    }
    match contract()["fields"][key]["target"].as_str() {
        Some("timeoutMs") => {
            let milliseconds = crate::decimal_duration::milliseconds_ceil(value, MAX_TIMEOUT_MS)
                .filter(|ms| *ms > 0)
                .context("session_config_tool_timeout_invalid")?;
            Ok(Some(milliseconds))
        }
        Some("defaultAnswerChars") => Ok(Some(
            value
                .as_u64()
                .filter(|n| *n <= MAX_ANSWER_CHARS)
                .context("session_config_default_answer_chars_invalid")?,
        )),
        _ => anyhow::bail!("Unknown compiled tool-budget field"),
    }
}
pub(crate) fn yaml_field(key: &str, value: &Value, global: bool) -> Result<bool> {
    if contract()["fields"].get(key).is_none() {
        return Ok(false);
    }
    ensure!(global, "session_config_tool_budget_global_only");
    field_value(key, value)?;
    Ok(true)
}
pub(crate) fn project(selected: &mut Option<Config>, fields: &Map<String, Value>) -> Result<()> {
    if fields.is_empty() {
        return Ok(());
    }
    let mut value = selected.unwrap_or_default();
    for (key, raw) in fields {
        let n = field_value(key, raw)?;
        match contract()["fields"][key]["target"].as_str() {
            Some("timeoutMs") => value.timeout_ms = n,
            Some("defaultAnswerChars") => value.default_answer_chars = n,
            _ => anyhow::bail!("Unknown compiled tool-budget target"),
        }
    }
    value.validate()?;
    *selected = (!value.empty()).then_some(value);
    Ok(())
}
/// The selected timeout can replace a transport's own default, but never its
/// original host request ceiling. Nested new deadlines inherit the same scope.
pub(crate) fn with_deadline<T>(
    policy: Option<&Config>,
    parent: &Deadline,
    run: impl FnOnce(&Deadline) -> Result<T>,
) -> Result<T> {
    if let Some(policy) = policy {
        policy.validate()?;
    }
    let Some(ms) = policy.and_then(|p| p.timeout_ms) else {
        return run(parent);
    };
    let bounded = parent.intersection(&Deadline::new(ms))?;
    Deadline::run_bounded(
        &bounded,
        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        || run(&bounded),
    )
}
pub(crate) fn answer_limit(
    policy: Option<&Config>,
    method: &str,
    args: &Value,
) -> Result<Option<u64>> {
    let explicit = args
        .get("max_answer_chars")
        .map(|v| {
            v.as_i64()
                .filter(|n| (-1..=MAX_ANSWER_CHARS as i64).contains(n))
                .context("Invalid max_answer_chars bound")
        })
        .transpose()?;
    if let Some(n) = explicit.filter(|n| *n >= 0) {
        return Ok(Some(n as u64));
    }
    let declared = contract()["answerFacades"]
        .as_array()
        .expect("Compiled answer facade inventory")
        .iter()
        .any(|v| v.as_str() == Some(method));
    Ok(if declared {
        policy.and_then(|p| p.default_answer_chars)
    } else {
        None
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn answer_sentinel_and_zero_are_distinct_and_unicode_counts_remain_exact() {
        let selected = Config {
            default_answer_chars: Some(1),
            ..Config::default()
        };
        for args in [json!({}), json!({"max_answer_chars":-1})] {
            assert_eq!(
                answer_limit(Some(&selected), "fs.read_file", &args).unwrap(),
                Some(1)
            );
            assert_eq!(answer_limit(None, "fs.read_file", &args).unwrap(), None);
            assert_eq!(
                answer_limit(Some(&selected), "cfg.get_current_config", &args).unwrap(),
                None
            );
        }
        for n in [0, 1, 4000000] {
            assert_eq!(
                answer_limit(
                    Some(&selected),
                    "fs.read_file",
                    &json!({"max_answer_chars":n})
                )
                .unwrap(),
                Some(n)
            );
        }
        for v in [
            json!(true),
            json!("-1"),
            json!(-2),
            json!(4000001),
            json!(1.5),
            Value::Null,
        ] {
            assert!(
                answer_limit(
                    Some(&selected),
                    "fs.read_file",
                    &json!({"max_answer_chars":v})
                )
                .is_err()
            );
        }
        let text = json!("\u{1f4a1}e\u{301}\r\n");
        assert_eq!(serde_json::to_string(&text).unwrap().chars().count(), 9);
    }
    #[test]
    fn selected_timeout_never_extends_original_host_or_nested_new_deadlines() {
        let parent = Deadline::new(10000);
        let larger = Config {
            timeout_ms: Some(20000),
            ..Config::default()
        };
        with_deadline(Some(&larger), &parent, |bounded| {
            assert!(bounded.same_end_as(&parent));
            assert!(Deadline::new(600000).same_end_as(&parent));
            Ok(())
        })
        .unwrap();
        let smaller = Config {
            timeout_ms: Some(1000),
            ..Config::default()
        };
        with_deadline(Some(&smaller), &parent, |bounded| {
            assert!(bounded.remaining() <= std::time::Duration::from_millis(1000));
            assert!(Deadline::new(600000).same_end_as(bounded));
            Ok(())
        })
        .unwrap();
        assert!(Deadline::new(600000).remaining() > std::time::Duration::from_secs(100));
    }
    #[test]
    fn inherited_cancellation_and_expiry_refuse_success_and_restore_scope() {
        let parent = Deadline::new(10000);
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let policy = Config {
            timeout_ms: Some(10000),
            ..Config::default()
        };
        let result = Deadline::run_bounded(&parent, cancel.clone(), || {
            with_deadline(Some(&policy), &Deadline::new(600000), |bounded| {
                cancel.store(true, std::sync::atomic::Ordering::SeqCst);
                assert!(bounded.check().is_err());
                Ok(())
            })
        });
        assert!(result.is_err());
        assert!(parent.check().is_ok());
        let one = Config {
            timeout_ms: Some(100),
            ..Config::default()
        };
        assert!(
            with_deadline(Some(&one), &parent, |_| {
                std::thread::sleep(std::time::Duration::from_millis(150));
                Ok(())
            })
            .is_err()
        );
        assert!(parent.check().is_ok());
    }
    #[test]
    fn cleanup_uncertainty_remains_typed_after_inherited_cancellation() {
        let parent = Deadline::new(10000);
        let policy = Config {
            timeout_ms: Some(10000),
            ..Config::default()
        };
        let flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let result: Result<()> = Deadline::run_bounded(&parent, flag.clone(), || {
            with_deadline(Some(&policy), &Deadline::new(600000), |_| {
                flag.store(true, std::sync::atomic::Ordering::SeqCst);
                Err(crate::process::CleanupUnproven::new("owned test cleanup unproven").into())
            })
        });
        assert!(crate::process::cleanup_unproven(&result.unwrap_err()));
    }
}
