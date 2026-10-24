//! Actual host-only template projection, OS bounds, and immutable original input.
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::PathBuf,
    process::{Command, Output, Stdio},
};
struct Fixture {
    root: tempfile::TempDir,
    source: PathBuf,
    selection: PathBuf,
    original: Vec<u8>,
}
impl Fixture {
    fn new(prompt: &str, context: Value) -> Self {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("owned.yml");
        let original =
            format!("prompt: {}\r\n", serde_json::to_string(prompt).unwrap()).into_bytes();
        fs::write(&source, &original).unwrap();
        let selection = root.path().join("selection.json");
        fs::write(&selection,serde_json::to_vec(&json!({"hostProfile":{"context":"desktop-app","steering":{}},"files":[{"path":source,"sha256":yoke::model::hash(&original),"target":{"kind":"mode","name":"owned"}}],"templateContext":context})).unwrap()).unwrap();
        Self {
            root,
            source,
            selection,
            original,
        }
    }
    fn run(&self, operation: &str, extra: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_yoke"))
            .args([
                "--timeout",
                "10000",
                "--max-bytes",
                "128000",
                "session-config",
                operation,
                "--selection",
            ])
            .arg(&self.selection)
            .args(["--allow-configuration-read", "--allow-policy-projection"])
            .args(extra)
            .output()
            .unwrap()
    }
    fn value(&self) -> Value {
        let out = self.run("plan", &["--allow-template-evaluation"]);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let v = serde_json::from_slice(&out.stdout).unwrap();
        self.unchanged();
        v
    }
    fn unchanged(&self) {
        assert_eq!(fs::read(&self.source).unwrap(), self.original);
        assert!(!self.root.path().join(".yoke").exists());
    }
}
fn instructions(v: &Value) -> &str {
    v["hostProfile"]["steering"]["modes"]["owned"]["instructions"]
        .as_str()
        .unwrap()
}
#[test]
fn templates_render_conditions_loops_filters_macros_and_json_data_without_evaluation_authority() {
    let f = Fixture::new(
        "{% macro greet(n) %}{{ n|upper }}{% endmacro %}{% if enabled %}{% for row in rows %}{{ greet(row.name) }}{% if not loop.last %},{% endif %}{% endfor %}{% else %}off{% endif %}\n",
        json!({"variables":{"enabled":true,"rows":[{"name":"alpha"},{"name":"beta"}]}}),
    );
    let v = f.value();
    assert_eq!(instructions(&v), "ALPHA,BETA\n");
    assert_eq!(v["templatesEvaluated"], true);
    assert_eq!(v["sourceExecuted"], false);
    assert_eq!(v["storeOpened"], false);
    assert_eq!(v["complete"], false);
    assert_eq!(
        v["sources"][0]["originalSha256"],
        yoke::model::hash(&f.original)
    );
    let proof = &v["sources"][0]["promptRendering"];
    assert_eq!(
        proof["renderedPromptSha256"],
        yoke::model::hash("ALPHA,BETA\n")
    );
    assert_eq!(
        proof["hostRendererEvidence"]["memoryCeilingBytes"],
        256 * 1024 * 1024
    );
}
#[test]
fn templates_use_selected_catalog_includes_inheritance_imports_and_unicode_exactly() {
    let f = Fixture::new(
        "{% extends 'base' %}{% block body %}{% from 'macros' import show %}{% include 'fragment' %} {{ show(name) }}{% endblock %}",
        json!({"variables":{"name":"e\u{301} \u{1f980}"},"templates":{"base":"<{% block body %}{% endblock %}>","fragment":"{{ '<owned>' }}","macros":"{% macro show(n) %}[{{ n }}]{% endmacro %}"}}),
    );
    assert_eq!(instructions(&f.value()), "<<owned> [e\u{301} \u{1f980}]>");
}
#[test]
fn templates_default_literal_path_preserves_original_plan_bytes() {
    let f = Fixture::new("literal {host} <data>\n", json!({}));
    let mut selected: Value = serde_json::from_slice(&fs::read(&f.selection).unwrap()).unwrap();
    selected.as_object_mut().unwrap().remove("templateContext");
    fs::write(&f.selection, serde_json::to_vec(&selected).unwrap()).unwrap();
    let a = f.run("plan", &[]);
    let b = f.run("plan", &[]);
    assert!(a.status.success() && b.status.success());
    assert_eq!(a.stdout, b.stdout);
    let v: Value = serde_json::from_slice(&a.stdout).unwrap();
    assert_eq!(v["templatesEvaluated"], false);
    assert!(v["sources"][0].get("promptRendering").is_none());
    assert_eq!(instructions(&v), "literal {host} <data>\n");
    f.unchanged();
}
#[test]
fn templates_require_separate_host_grant_before_selected_yaml_is_opened() {
    let f = Fixture::new("{{ name }}", json!({"variables":{"name":"owned"}}));
    fs::remove_file(&f.source).unwrap();
    let out = f.run("plan", &[]);
    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr)
            .contains("session_config_template_permission_required")
    );
    assert!(out.stdout.is_empty());
}
#[test]
fn templates_cannot_mutate_yaml_policy_or_supply_host_runtime_trust() {
    let f = Fixture::new(
        "{{ policy|tojson }}",
        json!({"variables":{"policy":{"allowWrites":true,"trusted":true}}}),
    );
    let v = f.value();
    assert!(instructions(&v).contains("trusted"));
    assert!(
        v["hostProfile"]
            .get("inlineCompiler")
            .is_none_or(Value::is_null)
    );
    let original = b"activation_command: 'forbidden'\nprompt: \"{{ 'x' * 1000000000 }}\"\n";
    fs::write(&f.source, original).unwrap();
    let mut selection: Value = serde_json::from_slice(&fs::read(&f.selection).unwrap()).unwrap();
    selection["files"][0]["sha256"] = json!(yoke::model::hash(original));
    fs::write(&f.selection, serde_json::to_vec(&selection).unwrap()).unwrap();
    let out = f.run("plan", &["--allow-template-evaluation"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("session_config_setting_unsupported"));
    assert_eq!(fs::read(&f.source).unwrap(), original);
    assert!(!f.root.path().join(".yoke").exists());
}
#[test]
fn templates_missing_variables_catalogs_and_python_environment_or_filesystem_access_refuse() {
    for prompt in [
        "{{ missing }}",
        "{% include 'C:/Windows/win.ini' %}",
        "{{ env.PATH }}",
        "{{ getenv('PATH') }}",
        "{{ [].__class__.__mro__ }}",
        "{% import '../outside' as x %}{{ x.read }}",
    ] {
        let f = Fixture::new(prompt, json!({}));
        let out = f.run("plan", &["--allow-template-evaluation"]);
        assert!(!out.status.success(), "{prompt}");
        assert!(out.stdout.is_empty());
        f.unchanged();
    }
}
#[test]
fn templates_fuel_recursion_output_and_intermediate_memory_are_bounded() {
    for prompt in [
        "{% for x in range(1000000000) %}{% set y=x %}{% endfor %}",
        "{% macro x() %}{{ x() }}{% endmacro %}{{ x() }}",
        "{{ 'x' * 9000 }}",
        "{{ 'x' * 1000000000 }}",
    ] {
        let f = Fixture::new(prompt, json!({}));
        let out = f.run("plan", &["--allow-template-evaluation"]);
        assert!(!out.status.success(), "{prompt}");
        assert!(out.stdout.is_empty());
        f.unchanged();
    }
}
#[test]
fn templates_export_is_create_only_and_failed_render_never_creates_output() {
    let f = Fixture::new("Hello {{ name }}", json!({"variables":{"name":"owned"}}));
    let target = f.root.path().join("profile.json");
    let path = target.to_str().unwrap();
    let a = f.run(
        "export",
        &[
            "--allow-template-evaluation",
            "--allow-output",
            "--output",
            path,
        ],
    );
    assert!(a.status.success(), "{}", String::from_utf8_lossy(&a.stderr));
    let original = fs::read(&target).unwrap();
    let value: Value = serde_json::from_slice(&original).unwrap();
    assert_eq!(
        value["steering"]["modes"]["owned"]["instructions"],
        "Hello owned"
    );
    let b = f.run(
        "export",
        &[
            "--allow-template-evaluation",
            "--allow-output",
            "--output",
            path,
        ],
    );
    assert!(!b.status.success());
    assert_eq!(fs::read(&target).unwrap(), original);
    f.unchanged();
    let bad = Fixture::new("{{ missing }}", json!({}));
    let target = bad.root.path().join("absent.json");
    assert!(
        !bad.run(
            "export",
            &[
                "--allow-template-evaluation",
                "--allow-output",
                "--output",
                target.to_str().unwrap()
            ]
        )
        .status
        .success()
    );
    assert!(!target.exists());
    bad.unchanged();
}
#[test]
fn templates_invalid_catalog_names_unknown_schema_duplicate_json_and_stale_source_refuse() {
    for context in [
        json!({"templates":{"../outside":"x"}}),
        json!({"templates":{"C:/outside":"x"}}),
        json!({"unknown":true}),
    ] {
        let f = Fixture::new("owned", context);
        let out = f.run("plan", &["--allow-template-evaluation"]);
        assert!(!out.status.success());
        assert!(out.stdout.is_empty());
        f.unchanged();
    }
    let f = Fixture::new("{{ x }}", json!({"variables":{"x":"owned"}}));
    fs::write(&f.source, b"prompt: changed\n").unwrap();
    let out = f.run("plan", &["--allow-template-evaluation"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("session_config_source_changed"));
    assert_eq!(fs::read(&f.source).unwrap(), b"prompt: changed\n");
    let f = Fixture::new("owned", json!({}));
    let raw = fs::read_to_string(&f.selection).unwrap();
    let raw = raw.replacen(
        "\"templateContext\":{}",
        "\"templateContext\":{},\"templateContext\":{}",
        1,
    );
    fs::write(&f.selection, raw).unwrap();
    assert!(
        !f.run("plan", &["--allow-template-evaluation"])
            .status
            .success()
    );
    f.unchanged();
}
#[test]
fn templates_discovery_returns_replayable_context_and_identical_pinned_plan() {
    let f = Fixture::new("{{ name }}", json!({"variables":{"name":"owned"}}));
    let v = f.run("discover", &["--allow-template-evaluation"]);
    assert!(v.status.success(), "{}", String::from_utf8_lossy(&v.stderr));
    let discovered: Value = serde_json::from_slice(&v.stdout).unwrap();
    assert_eq!(discovered["templatesEvaluated"], true);
    assert_eq!(
        discovered["selection"]["templateContext"]["variables"]["name"],
        "owned"
    );
    fs::write(
        &f.selection,
        serde_json::to_vec(&discovered["selection"]).unwrap(),
    )
    .unwrap();
    assert_eq!(f.value(), discovered["projection"]);
    f.unchanged();
}
#[test]
fn templates_worker_protocol_rejects_duplicate_unknown_trailing_and_oversized_input() {
    for input in [
        b"{\"schema\":1,\"schema\":1}".to_vec(),
        b"{\"schema\":1,\"context\":{},\"prompt\":\"x\",\"unknown\":true}".to_vec(),
        b"{} {}".to_vec(),
        vec![b' '; 256001],
    ] {
        let mut child = Command::new(env!("CARGO_BIN_EXE_yoke"))
            .args(["session-config", "render-worker"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let _ = child.stdin.take().unwrap().write_all(&input);
        let out = child.wait_with_output().unwrap();
        assert!(!out.status.success());
        assert!(out.stdout.is_empty());
    }
}
#[test]
fn templates_worker_direct_call_self_imposes_and_observes_os_memory_limit() {
    let input = serde_json::to_vec(
        &json!({"schema":1,"context":{"variables":{"x":"owned"}},"prompt":"{{ x }}"}),
    )
    .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .args(["session-config", "render-worker"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(&input).unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["rendered"], "owned");
    assert_eq!(v["memoryCeilingBytes"], 256 * 1024 * 1024);
    assert_eq!(v["requestSha256"], yoke::model::hash(input));
}

#[test]
fn direct_template_worker_honors_the_selected_reply_budget_before_stdout() {
    let input =
        serde_json::to_vec(&json!({"schema":1,"context":{},"prompt":"x".repeat(2000)})).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .args(["--max-bytes", "1024", "session-config", "render-worker"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(&input).unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
}
