//! Source replay compares evidence exactly; macOS VM measurements describe one
//! execution and are independently validated rather than required to repeat.
use super::*;
use crate::process::{MACOS_MEMORY_KIND, MacosMemoryBudget};

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MacosLimits {
    memory_bytes: u64,
    cpu_seconds: u32,
    memory_enforcement: String,
    memory_budget: MacosMemoryBudget,
}
fn validate_observation(value: &Value) -> Result<()> {
    let limits: MacosLimits = crate::json::decode(value.clone())?;
    limits.memory_budget.validate(256 * 1024 * 1024)?;
    input(
        limits.cpu_seconds == 20
            && limits.memory_enforcement == MACOS_MEMORY_KIND
            && limits.memory_bytes == limits.memory_budget.ceiling_bytes(),
    )
}

pub(super) fn same_inputs(mut replayed: Value, saved: &Value, d: &Deadline) -> Result<bool> {
    check(d)?;
    if let (Some(current), Some(original)) = (
        replayed
            .pointer_mut("/mixedView/pdfs")
            .and_then(Value::as_array_mut),
        saved.pointer("/mixedView/pdfs").and_then(Value::as_array),
    ) {
        if current.len() != original.len() {
            return Ok(false);
        }
        for (current, original) in current.iter_mut().zip(original) {
            check(d)?;
            let now = &mut current["resourceLimits"];
            let then = &original["resourceLimits"];
            if now["memoryEnforcement"] == MACOS_MEMORY_KIND
                && then["memoryEnforcement"] == MACOS_MEMORY_KIND
            {
                validate_observation(now)?;
                validate_observation(then)?;
                // Only these execution measurements may vary. Policy schema,
                // budget, CPU cap, PDF hashes/pages/warnings and all other data
                // still compare exactly. The saved receipt is never rewritten.
                now["memoryBytes"] = then["memoryBytes"].clone();
                for field in ["baselineVirtualBytes", "ceilingBytes"] {
                    now["memoryBudget"][field] = then["memoryBudget"][field].clone();
                }
            }
        }
    }
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
            "mixedView":{"pdfs":[{"assetId":"original","pages":[{"contentHash":"exact","page":1}],
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
