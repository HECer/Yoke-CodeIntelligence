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
