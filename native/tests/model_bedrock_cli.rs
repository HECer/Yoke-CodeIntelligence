use std::process::Command;

#[test]
fn bedrock_parent_requires_network_trust_before_profile_or_source_state() {
    let root = tempfile::tempdir().unwrap();
    let absent = root.path().join("source-never-opened");
    let out = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(&absent)
        .args(["models-bedrock", "--config"])
        .arg(root.path().join("missing.json"))
        .arg("summarize")
        .output()
        .unwrap();
    assert!(!out.status.success());
    let error = String::from_utf8_lossy(&out.stderr);
    assert!(error.contains("--trust-network"), "{error}");
    assert!(!absent.exists());
}

#[test]
fn changed_bedrock_worker_profile_fails_before_stdin_credentials_or_source_state() {
    let root = tempfile::tempdir().unwrap();
    let absent = root.path().join("source-never-opened");
    let config = root.path().join("host.json");
    std::fs::write(&config, "{}").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(&absent)
        .args(["model-bedrock-worker", "--config"])
        .arg(config)
        .args(["--trust-network", "--config-hash"])
        .arg("a".repeat(64))
        .output()
        .unwrap();
    assert!(!out.status.success());
    let error = String::from_utf8_lossy(&out.stderr);
    assert!(
        error.contains("Bedrock provider profile changed"),
        "{error}"
    );
    assert!(!absent.exists());
}

#[test]
fn incompatible_bedrock_operation_is_denied_before_opening_source_or_reserving_cost() {
    let root = tempfile::tempdir().unwrap();
    let inputs = tempfile::tempdir().unwrap();
    let config = inputs.path().join("host.json");
    std::fs::write(&config,serde_json::json!({"endpoint":"https://bedrock-runtime.us-east-1.amazonaws.com/",
        "region":"us-east-1","remoteModelId":"amazon.titan-embed-text-v1","modelId":"fixture","modelVersion":"1",
        "credentialName":"unused-owned-fixture","dialect":{"kind":"titanV1"},"taskId":"test","currency":"USD",
        "maxCostMicros":10,"taskLimitMicros":100}).to_string()).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(root.path())
        .args(["models-bedrock", "--config"])
        .arg(config)
        .args(["--trust-network", "summarize"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let error = String::from_utf8_lossy(&out.stderr);
    assert!(
        error.contains("Bedrock dialect does not support this operation"),
        "{error}"
    );
    assert!(!root.path().join(".yoke").exists());
}

#[test]
fn embedding_batch_host_limits_are_validated_before_source_or_credential_access() {
    let root = tempfile::tempdir().unwrap();
    let inputs = tempfile::tempdir().unwrap();
    let config = inputs.path().join("host.json");
    std::fs::write(&config,serde_json::json!({"endpoint":"https://bedrock-runtime.us-east-1.amazonaws.com/",
        "region":"us-east-1","remoteModelId":"amazon.titan-embed-text-v2:0","modelId":"fixture","modelVersion":"1",
        "credentialName":"unused-owned-fixture","dialect":{"kind":"titanV2","dimensions":256,"normalize":true},
        "taskId":"test","currency":"USD","maxCostMicros":10,"taskLimitMicros":100,
        "embeddingBatch":{"maxCalls":0,"maxEntities":3,"maxTotalSourceBytes":15000}}).to_string()).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .arg("--root")
        .arg(root.path())
        .args(["models-bedrock", "--config"])
        .arg(config)
        .args(["--trust-network", "embeddings"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let error = String::from_utf8_lossy(&out.stderr);
    assert!(
        error.contains("Invalid host embedding batch limits"),
        "{error}"
    );
    assert!(!root.path().join(".yoke").exists());
}
