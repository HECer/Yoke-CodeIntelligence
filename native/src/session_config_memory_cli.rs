//! A pure function over resolved inert note strings, never a host/store callback.
use anyhow::{Context, Result};
use minijinja::{Environment, Error, ErrorKind, Value as JinjaValue};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
use yoke::{
    capture::Deadline,
    session_prompt::memory::{Entry, Snapshot, VERSION},
};

pub(super) struct Usage {
    snapshot: Arc<Snapshot>,
    calls: Arc<Mutex<BTreeMap<String, usize>>>,
}

pub(super) fn install(
    env: &mut Environment<'_>,
    snapshot: &Snapshot,
    d: &Deadline,
) -> Result<Usage> {
    snapshot.validate(d)?;
    let snapshot = Arc::new(snapshot.clone());
    let calls = Arc::new(Mutex::new(BTreeMap::<String, usize>::new()));
    let selected = Arc::clone(&snapshot);
    let called = Arc::clone(&calls);
    let deadline = d.clone();
    env.add_function(
        "embed_memory",
        move |argument: JinjaValue| -> std::result::Result<String, Error> {
            let refused = || {
                Error::new(
                    ErrorKind::InvalidOperation,
                    "Selected memory embedding unavailable",
                )
            };
            deadline.check().map_err(|_| refused())?;
            let name = argument.as_str().ok_or_else(refused)?;
            let entry = selected.entries.get(name).ok_or_else(refused)?;
            let mut usage = called.lock().map_err(|_| refused())?;
            if usage.values().sum::<usize>() >= 128 {
                return Err(refused());
            }
            *usage.entry(name.to_owned()).or_default() += 1;
            match entry {
                Entry::Unavailable => Ok(String::new()),
                Entry::Available { content, .. } => {
                    let escaped = name
                        .replace('&', "&amp;")
                        .replace('"', "&quot;")
                        .replace('<', "&lt;")
                        .replace('>', "&gt;");
                    if content
                        .len()
                        .saturating_add(escaped.len())
                        .saturating_add(27)
                        > 8192
                    {
                        return Err(refused());
                    }
                    // Returned strings are output data, never parsed as another template.
                    Ok(format!("<memory name=\"{escaped}\">\n{content}\n</memory>"))
                }
            }
        },
    );
    Ok(Usage { snapshot, calls })
}

impl Usage {
    pub(super) fn evidence(&self) -> Result<Value> {
        let calls = self
            .calls
            .lock()
            .map_err(|_| anyhow::anyhow!("Memory usage unavailable"))?;
        let mut used = BTreeMap::new();
        let mut warnings = Vec::new();
        let receipts = self.snapshot.receipts();
        for (name, count) in calls.iter() {
            let mut receipt = receipts["entries"]
                .get(name)
                .context("Memory use binding changed")?
                .clone();
            receipt["calls"] = json!(count);
            if matches!(self.snapshot.entries[name], Entry::Unavailable) {
                warnings.push(json!({"name":name,"code":"named_memory_unavailable"}));
            }
            used.insert(name, receipt);
        }
        Ok(
            json!({"schema":VERSION,"snapshotSha256":self.snapshot.hash()?,"used":used,"warnings":warnings,"partial":!warnings.is_empty(),"contentIsAuthoredClaim":true,"sourceBound":false,"complete":false}),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_selected_memory_has_explicit_partial_warning_and_no_content() {
        let d = Deadline::new(10000);
        let snapshot = Snapshot {
            entries: BTreeMap::from([("missing".into(), Entry::Unavailable)]),
        };
        let mut env = Environment::new();
        let usage = install(&mut env, &snapshot, &d).unwrap();
        env.add_template("root", "A{{ embed_memory('missing') }}B")
            .unwrap();
        assert_eq!(env.get_template("root").unwrap().render(()).unwrap(), "AB");
        let evidence = usage.evidence().unwrap();
        assert_eq!(evidence["partial"], true);
        assert_eq!(evidence["warnings"][0]["code"], "named_memory_unavailable");
        assert_eq!(evidence["used"]["missing"]["calls"], 1);
    }
    #[test]
    fn unselected_wrong_type_and_excessive_helper_calls_refuse() {
        let d = Deadline::new(10000);
        for text in [
            "{{ embed_memory('other') }}",
            "{{ embed_memory(1) }}",
            "{% for i in range(129) %}{{ embed_memory('missing') }}{% endfor %}",
        ] {
            let snapshot = Snapshot {
                entries: BTreeMap::from([("missing".into(), Entry::Unavailable)]),
            };
            let mut env = Environment::new();
            install(&mut env, &snapshot, &d).unwrap();
            env.add_template("root", text).unwrap();
            assert!(env.get_template("root").unwrap().render(()).is_err());
        }
    }
}
