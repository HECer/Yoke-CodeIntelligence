use serde_json::Value;
#[path = "memory_budget.rs"]
mod memory_budget;

pub fn assert_inputs(mut replayed: Value, mut original: Value) {
    for inputs in [&mut replayed, &mut original] {
        for pdf in inputs["mixedView"]["pdfs"].as_array_mut().unwrap() {
            let limits = &mut pdf["resourceLimits"];
            memory_budget::assert_report(limits, "memoryBytes");
            if cfg!(target_os = "macos") {
                limits.as_object_mut().unwrap().remove("memoryBytes");
                for key in ["baselineVirtualBytes", "ceilingBytes"] {
                    limits["memoryBudget"].as_object_mut().unwrap().remove(key);
                }
            }
        }
    }
    assert_eq!(replayed, original);
}
