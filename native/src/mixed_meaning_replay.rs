//! Compare all source evidence exactly after validating and retaining only the
//! independent execution's permitted resource observations.
use super::*;
#[cfg(test)]
use crate::process::MACOS_MEMORY_KIND;

pub(super) fn same_inputs(mut replayed: Value, saved: &Value, d: &Deadline) -> Result<bool> {
    check(d)?;
    // A source identity mismatch is already a negative comparison. It must not
    // become eligible for resource-observation normalization.
    if let (Some(current), Some(original)) = (
        replayed
            .pointer("/mixedView/pdfs")
            .and_then(Value::as_array),
        saved.pointer("/mixedView/pdfs").and_then(Value::as_array),
    ) {
        if current.len() != original.len() {
            return Ok(false);
        }
        for (current, original) in current.iter().zip(original) {
            check(d)?;
            if current["assetId"] != original["assetId"]
                || current["containerHash"] != original["containerHash"]
                || current["resourceLimits"]["memoryEnforcement"]
                    != original["resourceLimits"]["memoryEnforcement"]
            {
                return Ok(false);
            }
        }
    }
    resource_replay::retain(&mut replayed, saved, &mut || check(d))?;
    let same = replayed == *saved;
    check(d)?;
    Ok(same)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn sample(baseline: u64) -> Value {
        let ceiling = baseline + 268435456;
        json!({"units":[{"item":{"text":"e\u{301} 🦀","source":{"startByte":1,"endByte":9}}}],
            "mixedView":{"pdfs":[{"assetId":"original","containerHash":"original-container","pages":[{"contentHash":"exact","page":1}],
                "warnings":[],"resourceLimits":{"memoryBytes":ceiling,"cpuSeconds":20,
                    "memoryEnforcement":MACOS_MEMORY_KIND,"memoryBudget":{"schema":1,
                        "baselineVirtualBytes":baseline,"growthBudgetBytes":268435456,"ceilingBytes":ceiling}}}]}})
    }
    #[test]
    fn vm_observations_vary_but_saved_receipts_and_source_evidence_stay_exact() {
        let original = sample(445000000000);
        let saved = original.clone();
        assert!(same_inputs(sample(446000000000), &saved, &Deadline::new(10000)).unwrap());
        let mut tighter = sample(446000000000);
        tighter["mixedView"]["pdfs"][0]["resourceLimits"]["memoryBytes"] = json!(446000004096_u64);
        tighter["mixedView"]["pdfs"][0]["resourceLimits"]["memoryBudget"]["ceilingBytes"] =
            json!(446000004096_u64);
        assert!(same_inputs(tighter, &saved, &Deadline::new(10000)).unwrap());
        assert_eq!(original, saved);
        for path in [
            "/units/0/item/text",
            "/units/0/item/source/startByte",
            "/mixedView/pdfs/0/assetId",
            "/mixedView/pdfs/0/containerHash",
            "/mixedView/pdfs/0/pages/0/contentHash",
            "/mixedView/pdfs/0/pages/0/page",
            "/mixedView/pdfs/0/warnings",
        ] {
            let mut changed = sample(446000000000);
            *changed.pointer_mut(path).unwrap() = json!("changed");
            assert!(!same_inputs(changed, &saved, &Deadline::new(10000)).unwrap());
        }
        assert!(same_inputs(sample(446000000000), &saved, &Deadline::new(0)).is_err());
    }
    #[test]
    fn forged_unknown_missing_or_different_memory_policies_do_not_compare_equal() {
        let saved = sample(445000000000);
        for (path, value) in [
            ("/mixedView/pdfs/0/resourceLimits/cpuSeconds", json!(21)),
            ("/mixedView/pdfs/0/resourceLimits/memoryBytes", json!(1)),
            (
                "/mixedView/pdfs/0/resourceLimits/memoryBudget/schema",
                json!(2),
            ),
            (
                "/mixedView/pdfs/0/resourceLimits/memoryBudget/growthBudgetBytes",
                json!(1),
            ),
            (
                "/mixedView/pdfs/0/resourceLimits/memoryBudget/baselineVirtualBytes",
                json!(u64::MAX),
            ),
            ("/mixedView/pdfs/0/resourceLimits/memoryBudget", Value::Null),
            (
                "/mixedView/pdfs/0/resourceLimits/memoryEnforcement",
                json!("rlimit_as"),
            ),
        ] {
            let mut changed = sample(446000000000);
            *changed.pointer_mut(path).unwrap() = value;
            assert!(!same_inputs(changed.clone(), &saved, &Deadline::new(10000)).unwrap_or(false));
            assert!(!same_inputs(saved.clone(), &changed, &Deadline::new(10000)).unwrap_or(false));
        }
        let mut unknown = sample(446000000000);
        unknown["mixedView"]["pdfs"][0]["resourceLimits"]["rssBytes"] = json!(1);
        assert!(same_inputs(unknown, &saved, &Deadline::new(10000)).is_err());
        let mut legacy = saved.clone();
        legacy["mixedView"]["pdfs"][0]["resourceLimits"] =
            json!({"memoryBytes":268435456,"cpuSeconds":20,"memoryEnforcement":"rlimit_as"});
        assert!(same_inputs(legacy.clone(), &legacy, &Deadline::new(10000)).unwrap());
        assert!(!same_inputs(legacy, &saved, &Deadline::new(10000)).unwrap());
    }
}
