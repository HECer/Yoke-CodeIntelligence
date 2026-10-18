//! Test-only recursive validator for the closed historical comparison output schema.
//! The adjacent historical-output.schema.json defines each output position.
use serde_json::Value;

fn fail(path: &str, message: impl std::fmt::Display) -> String {
    format!("{path}: {message}")
}
fn child_path(path: &str, key: &str) -> String {
    format!("{path}/{}", key.replace('~', "~0").replace('/', "~1"))
}
fn validate_at(value: &Value, schema: &Value, root: &Value, path: &str) -> Result<(), String> {
    for keyword in schema
        .as_object()
        .ok_or_else(|| fail(path, "schema position is not an object"))?
        .keys()
    {
        if ![
            "$ref",
            "$schema",
            "$comment",
            "$defs",
            "const",
            "enum",
            "oneOf",
            "anyOf",
            "type",
            "pattern",
            "minimum",
            "items",
            "properties",
            "required",
            "additionalProperties",
        ]
        .contains(&keyword.as_str())
        {
            return Err(fail(
                path,
                format!("unsupported oracle schema keyword {keyword}"),
            ));
        }
    }
    if let Some(reference) = schema["$ref"].as_str() {
        let pointer = reference
            .strip_prefix('#')
            .ok_or_else(|| fail(path, "nonlocal schema reference"))?;
        let target = root
            .pointer(pointer)
            .ok_or_else(|| fail(path, "unknown schema reference"))?;
        return validate_at(value, target, root, path);
    }
    if let Some(expected) = schema.get("const") {
        return (value == expected)
            .then_some(())
            .ok_or_else(|| fail(path, "wrong constant"));
    }
    if let Some(choices) = schema["enum"].as_array() {
        return choices
            .contains(value)
            .then_some(())
            .ok_or_else(|| fail(path, "outside closed enumeration"));
    }
    for (key, exact) in [("oneOf", true), ("anyOf", false)] {
        if let Some(choices) = schema[key].as_array() {
            let attempts = choices
                .iter()
                .map(|choice| validate_at(value, choice, root, path))
                .collect::<Vec<_>>();
            let matched = attempts.iter().filter(|attempt| attempt.is_ok()).count();
            return if matched == 1 || !exact && matched > 0 {
                Ok(())
            } else {
                Err(fail(
                    path,
                    format!("{key} matched {matched} variants: {attempts:?}"),
                ))
            };
        }
    }
    match schema["type"]
        .as_str()
        .ok_or_else(|| fail(path, "missing schema type"))?
    {
        "null" if value.is_null() => Ok(()),
        "boolean" if value.is_boolean() => Ok(()),
        "string" if value.is_string() => {
            if let Some(pattern) = schema["pattern"].as_str() {
                if pattern != "^[0-9a-f]{64}$" {
                    return Err(fail(path, "unsupported schema pattern"));
                }
                if !yoke::model::valid_id(value.as_str().unwrap()) {
                    return Err(fail(path, "invalid digest/identity"));
                }
            }
            Ok(())
        }
        "integer" if value.as_u64().is_some() || value.as_i64().is_some() => {
            if schema.get("minimum").is_some_and(|minimum| minimum != 0) {
                return Err(fail(path, "unsupported oracle minimum"));
            }
            if schema["minimum"] == 0 && value.as_u64().is_none() {
                return Err(fail(path, "negative unsigned value"));
            }
            Ok(())
        }
        "array" => {
            let values = value
                .as_array()
                .ok_or_else(|| fail(path, "expected array"))?;
            let item = schema
                .get("items")
                .ok_or_else(|| fail(path, "array without position schema"))?;
            for (index, child) in values.iter().enumerate() {
                validate_at(child, item, root, &child_path(path, &index.to_string()))?;
            }
            Ok(())
        }
        "object" => {
            let fields = value
                .as_object()
                .ok_or_else(|| fail(path, "expected object"))?;
            if schema["additionalProperties"] != false {
                return Err(fail(path, "object schema is not closed"));
            }
            let properties = schema["properties"]
                .as_object()
                .ok_or_else(|| fail(path, "object without position schema"))?;
            for name in schema["required"]
                .as_array()
                .ok_or_else(|| fail(path, "required field list absent"))?
            {
                let name = name
                    .as_str()
                    .ok_or_else(|| fail(path, "bad required field"))?;
                if !fields.contains_key(name) {
                    return Err(fail(&child_path(path, name), "required field absent"));
                }
            }
            for (name, child) in fields {
                let position = child_path(path, name);
                let schema = properties
                    .get(name)
                    .ok_or_else(|| fail(&position, "field not allowed at this DTO position"))?;
                validate_at(child, schema, root, &position)?;
            }
            Ok(())
        }
        _ => Err(fail(path, "wrong scalar/container type")),
    }
}

