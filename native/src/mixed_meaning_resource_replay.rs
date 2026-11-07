use super::{Result, Value, failure, input};

/// After fresh source extraction under current host authority, retain the
/// original graph's resource observation, not a new process's VM baseline.
/// Callers still compare every other prepared/graph byte with its saved value.
pub(super) fn retain(
    replayed: &mut Value,
    original: &Value,
    check: &mut impl FnMut() -> Result<()>,
) -> Result<()> {
    check()?;
    match (
        replayed.pointer_mut("/mixedView/pdfs"),
        original.pointer("/mixedView/pdfs"),
    ) {
        (None, None) => Ok(()),
        (Some(Value::Array(current)), Some(Value::Array(saved))) => {
            input(current.len() == saved.len())?;
            for (current, saved) in current.iter_mut().zip(saved) {
                check()?;
                input(
                    current["assetId"].as_str().is_some()
                        && current["assetId"] == saved["assetId"]
                        && current["containerHash"].as_str().is_some()
                        && current["containerHash"] == saved["containerHash"]
                        && crate::media::same_pdf_resource_policy(
                            &current["resourceLimits"],
                            &saved["resourceLimits"],
                        )?,
                )?;
                current["resourceLimits"] = saved["resourceLimits"].clone();
            }
            check()
        }
        _ => Err(failure("mixed_view_input_invalid")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn prepared(baseline: u64) -> Value {
        let ceiling = baseline + 256 * 1024 * 1024;
        json!({"kind":"mixed_meaning_inputs_v1", "units":[{"text":"Grüße 🌍\r\n"}],
            "mixedView":{"pdfs":[{"assetId":"a".repeat(64),
                "containerHash":"b".repeat(64), "path":"資料.pdf",
                "pages":[{"contentHash":"c".repeat(64)}],
                "resourceLimits":{"memoryBytes":ceiling,"cpuSeconds":20,
                    "memoryEnforcement":"macos_rlimit_as_growth_v1",
                    "memoryBudget":{"schema":1,"baselineVirtualBytes":baseline,
                        "growthBudgetBytes":256*1024*1024,"ceilingBytes":ceiling}}}]}})
    }
    #[test]
    fn retains_original_observation_but_preserves_all_other_replay_comparisons() {
        let saved = prepared(415 * 1024_u64.pow(3));
        let before = saved.clone();
        let mut replayed = prepared(415 * 1024_u64.pow(3) + 4096);
        retain(&mut replayed, &saved, &mut || Ok(())).unwrap();
        assert_eq!(replayed, saved);
        assert_eq!(saved, before);
        for pointer in [
            "/units/0/text",
            "/mixedView/pdfs/0/path",
            "/mixedView/pdfs/0/pages/0/contentHash",
        ] {
            let mut changed = prepared(415 * 1024_u64.pow(3) + 8192);
            *changed.pointer_mut(pointer).unwrap() = json!("tampered 🌍");
            retain(&mut changed, &saved, &mut || Ok(())).unwrap();
            assert_ne!(
                changed, saved,
                "The caller's full equality must still reject {pointer}"
            );
        }
    }
    #[test]
    fn rejects_foreign_or_missing_pdf_headers_policy_changes_and_interrupted_replay() {
        let saved = prepared(415 * 1024_u64.pow(3));
        for pointer in [
            "/mixedView/pdfs/0/assetId",
            "/mixedView/pdfs/0/containerHash",
            "/mixedView/pdfs/0/resourceLimits/memoryEnforcement",
        ] {
            let mut changed = saved.clone();
            *changed.pointer_mut(pointer).unwrap() = json!("foreign");
            assert!(retain(&mut changed, &saved, &mut || Ok(())).is_err());
        }
        let mut missing = json!({"mixedView":{}});
        assert!(retain(&mut missing, &saved, &mut || Ok(())).is_err());
        let mut changed = prepared(415 * 1024_u64.pow(3) + 4096);
        let before = changed.clone();
        let mut checks = 0;
        assert!(
            retain(&mut changed, &saved, &mut || {
                checks += 1;
                if checks == 2 {
                    anyhow::bail!("owned inherited cancellation");
                }
                Ok(())
            })
            .is_err()
        );
        assert_eq!(changed, before);
        let mut office = json!({"mixedView":{"documents":[]}});
        let original = office.clone();
        retain(&mut office, &original, &mut || Ok(())).unwrap();
        assert_eq!(office, original);
    }
}
