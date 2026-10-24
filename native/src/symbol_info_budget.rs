//! Host-selected soft limits for optional information, never source/runtime authority.
use crate::{capture::Deadline, project_session::HostProfile};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeMap, time::Instant};

pub const MAX_MILLISECONDS: u64 = 600_000;

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct ProjectBudget {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_ms: Option<u64>,
}
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct HostBudgets {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub global_ms: Option<u64>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub projects: BTreeMap<String, ProjectBudget>,
}
impl HostBudgets {
    pub(crate) fn empty(&self) -> bool {
        self.global_ms.is_none()
            && self
                .projects
                .values()
                .all(|p| p.project_ms.is_none() && p.local_ms.is_none())
    }
    pub(crate) fn canonicalize(&mut self) {
        self.projects
            .retain(|_, p| p.project_ms.is_some() || p.local_ms.is_some());
    }
    pub fn select(&self, name: &str) -> Option<u64> {
        self.projects
            .get(name)
            .and_then(|p| p.local_ms.or(p.project_ms))
            .or(self.global_ms)
    }
    pub(crate) fn validate(&self, host: &HostProfile) -> Result<()> {
        ensure!(
            self.projects.len() <= 101,
            "symbol_info_budget_project_capacity"
        );
        for (name, p) in &self.projects {
            ensure!(
                !name.is_empty()
                    && name.len() <= 100
                    && name
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"_-.".contains(&b))
                    && (name == "default" || host.projects.iter().any(|p| p.name == *name)),
                "symbol_info_budget_project_not_host_registered"
            );
            ensure!(
                [p.project_ms, p.local_ms]
                    .into_iter()
                    .flatten()
                    .all(|ms| ms <= MAX_MILLISECONDS),
                "symbol_info_budget_invalid"
            );
        }
        ensure!(
            self.global_ms.is_none_or(|ms| ms <= MAX_MILLISECONDS),
            "symbol_info_budget_invalid"
        );
        Ok(())
    }
}
pub(crate) fn unselected(value: &Option<HostBudgets>) -> bool {
    value.as_ref().is_none_or(HostBudgets::empty)
}
/// YAML seconds become an explicit integral native duration. Positive values
/// round upward; a positive sub-millisecond value must never become unlimited.
pub(crate) fn milliseconds(value: &Value) -> Result<Option<u64>> {
    if value.is_null() {
        return Ok(None);
    }
    let seconds = value
        .as_f64()
        .filter(|s| s.is_finite() && *s >= 0.0 && *s <= 600.0)
        .ok_or_else(|| anyhow::anyhow!("session_config_symbol_info_budget_invalid"))?;
    Ok(Some(if seconds == 0.0 {
        0
    } else {
        (seconds * 1000.0).ceil().max(1.0) as u64
    }))
}

#[derive(Default)]
pub(crate) struct Retrieval {
    milliseconds: Option<u64>,
    started: Option<Instant>,
    retrieved: usize,
    omitted: usize,
}
impl Retrieval {
    pub(crate) fn new(milliseconds: Option<u64>) -> Self {
        Self {
            milliseconds,
            ..Self::default()
        }
    }
    pub(crate) fn permit(&mut self, deadline: &Deadline) -> Result<bool> {
        deadline.check()?;
        if let Some(ms) = self.milliseconds.filter(|ms| *ms > 0) {
            if self
                .started
                .is_some_and(|start| start.elapsed().as_millis() >= u128::from(ms))
            {
                self.omitted += 1;
                return Ok(false);
            }
            self.started.get_or_insert_with(Instant::now);
        }
        Ok(true)
    }
    pub(crate) fn received(&mut self) {
        self.retrieved += 1;
    }
    pub(crate) fn qualify(&self, value: &mut Value) {
        if let Some(ms) = self.milliseconds {
            value["symbolInfo"] = json!({"budgetMs":ms,"scope":"optional_lsp_hover_requests",
                "retrieved":self.retrieved,"omitted":self.omitted,"earlyStopped":self.omitted>0,
                "complete":self.omitted==0,"inFlightRequestsInterrupted":false,
                "hostDeadlineStillApplies":true});
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn seconds_are_bounded_and_positive_sub_milliseconds_never_disable_stopping() {
        for (input, expected) in [
            (json!(0), 0),
            (json!(-0.0), 0),
            (json!(0.00001), 1),
            (json!(1.125), 1125),
            (json!(600), 600000),
        ] {
            assert_eq!(milliseconds(&input).unwrap(), Some(expected));
        }
        assert_eq!(milliseconds(&Value::Null).unwrap(), None);
        for v in [
            json!(-1),
            json!(600.001),
            json!("0.5"),
            json!(false),
            json!([]),
            json!({}),
        ] {
            assert!(milliseconds(&v).is_err());
        }
    }
    #[test]
    fn soft_budget_stops_only_subsequent_requests_and_zero_remains_host_bounded() {
        let d = Deadline::new(10000);
        let mut budget = Retrieval::new(Some(1));
        assert!(budget.permit(&d).unwrap());
        budget.received();
        budget.started = Some(Instant::now() - std::time::Duration::from_millis(2));
        assert!(!budget.permit(&d).unwrap());
        let mut v = json!({});
        budget.qualify(&mut v);
        assert_eq!(v["symbolInfo"]["retrieved"], 1);
        assert_eq!(v["symbolInfo"]["omitted"], 1);
        let mut unlimited = Retrieval::new(Some(0));
        unlimited.started = budget.started;
        assert!(unlimited.permit(&d).unwrap());
        assert!(unlimited.permit(&Deadline::new(0)).is_err());
        let mut legacy = Retrieval::default();
        assert!(legacy.permit(&d).unwrap());
        let mut v = json!({"legacy":true});
        legacy.qualify(&mut v);
        assert_eq!(v, json!({"legacy":true}));
    }
}
