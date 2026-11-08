//! Selected context metadata and output policy; no source or runtime authority.
use crate::session_steering::Definition;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::LazyLock,
};

static CONTRACT: LazyLock<Value> = LazyLock::new(|| {
    serde_json::from_str(include_str!("context_options_contract.json"))
        .expect("Built-in public context field contract")
});

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StructuredOutput {
    Flag(bool),
    Auto(()),
}

impl StructuredOutput {
    pub(crate) fn enabled(self) -> bool {
        match self {
            Self::Flag(value) => value,
            // Native Serena calls always produce typed JSON object envelopes.
            Self::Auto(()) => true,
        }
    }
}

/// Missing preserves the preceding native transport. Explicit null selects
/// inference from the native object-envelope return type instead of disappearing.
pub(crate) fn output_option<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Option<StructuredOutput>, D::Error> {
    StructuredOutput::deserialize(deserializer).map(Some)
}

fn text(value: &str) -> Result<()> {
    ensure!(
        value.len() <= 8192 && !value.contains('\0'),
        "context_description_capacity_or_text_invalid"
    );
    Ok(())
}

fn descriptions(values: &BTreeMap<String, String>) -> Result<()> {
    ensure!(
        values.len() <= 53,
        "context_tool_description_count_exceeded"
    );
    if values.is_empty() {
        return Ok(());
    }
    let tools = crate::compatibility::serena_tools();
    let known: BTreeSet<&str> = tools
        .as_array()
        .expect("Owned tool catalog")
        .iter()
        .filter_map(|t| t["name"].as_str())
        .collect();
    let mut bytes = 0usize;
    for (name, value) in values {
        ensure!(
            known.contains(name.as_str()),
            "context_tool_description_unknown_tool"
        );
        text(value)?;
        bytes = bytes
            .checked_add(name.len())
            .and_then(|n| n.checked_add(value.len()))
            .context("Context description byte overflow")?;
        ensure!(bytes <= 32768, "context_tool_description_total_capacity");
    }
    Ok(())
}

pub(crate) fn validate(definition: &Definition, context: bool) -> Result<()> {
    text(&definition.description)?;
    ensure!(
        context
            || (definition.tool_description_overrides.is_empty()
                && definition.single_project.is_none()
                && definition.structured_tool_output.is_none()),
        "context_only_definition_field"
    );
    descriptions(&definition.tool_description_overrides)
}

pub(crate) fn yaml_field(
    key: &str,
    value: &Value,
    definition: bool,
    context: bool,
) -> Result<Option<(String, Value)>> {
    if !matches!(
        key,
        "description" | "tool_description_overrides" | "single_project" | "structured_tool_output"
    ) {
        return Ok(None);
    }
    let field = &CONTRACT["fields"][key];
    ensure!(
        definition && (context || field["contextOnly"] == false),
        "session_config_context_field_scope_invalid"
    );
    let normalized = match field["kind"]
        .as_str()
        .context("Built-in context field kind")?
    {
        "metadata_text" => {
            let value = if value.is_null() {
                ""
            } else {
                value.as_str().context("Context description must be text")?
            };
            text(value)?;
            json!(value)
        }
        "tool_descriptions" => {
            let values: BTreeMap<String, String> = serde_json::from_value(value.clone())
                .context("Context tool descriptions must map names to text")?;
            descriptions(&values)?;
            serde_json::to_value(values)?
        }
        "boolean" => {
            ensure!(
                value.is_boolean(),
                "context_single_project_boolean_required"
            );
            value.clone()
        }
        "nullable_boolean" => {
            ensure!(
                value.is_null() || value.is_boolean(),
                "context_structured_output_boolean_or_null_required"
            );
            value.clone()
        }
        _ => anyhow::bail!("Unknown built-in context field kind"),
    };
    Ok(Some((
        field["target"]
            .as_str()
            .context("Built-in context field target")?
            .into(),
        normalized,
    )))
}

pub(crate) fn decorate(tools: &mut Value, definition: &Definition) {
    for tool in tools.as_array_mut().expect("Owned Serena catalog") {
        if let Some(description) = tool["name"]
            .as_str()
            .and_then(|name| definition.tool_description_overrides.get(name))
        {
            tool["description"] = json!(description);
        }
        if let Some(output) = definition.structured_tool_output {
            if output.enabled() {
                tool["outputSchema"] = json!({"type":"object","additionalProperties":true});
            } else {
                tool.as_object_mut()
                    .expect("Owned tool metadata")
                    .remove("outputSchema");
            }
        }
    }
}

