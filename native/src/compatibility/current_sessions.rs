//! Current public spellings normalize to the unchanged host-owned Session check.
use anyhow::{Context, Result, ensure};
use serde_json::Value;

pub(super) fn schemas() -> Value {
    let mut value: Value =
        serde_json::from_str(include_str!("../compatibility_current_sessions.json"))
            .expect("Built-in current Session parameter contracts");
    value["tools"].take()
}

pub(super) fn normalize(name: &str, args: &Value, schema: &Value) -> Result<Value> {
    if !matches!(name, "activate_project" | "serena_repl") {
        return super::arguments(args, schema);
    }
    // Check the original request before allocating or shortening any key. The
    // original validator also retains its canonical-input capacity afterwards.
    ensure!(
        serde_json::to_vec(args)?.len() <= 4_000_000,
        "Compatibility input budget exceeded"
    );
    let object = args.as_object().context("Tool arguments must be object")?;
    ensure!(
        object.contains_key("session") != object.contains_key("session_id"),
        "Exactly one of session or session_id is required"
    );
    if let Some(value) = object.get("session") {
        let mut canonical = object.clone();
        canonical.remove("session");
        canonical.insert("session_id".into(), value.clone());
        super::arguments(&Value::Object(canonical), schema)
    } else {
        super::arguments(args, schema)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn schema(name: &str) -> Value {
        super::super::contracts()["tools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["name"] == name)
            .unwrap()["inputSchema"]
            .clone()
    }

    #[test]
    fn current_alias_normalizes_to_the_exact_legacy_arguments_for_only_two_tools() {
        for (name, mut args) in [
            ("activate_project", json!({"project":"default"})),
            ("serena_repl", json!({"code":"'\u{1f4a1} e\u{301}'"})),
        ] {
            args["session"] = json!("owned");
            let current = normalize(name, &args, &schema(name)).unwrap();
            args.as_object_mut().unwrap().remove("session");
            args["session_id"] = json!("owned");
            let historical = super::super::arguments(&args, &schema(name)).unwrap();
            assert_eq!(current, historical);
            assert_eq!(normalize(name, &args, &schema(name)).unwrap(), historical);
        }
        assert!(
            normalize(
                "read_file",
                &json!({"relative_path":"main.rs","session":"owned"}),
                &schema("read_file")
            )
            .is_err()
        );
    }

    #[test]
    fn missing_dual_wrong_types_unicode_confusables_and_foreign_fields_refuse() {
        for (name, base) in [
            ("activate_project", json!({"project":"default"})),
            ("serena_repl", json!({"code":"42"})),
        ] {
            for extra in [
                json!({}),
                json!({"session":"owned","session_id":"owned"}),
                json!({"session":"owned","session_id":"foreign"}),
                json!({"session":null}),
                json!({"session":false}),
                json!({"session":["owned"]}),
                json!({"session":{"grant":true}}),
                json!({"\u{455}ession":"owned"}),
                json!({"session":"owned","command":"never run"}),
            ] {
                let mut args = base.clone();
                args.as_object_mut()
                    .unwrap()
                    .extend(extra.as_object().unwrap().clone());
                assert!(normalize(name, &args, &schema(name)).is_err(), "{args}");
            }
        }
    }

    #[test]
    fn original_input_and_canonical_budgets_still_apply_without_generic_union_widening() {
        let args = json!({"session":"owned","code":"x".repeat(4_000_000)});
        assert!(normalize("serena_repl", &args, &schema("serena_repl")).is_err());
        let args = json!({"session_id":"owned","code":"x".repeat(4_000_000)});
        assert!(normalize("serena_repl", &args, &schema("serena_repl")).is_err());
        for args in [Value::Null, json!([]), json!("owned"), json!(true)] {
            assert!(normalize("serena_repl", &args, &schema("serena_repl")).is_err());
        }
        let args = json!({"relative_path":"main.rs"});
        assert_eq!(
            normalize("read_file", &args, &schema("read_file")).unwrap(),
            super::super::arguments(&args, &schema("read_file")).unwrap()
        );
    }
}
