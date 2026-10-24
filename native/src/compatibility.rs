//! Independent adapters for pinned public tool contracts.
use crate::{capture::Deadline, project_session::Session};
use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
fn contracts() -> Value {
    serde_json::from_str(include_str!("compatibility_contracts.json"))
        .expect("Built-in public contract JSON")
}
pub fn serena_tools() -> Value {
    Value::Array(contracts()["tools"].as_array().expect("Built-in tools").iter().map(|row| json!({"name":row["name"],"description":row["description"],"inputSchema":row["inputSchema"]})).collect())
}
pub(crate) fn serena_tools_for_session(session: &Session) -> Value {
    let mut tools = serena_tools();
    if session.dashboard_enabled() {
        for tool in tools.as_array_mut().expect("Owned public tools") {
            if tool["name"] == "open_dashboard" {
                tool["description"] = if session.dashboard_management_requested() {
                    json!(
                        "Returns a private URL for the explicitly host-enabled native session management dashboard. Its owning host must install the management driver. Session controls and project memories retain independent host permissions. Configuration files, language selection, raw logs and MCP shutdown are unavailable. The client opens the browser."
                    )
                } else {
                    json!(
                        "Returns a private URL for the explicitly host-enabled read-only native session dashboard. The client opens the browser. Configuration editing, raw logs and MCP shutdown are unavailable."
                    )
                };
            }
        }
    }
    if let Some(default) = session.git_ignore_search_default() {
        for tool in tools.as_array_mut().expect("Owned public tools") {
            if tool["name"] == "search_for_pattern" {
                tool["inputSchema"]["properties"]["skip_ignored_files"]["default"] = json!(default);
            }
        }
    }
    tools
}
pub(crate) fn repl_control_tool(name: &str) -> bool {
    matches!(
        name,
        "activate_project"
            | "remove_project"
            | "query_project"
            | "serena_repl"
            | "initial_instructions"
            | "serena_info"
    )
}
fn validate(value: &Value, schema: &Value) -> Result<()> {
    if let Some(alternatives) = schema["anyOf"].as_array() {
        ensure!(
            alternatives
                .iter()
                .any(|item| validate(value, item).is_ok()),
            "Argument does not match declared union"
        );
        return Ok(());
    }
    if let Some(values) = schema["enum"].as_array() {
        ensure!(values.contains(value), "Argument outside declared enum");
    }
    if let Some(kind) = schema["type"].as_str() {
        ensure!(
            match kind {
                "string" => value.is_string(),
                "integer" => value.as_i64().is_some(),
                "number" => value.is_number(),
                "boolean" => value.is_boolean(),
                "null" => value.is_null(),
                "array" => value.is_array(),
                "object" => value.is_object(),
                _ => false,
            },
            "Argument has wrong declared type"
        );
        if kind == "array"
            && let Some(items) = schema.get("items")
        {
            for item in value.as_array().context("Array required")? {
                validate(item, items)?;
            }
        }
    }
    Ok(())
}
pub(crate) fn arguments(args: &Value, schema: &Value) -> Result<Value> {
    ensure!(
        serde_json::to_vec(args)?.len() <= 4_000_000,
        "Compatibility input budget exceeded"
    );
    let object = args.as_object().context("Tool arguments must be object")?;
    let properties = schema["properties"]
        .as_object()
        .context("Contract properties missing")?;
    ensure!(
        object.keys().all(|key| properties.contains_key(key)),
        "Unknown public tool argument"
    );
    for required in schema["required"]
        .as_array()
        .context("Contract required fields missing")?
    {
        ensure!(
            object.contains_key(required.as_str().context("Contract field invalid")?),
            "Required public tool argument missing"
        );
    }
    let mut normalized = args.clone();
    for (key, property) in properties {
        if let Some(value) = object.get(key) {
            validate(value, property).with_context(|| format!("Invalid argument {key}"))?;
        } else if let Some(default) = property.get("default") {
            normalized[key] = default.clone();
        }
    }
    Ok(normalized)
}
fn session_arguments(
    session: &Session,
    args: &Value,
    schema: &Value,
    method: &str,
) -> Result<Value> {
    let mut normalized = arguments(args, schema)?;
    if method == "fs.search_for_pattern"
        && session.has_git_ignore_selection()
        && args.get("skip_ignored_files").is_none()
    {
        normalized
            .as_object_mut()
            .expect("Validated public arguments")
            .remove("skip_ignored_files");
    }
    Ok(normalized)
}
fn owned_session(session: &mut Session, args: &Value, deadline: &Deadline) -> Result<()> {
    let configuration = session.invoke("configuration", &json!({}), deadline)?;
    ensure!(
        args["session_id"] == configuration["sessionId"],
        "Session ID must match the host-owned session"
    );
    Ok(())
}
/// Pure native adapter. Python and shell execution require a separately host-owned runtime.
pub fn invoke_serena(
    session: &mut Session,
    name: &str,
    args: &Value,
    deadline: &Deadline,
) -> Result<Value> {
    deadline.check()?;
    session.require_serena_tool(name, deadline)?;
    let inventory = contracts();
    let row = inventory["tools"]
        .as_array()
        .context("Contract inventory missing")?
        .iter()
        .find(|row| row["name"] == name)
        .context("Unknown public Serena tool")?;
    let args = session_arguments(
        session,
        args,
        &row["inputSchema"],
        row["facadeMethod"].as_str().unwrap_or(""),
    )?;
    match name {
        "activate_project" => {
            owned_session(session, &args, deadline)?;
            session.invoke("activate", &json!({"project":args["project"]}), deadline)
        }
        "remove_project" => session.invoke(
            "removeProject",
            &json!({"project":args["project_name"]}),
            deadline,
        ),
        "initial_instructions" => session.invoke("initialInstructions", &json!({}), deadline),
        "query_project" => {
            let nested = crate::json::parse(
                args["tool_params_json"]
                    .as_str()
                    .context("Tool parameters required")?
                    .as_bytes(),
                4_000_000,
            )?;
            let tool = args["tool_name"].as_str().context("Tool name required")?;
            let project = args["project_name"]
                .as_str()
                .context("Registered project required")?;
            if tool.starts_with("ci_") {
                return session.invoke(
                    "queryProject",
                    &json!({"project":project,"tool":tool,"arguments":nested}),
                    deadline,
                );
            }
            let target = inventory["tools"]
                .as_array()
                .context("Contract inventory missing")?
                .iter()
                .find(|row| row["name"] == tool)
                .context("Unknown external public query")?;
            let method = target["facadeMethod"]
                .as_str()
                .context("External tool has no read-only facade")?;
            ensure!(
                matches!(
                    method,
                    "fs.read_file"
                        | "fs.list_dir"
                        | "fs.find_file"
                        | "fs.search_for_pattern"
                        | "lsp.find_symbol"
                        | "lsp.get_symbols_overview"
                        | "lsp.find_referencing_symbols"
                        | "lsp.find_implementations"
                        | "lsp.get_diagnostics_for_file"
                        | "lsp.get_diagnostics_for_symbol"
                        | "jb.find_symbol"
                        | "jb.get_symbols_overview"
                        | "jb.find_referencing_symbols"
                        | "jb.find_implementations"
                        | "jb.get_type_hierarchy"
                        | "jb.run_inspections"
                        | "jb.list_inspections"
                ),
                "External public query is not an immutable source read"
            );
            let nested = session_arguments(session, &nested, &target["inputSchema"], method)?;
            crate::agent_repl::invoke_facade(session, method, nested, Some(project), deadline)
        }
        "serena_repl" => {
            owned_session(session, &args, deadline)?;
            bail!("Persistent Python runtime must be configured explicitly by the host")
        }
        "serena_info" => {
            ensure!(
                args["topic"] == "jet_brains_debug_repl",
                "Unknown public information topic"
            );
            Ok(
                json!({"topic":"jet_brains_debug_repl","runtime":"native","executionStarted":false,"instructions":"Debugger execution requires an explicitly selected host debugger and target profile. Evaluation requires separate host authority and may execute target code. Use captured source hashes and stopped frames to interpret results; runtime observations are distinct from static graph candidates. The native Python debugger CLI and trusted agent REPL are independent implementations; this information response does not start an IDE backend.","limitations":["Proprietary IDE debugger implementation is not included.","The public debug_eval facade requires a configured native debugger bridge; tool arguments cannot select processes or executables."]}),
            )
        }
        _ => {
            let method = row["facadeMethod"]
                .as_str()
                .context("Public tool has no native adapter")?;
            // Keep signature defaults at this boundary; facade contracts independently reject unknown fields.
            crate::agent_repl::invoke_facade(session, method, args, None, deadline)
        }
    }
}
/// Use an already authorized, persistent host runtime; tool arguments never configure it.
pub fn invoke_serena_with_repl(
    session: &mut Session,
    runtime: &mut crate::agent_repl::AgentRepl,
    name: &str,
    args: &Value,
    deadline: &Deadline,
) -> Result<Value> {
    deadline.check()?;
    session.require_serena_tool(name, deadline)?;
    let inventory = contracts();
    let row = inventory["tools"]
        .as_array()
        .context("Contract inventory missing")?
        .iter()
        .find(|row| row["name"] == name)
        .context("Unknown public Serena tool")?;
    let args = session_arguments(
        session,
        args,
        &row["inputSchema"],
        row["facadeMethod"].as_str().unwrap_or(""),
    )?;
    if name == "serena_repl" {
        owned_session(session, &args, deadline)?;
        let response = runtime.execute(
            session,
            args["code"].as_str().context("REPL code required")?,
            deadline,
        )?;
        ensure!(
            response["error"].is_null(),
            "Python cell failed: {}",
            response["error"]
        );
        return Ok(response);
    }
    let method = row["facadeMethod"].as_str().unwrap_or("");
    if matches!(
        method,
        "shell.execute_shell_command" | "lsp.find_declaration" | "jb.find_declaration"
    ) {
        ensure!(
            method
                .split('.')
                .all(|part| part.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')),
            "Invalid owned facade method"
        );
        let encoded = serde_json::to_string(&serde_json::to_string(&args)?)?;
        let code = format!("s.{method}(**__import__('json').loads({encoded}))");
        let response = runtime.execute_tool(session, &code, deadline)?;
        ensure!(
            response["error"].is_null(),
            "Python facade failed: {}",
            response["error"]
        );
        return Ok(response["result"].clone());
    }
    invoke_serena(session, name, &args, deadline)
}
