use crate::model::valid_id;
use anyhow::{Result, bail, ensure};
use serde_json::Value;
pub fn validate(tool: &str, args: &Value) -> Result<()> {
    let object = args
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("Arguments must be an object"))?;
    ensure!(
        serde_json::to_vec(args)?.len() <= 4_000_000,
        "Input budget exceeded"
    );
    let specific: &[&str] = match tool {
        "ci_status" => &["mode"],
        "ci_find" => &[
            "query",
            "kind",
            "path",
            "limit",
            "offset",
            "regex",
            "model",
            "vector",
            "languageServer",
        ],
        "ci_read" => &[
            "path",
            "entityId",
            "startByte",
            "endByte",
            "startLine",
            "endLine",
            "artifactId",
            "cursor",
            "encoding",
        ],
        "ci_trace" => &[
            "entityIds",
            "direction",
            "relations",
            "resolutions",
            "depth",
            "maxVisited",
        ],
        "ci_context" => &["query", "limit", "plan", "includeMemory"],
        "ci_change_prepare" => &[
            "edits",
            "rename",
            "replaceSymbol",
            "insertBefore",
            "insertAfter",
            "deleteSymbol",
        ],
        "ci_change_apply" => &["planId", "idempotencyKey", "mode"],
        "ci_verify" => &["planId", "profiles"],
        _ => bail!("Unknown tool: {tool}"),
    };
    for (key, value) in object {
        ensure!(
            [
                "snapshotId",
                "maxBytes",
                "tokenBudget",
                "timeoutMs",
                "taskId"
            ]
            .contains(&key.as_str())
                || specific.contains(&key.as_str()),
            "Undeclared argument: {key}"
        );
        if ["snapshotId", "entityId", "artifactId", "planId"].contains(&key.as_str()) {
            ensure!(value.as_str().is_some_and(valid_id), "Invalid ID: {key}");
        }
        if [
            "query",
            "kind",
            "path",
            "cursor",
            "taskId",
            "direction",
            "mode",
            "idempotencyKey",
            "model",
        ]
        .contains(&key.as_str())
        {
            ensure!(
                value.as_str().is_some_and(|s| s.len() <= 16000),
                "Invalid string: {key}"
            );
        }
        if matches!(key.as_str(), "regex" | "includeMemory") {
            ensure!(value.is_boolean(), "Flag must be boolean: {key}");
        }
        let range = match key.as_str() {
            "maxBytes" => Some((1024, 4_000_000)),
            "tokenBudget" => Some((1, 1_000_000)),
            "timeoutMs" => Some((10, 600000)),
            "limit" => Some((1, 200)),
            "offset" => Some((0, 100000)),
            "depth" => Some((0, 8)),
            "maxVisited" => Some((1, 1000)),
            "startByte" | "endByte" => Some((0, 4_000_000)),
            "startLine" | "endLine" => Some((1, 4_000_000)),
            _ => None,
        };
        if let Some((min, max)) = range {
            ensure!(
                value.as_u64().is_some_and(|n| n >= min && n <= max),
                "Invalid numeric range: {key}"
            );
        }
    }
    if let Some(mode) = args.get("mode") {
        let allowed = if tool == "ci_status" {
            &["status", "index", "analyze", "dataflow"][..]
        } else {
            &["isolated", "workspace"][..]
        };
        ensure!(allowed.contains(&mode.as_str().unwrap()), "Invalid mode");
    }
    if let Some(kind) = args["kind"].as_str()
        && kind != "semantic"
    {
        crate::query::require_kind(kind)?;
    }
    if tool == "ci_read" {
        if let Some(encoding) = args.get("encoding") {
            ensure!(
                args.get("artifactId").is_some()
                    && matches!(encoding.as_str(), Some("utf8" | "base64")),
                "Artifact encoding must be utf8 or base64"
            );
        }
        if args.get("artifactId").is_some() {
            ensure!(
                [
                    "path",
                    "entityId",
                    "startByte",
                    "endByte",
                    "startLine",
                    "endLine"
                ]
                .iter()
                .all(|field| args.get(field).is_none()),
                "Artifact paging cannot be mixed with source ranges"
            );
        } else {
            ensure!(
                args.get("cursor").is_none(),
                "Cursor requires artifact paging"
            );
        }
    }
    if tool == "ci_find" {
        if let Some(request) = args.get("languageServer") {
            let request = request
                .as_object()
                .ok_or_else(|| anyhow::anyhow!("Language server request must be an object"))?;
            ensure!(
                request
                    .keys()
                    .all(|key| ["operation", "path", "offset"].contains(&key.as_str())),
                "Unknown language server request field"
            );
            ensure!(
                request
                    .get("operation")
                    .and_then(Value::as_str)
                    .is_some_and(|operation| {
                        [
                            "definition",
                            "declaration",
                            "references",
                            "implementation",
                            "hover",
                            "symbols",
                        ]
                        .contains(&operation)
                    }),
                "Unsupported read-only language server operation"
            );
            let path = request
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow::anyhow!("Language server source path required"))?;
            crate::paths::normalized(path)?;
            ensure!(
                request
                    .get("offset")
                    .and_then(Value::as_u64)
                    .is_some_and(|offset| offset <= 4_000_000),
                "Language server offset must be a UTF-8 byte offset within 0..4000000"
            );
            ensure!(
                [
                    "query", "kind", "model", "vector", "path", "regex", "offset", "limit"
                ]
                .iter()
                .all(|field| args.get(field).is_none()),
                "Language server navigation cannot be mixed with search fields"
            );
            return Ok(());
        }
        ensure!(
            args.get("query").is_some()
                || (args["kind"] == "semantic" && args.get("vector").is_some()),
            "Find requires a query, an explicit semantic vector, or a language server request"
        );
        if args["kind"] == "semantic" {
            ensure!(
                args["model"]
                    .as_str()
                    .is_some_and(|model| !model.is_empty() && model.len() <= 200),
                "Indexed vector model required"
            );
            ensure!(
                args.get("query").is_some() != args.get("vector").is_some(),
                "Choose a lexical query or explicit query vector"
            );
            ensure!(
                args.get("path").is_none()
                    && args.get("regex").is_none()
                    && args.get("offset").is_none(),
                "Semantic retrieval does not accept text filters or offsets"
            );
            ensure!(
                args["limit"].as_u64().is_none_or(|limit| limit <= 100),
                "Semantic result limit is 1..100"
            );
            if let Some(vector) = args.get("vector") {
                let vector = vector
                    .as_array()
                    .ok_or_else(|| anyhow::anyhow!("Query vector must be an array"))?;
                ensure!(
                    !vector.is_empty()
                        && vector.len() <= 4096
                        && vector
                            .iter()
                            .all(|number| number.as_f64().is_some_and(f64::is_finite)),
                    "Invalid query vector"
                );
            }
        } else {
            ensure!(
                args.get("model").is_none() && args.get("vector").is_none(),
                "Vector fields require semantic retrieval"
            );
        }
        ensure!(
            args["regex"] != true || args["kind"] == "text",
            "Regex requires literal text search mode"
        );
    }
    if tool == "ci_context" {
        ensure!(
            args["includeMemory"] != true || args.get("query").is_some(),
            "Knowledge inclusion needs a context query"
        );
        ensure!(
            args.get("query").is_some() != args.get("plan").is_some(),
            "Choose a context query or read-only plan"
        );
        if let Some(plan) = args.get("plan") {
            crate::query::validate_context_plan(plan)?;
        }
        if let Some(limit) = args["limit"].as_u64() {
            ensure!(limit <= 20, "Context limit must be 1..20");
        }
    }
    if let Some(direction) = args["direction"].as_str() {
        ensure!(
            ["in", "out", "both"].contains(&direction),
            "Invalid direction"
        );
    }
    for name in ["relations", "resolutions", "entityIds"] {
        if let Some(value) = args.get(name) {
            let list = value
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("{name} must be an array"))?;
            ensure!(
                list.len() <= 20
                    && list
                        .iter()
                        .all(|v| v.as_str().is_some_and(|s| s.len() <= 1000)),
                "Invalid trace filter"
            );
            if name == "entityIds" {
                ensure!(
                    !list.is_empty() && list.iter().all(|v| valid_id(v.as_str().unwrap())),
                    "Invalid entity IDs"
                );
            }
        }
    }
    for name in [
        "rename",
        "replaceSymbol",
        "insertBefore",
        "insertAfter",
        "deleteSymbol",
    ] {
        if let Some(value) = args.get(name) {
            let obj = value
                .as_object()
                .ok_or_else(|| anyhow::anyhow!("Symbol operation must be an object"))?;
            ensure!(
                obj.keys()
                    .all(|k| ["entityId", "text", "newName"].contains(&k.as_str())),
                "Unknown symbol operation field"
            );
            ensure!(
                value["entityId"].as_str().is_some_and(valid_id),
                "Invalid symbol ID"
            );
        }
    }
    Ok(())
}
