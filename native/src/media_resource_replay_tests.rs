use super::*;

fn observation(baseline: u64, headroom: u64) -> Value {
    serde_json::json!({"memoryBytes":baseline+headroom,"cpuSeconds":20,
        "memoryEnforcement":"macos_rlimit_as_growth_v1",
        "memoryBudget":{"schema":1,"baselineVirtualBytes":baseline,
            "growthBudgetBytes":PDF_MEMORY,"ceilingBytes":baseline+headroom}})
}

#[test]
fn replay_accepts_distinct_valid_vm_observations_without_modifying_original_evidence() {
    let original = observation(415 * 1024_u64.pow(3), PDF_MEMORY);
    let current = observation(415 * 1024_u64.pow(3) + 4096, PDF_MEMORY / 2);
    let before = (original.clone(), current.clone());
    assert_ne!(current, original);
    assert!(same_pdf_resource_policy(&current, &original).unwrap());
    assert_eq!((original, current), before);
}

#[test]
fn replay_refuses_changed_policy_inconsistent_ceilings_and_forged_report_fields() {
    let current = observation(415 * 1024_u64.pow(3), PDF_MEMORY);
    for (pointer, value) in [
        ("/cpuSeconds", serde_json::json!(21)),
        ("/memoryBytes", serde_json::json!(1)),
        ("/memoryEnforcement", serde_json::json!("rlimit_as")),
        ("/memoryBudget/schema", serde_json::json!(2)),
        (
            "/memoryBudget/growthBudgetBytes",
            serde_json::json!(PDF_MEMORY * 2),
        ),
        ("/memoryBudget/baselineVirtualBytes", serde_json::json!(0)),
        ("/memoryBudget/ceilingBytes", serde_json::json!(u64::MAX)),
    ] {
        let mut changed = current.clone();
        *changed.pointer_mut(pointer).unwrap() = value;
        assert!(
            !same_pdf_resource_policy(&current, &changed).unwrap_or(false),
            "{pointer}"
        );
    }
    let mut unknown = current.clone();
    unknown["memoryBudget"]["rssCap"] = serde_json::json!(PDF_MEMORY);
    assert!(same_pdf_resource_policy(&current, &unknown).is_err());
    let mut identical_invalid = current.clone();
    identical_invalid["cpuSeconds"] = serde_json::json!(21);
    assert!(!same_pdf_resource_policy(&identical_invalid, &identical_invalid).unwrap());
}

#[test]
fn legacy_resource_observations_keep_exact_equality_and_closed_shapes() {
    let original = serde_json::json!({"memoryBytes":PDF_MEMORY,
        "cpuSeconds":20,"memoryEnforcement":"rlimit_as"});
    assert!(same_pdf_resource_policy(&original, &original).unwrap());
    let mut different = original.clone();
    different["memoryBytes"] = serde_json::json!(PDF_MEMORY + 1);
    assert!(!same_pdf_resource_policy(&different, &original).unwrap());
    different = original.clone();
    different["untrustedExtra"] = serde_json::json!("not policy");
    assert!(same_pdf_resource_policy(&different, &original).is_err());
}
