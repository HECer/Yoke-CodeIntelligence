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
    ensure!(
        session.tool_allowed(name),
        "Tool excluded by session steering"
    );
    let inventory = contracts();
    let row = inventory["tools"]
        .as_array()
        .context("Contract inventory missing")?
        .iter()
        .find(|row| row["name"] == name)
        .context("Unknown public Serena tool")?;
    let args = arguments(args, &row["inputSchema"])?;
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
            let nested = arguments(&nested, &target["inputSchema"])?;
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
    ensure!(
        session.tool_allowed(name),
        "Tool excluded by session steering"
    );
    let inventory = contracts();
    let row = inventory["tools"]
        .as_array()
        .context("Contract inventory missing")?
        .iter()
        .find(|row| row["name"] == name)
        .context("Unknown public Serena tool")?;
    let args = arguments(args, &row["inputSchema"])?;
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
        let response = runtime.execute(session, &code, deadline)?;
        ensure!(
            response["error"].is_null(),
            "Python facade failed: {}",
            response["error"]
        );
        return Ok(response["result"].clone());
    }
    invoke_serena(session, name, &args, deadline)
}
