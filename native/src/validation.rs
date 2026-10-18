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
            "graphViewId",
            "query",
            "kind",
            "path",
            "limit",
            "offset",
            "regex",
            "model",
            "vector",
            "languageServer",
            "namePath",
            "relativePath",
            "pathGlob",
            "excludeGlob",
            "includeBody",
            "substringMatching",
            "includeKinds",
            "excludeKinds",
            "ignoreCase",
            "multiline",
            "groupBySymbol",
            "beforeContextLines",
            "afterContextLines",
            "depth",
            "searchBody",
            "skipIgnored",
        ],
        "ci_read" => &[
            "graphViewId",
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
            "graphViewId",
            "entityIds",
            "direction",
            "relations",
            "resolutions",
            "depth",
            "maxVisited",
        ],
        "ci_context" => &[
            "graphViewId",
            "query",
            "limit",
            "plan",
            "includeMemory",
            "kind",
            "path",
            "depth",
            "maxSourceBytes",
        ],
        "ci_change_prepare" => &[
            "edits",
            "rename",
            "replaceSymbol",
            "insertBefore",
            "insertAfter",
            "deleteSymbol",
            "bulkReplace",
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
        if [
            "snapshotId",
            "graphViewId",
            "entityId",
            "artifactId",
            "planId",
        ]
        .contains(&key.as_str())
        {
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
            "namePath",
            "relativePath",
            "pathGlob",
            "excludeGlob",
        ]
        .contains(&key.as_str())
        {
            ensure!(
                value.as_str().is_some_and(|s| s.len() <= 16000),
                "Invalid string: {key}"
            );
        }
        if matches!(
            key.as_str(),
            "regex"
                | "includeMemory"
                | "includeBody"
                | "substringMatching"
                | "ignoreCase"
                | "multiline"
                | "groupBySymbol"
                | "searchBody"
                | "skipIgnored"
        ) {
            ensure!(value.is_boolean(), "Flag must be boolean: {key}");
        }
        let range = match key.as_str() {
            "maxBytes" => Some((1024, 4_000_000)),
            "tokenBudget" => Some((1, 1_000_000)),
            "timeoutMs" => Some((10, 600000)),
            "limit" => Some((1, 200)),
            "offset" => Some((0, 100000)),
            "depth" => Some((0, 8)),
            "beforeContextLines" | "afterContextLines" => Some((0, 100)),
            "maxVisited" => Some((1, 1000)),
            "maxSourceBytes" => Some((100, 256000)),
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
    if args.get("graphViewId").is_some() {
        validate_selected_view(tool, args)?;
        if tool != "ci_trace" {
            return Ok(());
        }
    } else if tool == "ci_context" {
        ensure!(
            ["kind", "path", "depth", "maxSourceBytes"]
                .iter()
                .all(|key| args.get(key).is_none()),
            "Graph context options require an explicit graphViewId"
        );
    }
    if let Some(mode) = args.get("mode") {
        let allowed = if tool == "ci_status" {
            &["status", "index", "analyze", "dataflow", "freshness"][..]
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
        ensure!(
            args.get("skipIgnored").is_none()
                || (args.get("languageServer").is_none()
                    && matches!(
                        args["kind"].as_str().unwrap_or("symbol"),
                        "symbol" | "all" | "file" | "text"
                    )),
            "skipIgnored requires captured symbol, file or text search"
        );
        for field in ["includeKinds", "excludeKinds"] {
            if let Some(values) = args.get(field) {
                ensure!(
                    values.as_array().is_some_and(|values| values.len() <= 100
                        && values
                            .iter()
                            .all(|value| value.as_str().is_some_and(|value| value.len() <= 100))),
                    "Invalid symbol-kind filters"
                );
            }
        }
        if let Some(request) = args.get("languageServer") {
            let request = request
                .as_object()
                .ok_or_else(|| anyhow::anyhow!("Language server request must be an object"))?;
            ensure!(
                request
                    .keys()
                    .all(|key| ["operation", "path", "offset", "options"].contains(&key.as_str())),
                "Unknown language server request field"
            );
            ensure!(
                request
                    .get("operation")
                    .and_then(Value::as_str)
                    .is_some_and(|operation| { crate::lsp::READ_OPERATIONS.contains(&operation) }),
                "Unsupported read-only language server operation"
            );
            let path = request
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow::anyhow!("Language server source path required"))?;
            // Syntax is checked here; captured host scope is proved by the LSP source reader.
            crate::paths::normalized_syntax(path)?;
            ensure!(
                request
                    .get("offset")
                    .and_then(Value::as_u64)
                    .is_some_and(|offset| offset <= 4_000_000),
                "Language server offset must be a UTF-8 byte offset within 0..4000000"
            );
            ensure!(
                [
                    "query",
                    "kind",
                    "model",
                    "vector",
                    "path",
                    "regex",
                    "offset",
                    "limit",
                    "namePath",
                    "relativePath",
                    "pathGlob",
                    "excludeGlob",
                    "includeBody",
                    "substringMatching",
                    "includeKinds",
                    "excludeKinds",
                    "ignoreCase",
                    "multiline",
                    "groupBySymbol",
                    "beforeContextLines",
                    "afterContextLines",
                    "depth",
                    "searchBody"
                ]
                .iter()
                .all(|field| args.get(field).is_none()),
                "Language server navigation cannot be mixed with search fields"
            );
            if let Some(options) = request.get("options") {
                crate::lsp::validate_query_options(options)?;
            }
            return Ok(());
        }
        ensure!(
            args.get("query").is_some()
                || args.get("namePath").is_some()
                || args.get("pathGlob").is_some()
                || args.get("relativePath").is_some()
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
                    && args.get("offset").is_none()
                    && [
                        "namePath",
                        "relativePath",
                        "pathGlob",
                        "excludeGlob",
                        "includeBody",
                        "substringMatching",
                        "includeKinds",
                        "excludeKinds",
                        "ignoreCase",
                        "multiline",
                        "groupBySymbol",
                        "beforeContextLines",
                        "afterContextLines",
                        "depth",
                        "searchBody"
                    ]
                    .iter()
                    .all(|key| args.get(key).is_none()),
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
        if args["kind"] == "text" {
            ensure!(
                args["query"]
                    .as_str()
                    .is_some_and(|query| !query.is_empty()),
                "Text search requires a nonempty query"
            );
            ensure!(
                [
                    "namePath",
                    "includeBody",
                    "substringMatching",
                    "includeKinds",
                    "excludeKinds",
                    "depth",
                    "searchBody"
                ]
                .iter()
                .all(|key| args.get(key).is_none()),
                "Symbol filters require symbol search"
            );
        } else {
            ensure!(
                [
                    "multiline",
                    "groupBySymbol",
                    "beforeContextLines",
                    "afterContextLines"
                ]
                .iter()
                .all(|key| args.get(key).is_none()),
                "Text options require text search"
            );
        }
        if matches!(
            args["kind"].as_str(),
            Some("document" | "concept" | "rationale" | "api_contract")
        ) {
            ensure!(
                [
                    "namePath",
                    "relativePath",
                    "pathGlob",
                    "excludeGlob",
                    "includeBody",
                    "substringMatching",
                    "includeKinds",
                    "excludeKinds",
                    "ignoreCase",
                    "depth",
                    "searchBody"
                ]
                .iter()
                .all(|key| args.get(key).is_none()),
                "Advanced filters require symbol, file or text search"
            );
        }
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
    if tool == "ci_change_prepare" && args.get("bulkReplace").is_some() {
        ensure!(
            [
                "edits",
                "rename",
                "replaceSymbol",
                "insertBefore",
                "insertAfter",
                "deleteSymbol"
            ]
            .iter()
            .all(|key| args.get(key).is_none()),
            "Bulk replacement cannot be mixed with another change mode"
        );
        ensure!(
            args["bulkReplace"].is_object(),
            "Bulk replacement must be an object"
        );
    }
    Ok(())
}

fn validate_selected_view(tool: &str, args: &Value) -> Result<()> {
    let fields: &[&str] = match tool {
        "ci_find" => &["query", "kind", "path", "limit"],
        "ci_trace" => &[
            "entityIds",
            "direction",
            "relations",
            "resolutions",
            "depth",
            "maxVisited",
        ],
        "ci_context" => &["query", "kind", "path", "limit", "depth", "maxSourceBytes"],
        "ci_read" => &[
            "entityId",
            "path",
            "startByte",
            "endByte",
            "startLine",
            "endLine",
        ],
        _ => bail!("Selected graph views support read-only query tools"),
    };
    ensure!(
        args.as_object().unwrap().keys().all(|key| {
            [
                "graphViewId",
                "snapshotId",
                "maxBytes",
                "tokenBudget",
                "timeoutMs",
                "taskId",
            ]
            .contains(&key.as_str())
                || fields.contains(&key.as_str())
        }),
        "Selected graph view cannot be mixed with canonical query options"
    );
    if matches!(tool, "ci_find" | "ci_context") {
        ensure!(
            args["query"]
                .as_str()
                .is_some_and(|s| !s.is_empty() && s.len() <= 1000),
            "Selected graph view requires a nonempty name query of at most 1000 bytes"
        );
        if let Some(kind) = args["kind"].as_str() {
            ensure!(
                !kind.is_empty()
                    && kind.len() <= 100
                    && !["symbol", "all", "text", "semantic"].contains(&kind),
                "Selected graph kind must name an exact node kind, not a canonical query mode"
            );
        }
        ensure!(
            args["path"].as_str().is_none_or(|path| path.len() <= 4096),
            "Selected graph path filter is too long"
        );
    }
    if tool == "ci_context" {
        ensure!(
            args["limit"].as_u64().is_none_or(|n| n <= 20),
            "Context limit must be 1..20"
        );
    }
    if tool == "ci_read" {
        ensure!(
            args.get("entityId").is_some() != args.get("path").is_some(),
            "Selected read requires exactly one entityId or snapshot path"
        );
        if let Some(path) = args["path"].as_str() {
            crate::paths::normalized_syntax(path)?;
        }
        ensure!(
            (args.get("startByte").is_none() && args.get("endByte").is_none())
                || (args.get("startLine").is_none() && args.get("endLine").is_none()),
            "Do not mix byte and line ranges"
        );
    }
    Ok(())
}
