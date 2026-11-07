use serde_json::Value;

pub fn assert_report(report: &Value, ceiling_field: &str) {
    let memory = 256 * 1024 * 1024_u64;
    if cfg!(target_os = "macos") {
        let budget = &report["memoryBudget"];
        assert_eq!(budget.as_object().unwrap().len(), 4);
        assert_eq!(budget["schema"], 1);
        assert_eq!(budget["growthBudgetBytes"], memory);
        let baseline = budget["baselineVirtualBytes"].as_u64().unwrap();
        let ceiling = budget["ceilingBytes"].as_u64().unwrap();
        assert!(baseline > 0 && ceiling > baseline && ceiling - baseline <= memory);
        assert_eq!(report[ceiling_field], ceiling);
        let kind = if ceiling_field == "memoryBytes" {
            "memoryEnforcement"
        } else {
            "memoryLimitKind"
        };
        assert_eq!(report[kind], "macos_rlimit_as_growth_v1");
    } else {
        assert_eq!(report[ceiling_field], memory);
        assert!(report.get("memoryBudget").is_none());
    }
}

// Compare independent executions without treating the macOS startup VM sample
// as stable output. Every report must satisfy the actual platform contract; all
// policy and non-observation fields still compare exactly.
#[allow(dead_code)]
pub fn assert_same_policy(actual: &Value, expected: &Value, ceiling_field: &str) {
    assert_report(actual, ceiling_field);
    assert_report(expected, ceiling_field);
    let mut compared = actual.clone();
    if cfg!(target_os = "macos") {
        compared[ceiling_field] = expected[ceiling_field].clone();
        for field in ["baselineVirtualBytes", "ceilingBytes"] {
            compared["memoryBudget"][field] = expected["memoryBudget"][field].clone();
        }
    }
    assert_eq!(compared, *expected);
}