pub fn row_units(value: &Value) -> usize {
    match value {
        Value::Array(items) => items
            .iter()
            .try_fold(items.len(), |total, item| {
                total.checked_add(row_units(item))
            })
            .expect("bounded fixture row count"),
        Value::Object(fields) => fields
            .values()
            .try_fold(0usize, |total, item| total.checked_add(row_units(item)))
            .expect("bounded fixture row count"),
        _ => 0,
    }
}
fn no_sentinel(value: &Value, sentinels: &[&str], path: &str) -> Result<(), String> {
    match value {
        Value::String(text) => {
            for sentinel in sentinels {
                if text.contains(sentinel) {
                    return Err(fail(path, "evidence-only sentinel leaked"));
                }
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                no_sentinel(item, sentinels, &child_path(path, &index.to_string()))?;
            }
        }
        Value::Object(fields) => {
            for (name, item) in fields {
                no_sentinel(item, sentinels, &child_path(path, name))?;
            }
        }
        _ => {}
    }
    Ok(())
}
pub fn validate(value: &Value, sentinels: &[&str]) -> Result<(), String> {
    let schema: Value = serde_json::from_str(include_str!("historical-output.schema.json"))
        .map_err(|error| error.to_string())?;
    validate_at(value, &schema, &schema, "$")?;
    no_sentinel(value, sentinels, "$")?;
    let used = row_units(value) as u64;
    if value["rowBudget"]["used"].as_u64() != Some(used) {
        return Err(fail(
            "$/rowBudget/used",
            "not the independent recursive array-element count",
        ));
    }
    if used
        > value["rowBudget"]["limit"]
            .as_u64()
            .ok_or_else(|| fail("$/rowBudget/limit", "missing limit"))?
    {
        return Err(fail("$/rowBudget/limit", "response exceeds row cap"));
    }
    if value["rowBudget"]["limit"] != value["options"]["maxResultRows"] {
        return Err(fail(
            "$/rowBudget/limit",
            "does not equal requested options.maxResultRows",
        ));
    }
    let canonical_bytes = serde_json::to_vec(value).map_err(|error| error.to_string())?;
    let output_cap = value["options"]["maxOutputBytes"]
        .as_u64()
        .ok_or_else(|| fail("$/options/maxOutputBytes", "missing cap"))?;
    if canonical_bytes.len() as u64 > output_cap {
        return Err(fail(
            "$/options/maxOutputBytes",
            "whole canonical Value exceeds requested output cap",
        ));
    }
    Ok(())
}

pub fn assert_original_anchor(actual: &Value, original: &Value) {
    for field in [
        "snapshotId",
        "path",
        "contentHash",
        "startByte",
        "endByte",
        "startLine",
        "endLine",
        "anchor",
    ] {
        assert_eq!(
            actual[field], original[field],
            "original anchor scalar {field}"
        );
    }
    for field in ["trust", "producer"] {
        assert_eq!(
            actual.get(field),
            original.get(field),
            "absent optional scalar must stay absent"
        );
    }
    assert_eq!(actual["textOmitted"], true);
}