pub(crate) fn tool_response(result: Value, structured: bool) -> Result<Value> {
    let mut response = json!({"isError":result["status"]=="error","content":[{"type":"text","text":serde_json::to_string(&result)?}]});
    if structured {
        ensure!(
            result.is_object(),
            "Structured native tool result must be an object"
        );
        response["structuredContent"] = result;
        // Count both copies and their actual JSON escaping against the existing
        // compatibility inline envelope cap; no clipped successful reply.
        ensure!(
            serde_json::to_vec(&response)?.len() <= 4_000_000,
            "Structured compatibility output budget exceeded"
        );
    }
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_fields_keep_legacy_definition_bytes_and_explicit_null_roundtrips_as_auto() {
        assert_eq!(
            serde_json::to_value(Definition::default()).unwrap(),
            json!({"instructions":"","excludedTools":[]})
        );
        for value in [json!(true), json!(false), Value::Null] {
            let definition: Definition =
                serde_json::from_value(json!({"structuredToolOutput":value})).unwrap();
            assert!(definition.structured_tool_output.is_some());
            assert_eq!(
                serde_json::to_value(&definition).unwrap()["structuredToolOutput"],
                value
            );
        }
        for value in [json!(0), json!("auto"), json!([]), json!({})] {
            assert!(
                serde_json::from_value::<Definition>(json!({"structuredToolOutput":value}))
                    .is_err()
            );
        }
    }

    #[test]
    fn yaml_fields_enforce_scope_types_known_tools_and_complete_metadata_budgets() {
        assert!(
            yaml_field("single_project", &json!(true), true, true)
                .unwrap()
                .is_some()
        );
        assert!(
            yaml_field("structured_tool_output", &Value::Null, true, true)
                .unwrap()
                .is_some()
        );
        assert_eq!(
            yaml_field("description", &Value::Null, true, false)
                .unwrap()
                .unwrap()
                .1,
            json!("")
        );
        for (key, value, definition, context) in [
            ("single_project", json!(false), true, false),
            ("single_project", json!(true), false, false),
            ("single_project", Value::Null, true, true),
            ("structured_tool_output", json!(1), true, true),
            ("description", json!("x".repeat(8193)), true, true),
            ("description", json!("bad\0metadata"), true, true),
            (
                "tool_description_overrides",
                json!({"unknown":"text"}),
                true,
                true,
            ),
            (
                "tool_description_overrides",
                json!({"read_file":true}),
                true,
                true,
            ),
        ] {
            assert!(yaml_field(key, &value, definition, context).is_err());
        }
        let tools = crate::compatibility::serena_tools();
        let too_large: BTreeMap<String, String> = tools
            .as_array()
            .unwrap()
            .iter()
            .take(5)
            .map(|row| (row["name"].as_str().unwrap().into(), "x".repeat(8192)))
            .collect();
        assert!(descriptions(&too_large).is_err());
        assert!(yaml_field("description", &json!("x".repeat(8192)), true, true).is_ok());
        assert!(
            yaml_field("prompt", &json!("literal"), true, true)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn context_only_host_definition_fields_cannot_apply_to_modes_or_change_runtime_authority() {
        for value in [
            json!({"singleProject":false}),
            json!({"structuredToolOutput":null}),
            json!({"toolDescriptionOverrides":{"read_file":"text"}}),
        ] {
            let definition: Definition = serde_json::from_value(value).unwrap();
            assert!(validate(&definition, true).is_ok());
            assert!(validate(&definition, false).is_err());
        }
        let definition: Definition =
            serde_json::from_value(json!({"description":"literal {{ variable }}"})).unwrap();
        assert!(validate(&definition, false).is_ok());
    }

    #[test]
    fn structured_results_preserve_text_errors_and_refuse_combined_capacity_without_truncation() {
        for result in [
            json!({"status":"ok","data":{"text":"e\u{301}\r\n\u{1f4a1}"}}),
            json!({"status":"error","error":{"message":"refused"},"data":{}}),
        ] {
            let plain = tool_response(result.clone(), false).unwrap();
            let selected = tool_response(result.clone(), true).unwrap();
            assert_eq!(selected["content"], plain["content"]);
            assert_eq!(selected["isError"], plain["isError"]);
            assert_eq!(selected["structuredContent"], result);
            assert!(plain.get("structuredContent").is_none());
        }
        assert!(
            tool_response(
                json!({"status":"ok","data":{"text":"x".repeat(2_100_000)}}),
                true
            )
            .is_err()
        );
        assert!(tool_response(json!(["not an object"]), true).is_err());
    }
}
