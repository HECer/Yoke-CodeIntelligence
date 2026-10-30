//! Actual guarded YAML catalogs, named rendering, replay and authority refusal.
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::PathBuf,
    process::{Command, Output, Stdio},
};
struct Fixture {
    root: tempfile::TempDir,
    context: PathBuf,
    selection: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let context = root.path().join("context.json");
        let selection = root.path().join("selection.json");
        Self {
            root,
            context,
            selection,
        }
    }
    fn file(&self, name: &str, bytes: &[u8]) -> Value {
        let path = self.root.path().join(name);
        fs::write(&path, bytes).unwrap();
        json!({"path":path,"sha256":yoke::model::hash(bytes)})
    }
    fn context(&self, value: Value) {
        fs::write(&self.context, serde_json::to_vec(&value).unwrap()).unwrap();
    }
    fn selected(&self, value: Value) {
        fs::write(&self.selection, serde_json::to_vec(&value).unwrap()).unwrap();
    }
    fn named(&self, name: &str, extra: &[&str]) -> Output {
        self.named_permission(name, true, extra)
    }
    fn named_permission(&self, name: &str, allowed: bool, extra: &[&str]) -> Output {
        let mut cmd = command();
        cmd.args(["render-prompt", "--context"])
            .arg(&self.context)
            .args(["--name", name, "--allow-configuration-read"]);
        if allowed {
            cmd.arg("--allow-template-evaluation");
        }
        cmd.args(extra).output().unwrap()
    }
    fn config(&self, op: &str, extra: &[&str]) -> Output {
        command()
            .args([op, "--selection"])
            .arg(&self.selection)
            .args([
                "--allow-configuration-read",
                "--allow-policy-projection",
                "--allow-template-evaluation",
            ])
            .args(extra)
            .output()
            .unwrap()
    }
}
fn command() -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_yoke"));
    c.args([
        "--timeout",
        "30000",
        "--max-bytes",
        "2000000",
        "session-config",
    ]);
    c
}
fn value(out: Output) -> Value {
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}
fn refuses(out: Output, reason: &str) {
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains(reason),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
fn worker(input: &[u8]) -> Output {
    let mut child = command()
        .arg("render-worker")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let _ = child.stdin.take().unwrap().write_all(input);
    child.wait_with_output().unwrap()
}
#[test]
fn catalog_named_inheritance_macros_includes_and_raw_unicode_bom_crlf_are_bound() {
    let f = Fixture::new();
    let raw="\u{feff}prompts:\r\n  base: 'Start {% block body %}{% endblock %} End'\r\n  part: '{{ subject }}'\r\n  macro: '{% macro mark(x) %}[{{ x }}]{% endmacro %}'\r\n  system_prompt: |\r\n    {% extends 'base' %}{% block body %}{% from 'macro' import mark %}{{ mark(subject) }} {% include 'part' %}{% endblock %}\r\n".as_bytes();
    let source = f.file("prompts.yml", raw);
    f.context(json!({"variables":{"subject":"e\u{301} \u{1f980}"},"catalogs":[source]}));
    let v = value(f.named("system_prompt", &[]));
    assert_eq!(
        v["rendered"],
        "Start [e\u{301} \u{1f980}] e\u{301} \u{1f980} End"
    );
    assert_eq!(v["evidence"]["schema"], "native_session_prompt_jinja_v2");
    assert_eq!(v["evidence"]["entryTemplate"], "system_prompt");
    let r = &v["promptCatalogResolution"];
    assert_eq!(r["sources"][0]["originalSha256"], yoke::model::hash(raw));
    assert_eq!(r["sources"][0]["sourceByteSpansAvailable"], false);
    assert_eq!(r["declarationCount"], 4);
    assert_eq!(fs::read(f.root.path().join("prompts.yml")).unwrap(), raw);
    assert!(!f.root.path().join(".yoke").exists());
}
#[test]
fn catalog_policy_projection_and_export_contain_only_literal_guidance() {
    let f = Fixture::new();
    let cat = f.file(
        "catalog.yaml",
        b"prompts: {system_prompt: 'Guide {{ subject }}'}\n",
    );
    let mode = f.file("mode.yml", b"prompt: \"{% include 'system_prompt' %}\"\n");
    f.selected(json!({"hostProfile":{"context":"desktop-app","steering":{}},"files":[{"path":mode["path"],"sha256":mode["sha256"],"target":{"kind":"mode","name":"owned"}}],"templateContext":{"variables":{"subject":"owned"},"catalogs":[cat]}}));
    let v = value(f.config("plan", &[]));
    assert_eq!(
        v["hostProfile"]["steering"]["modes"]["owned"]["instructions"],
        "Guide owned"
    );
    assert_eq!(v["templatesEvaluated"], true);
    assert_eq!(v["promptCatalogResolution"]["effectivePromptCount"], 1);
    let output = f.root.path().join("profile.json");
    let r = value(f.config(
        "export",
        &["--allow-output", "--output", output.to_str().unwrap()],
    ));
    let profile: Value = serde_json::from_slice(&fs::read(output).unwrap()).unwrap();
    assert_eq!(profile, v["hostProfile"]);
    assert!(profile.get("promptCatalogResolution").is_none());
    assert_eq!(r["promptCatalogResolution"], v["promptCatalogResolution"]);
}
#[test]
fn collisions_require_explicit_policy_and_preserve_every_shadowed_declaration() {
    let f = Fixture::new();
    let first = f.file("first.yml", b"prompts: {entry: first, other: retained}\n");
    let last = f.file("last.yml", b"prompts: {entry: last}\n");
    let mut c = json!({"templates":{"entry":"inline"},"catalogs":[first,last]});
    f.context(c.clone());
    refuses(
        f.named("entry", &[]),
        "session_config_prompt_name_collision",
    );
    c["collisionPolicy"] = json!("last_wins");
    f.context(c);
    let v = value(f.named("entry", &[]));
    assert_eq!(v["rendered"], "last");
    let d = v["promptCatalogResolution"]["declarations"]
        .as_array()
        .unwrap();
    assert_eq!(d.len(), 4);
    assert_eq!(d[0]["replacedByDeclaration"], 1);
    assert_eq!(d[1]["replacesDeclaration"], 0);
    assert_eq!(d[1]["replacedByDeclaration"], 3);
    assert_eq!(d[3]["replacesDeclaration"], 1);
    assert_eq!(d.iter().filter(|v| v["effective"] == true).count(), 2);
    assert_eq!(d[0]["decodedPromptSha256"], yoke::model::hash("inline"));
}
#[test]
fn shadowed_stale_pin_refuses_and_permissions_precede_catalog_access() {
    let f = Fixture::new();
    let mut first = f.file("first.yml", b"prompts: {entry: first}\n");
    let last = f.file("last.yml", b"prompts: {entry: last}\n");
    first["sha256"] = json!("0".repeat(64));
    f.context(json!({"catalogs":[first,last],"collisionPolicy":"last_wins"}));
    refuses(
        f.named_permission("entry", false, &[]),
        "session_config_host_permission_required",
    );
    refuses(
        f.named("entry", &[]),
        "session_config_catalog_source_changed",
    );
}
#[test]
fn discovery_catalog_roots_and_explicit_files_have_stable_order_and_pinned_replay() {
    let f = Fixture::new();
    let dir = f.root.path().join("catalogs");
    fs::create_dir(&dir).unwrap();
    fs::write(dir.join("\u{3c0}.yaml"), b"prompts: {second: second}\n").unwrap();
    fs::write(dir.join("a.yml"), b"prompts: {entry: '{{ subject }}'}\n").unwrap();
    fs::write(dir.join("ignored.txt"), b"not yaml").unwrap();
    let explicit = f.file("explicit.yml", b"prompts: {first: first}\n");
    f.selected(json!({"hostProfile":{"context":"desktop-app","steering":{}},"templateContext":{"variables":{"subject":"owned"}},"catalogs":[explicit],"directories":[{"path":dir,"kind":"prompt_templates"}]}));
    let v = value(f.config("discover", &[]));
    let catalogs = v["selection"]["templateContext"]["catalogs"]
        .as_array()
        .unwrap();
    assert_eq!(catalogs.len(), 3);
    assert!(catalogs[1]["path"].as_str().unwrap().ends_with("a.yml"));
    assert!(
        catalogs[2]["path"]
            .as_str()
            .unwrap()
            .ends_with("\u{3c0}.yaml")
    );
    assert_eq!(v["directories"][0]["inspectedEntries"], 3);
    f.selected(v["selection"].clone());
    assert_eq!(value(f.config("plan", &[])), v["projection"]);
    f.context(v["selection"]["templateContext"].clone());
    assert_eq!(value(f.named("entry", &[]))["rendered"], "owned");
}
#[test]
fn empty_catalog_and_empty_selected_root_are_observed_without_invented_completeness() {
    let f = Fixture::new();
    let dir = f.root.path().join("empty");
    fs::create_dir(&dir).unwrap();
    f.selected(json!({"hostProfile":{"context":"desktop-app","steering":{}},"directories":[{"path":dir,"kind":"prompt_templates"}]}));
    let v = value(f.config("discover", &[]));
    assert_eq!(v["discoveryComplete"], true);
    assert_eq!(v["complete"], false);
    assert!(v["selection"]["templateContext"].get("catalogs").is_none());
    assert_eq!(v["directories"][0]["yamlFiles"], json!([]));
    let cat = f.file("empty.yml", b"prompts: {}\n");
    f.context(json!({"catalogs":[cat]}));
    refuses(
        f.named("missing", &[]),
        "session_config_prompt_name_unavailable",
    );
}
#[test]
fn catalog_counts_and_discovery_collisions_refuse_before_any_yaml_payload() {
    let f = Fixture::new();
    let invalid = f.file("invalid.yml", b"this is not a mapping");
    let catalogs = vec![invalid.clone(); 33];
    f.context(json!({"catalogs":catalogs}));
    refuses(
        f.named("entry", &[]),
        "session_config_catalog_file_capacity",
    );
    f.selected(
        json!({"hostProfile":{"context":"desktop-app","steering":{}},"catalogs":vec![invalid;33]}),
    );
    refuses(
        f.config("discover", &[]),
        "session_config_catalog_file_capacity",
    );
    let same =
        json!({"path":f.root.path().join("missing.yml"),"target":{"kind":"mode","name":"same"}});
    f.selected(json!({"hostProfile":{"context":"desktop-app","steering":{}},"files":[same.clone(),same],"catalogs":[],"templateContext":{}}));
    refuses(
        f.config("discover", &[]),
        "session_config_discovery_target_ambiguous",
    );
}
#[test]
fn effective_and_shadowed_declaration_budgets_are_independent() {
    let f = Fixture::new();
    let mut prompts = serde_json::Map::new();
    for i in 0..256 {
        prompts.insert(format!("p{i:03}"), json!(format!("value{i}")));
    }
    let bytes = serde_json::to_vec(&json!({"prompts":prompts})).unwrap();
    let a = f.file("a.yml", &bytes);
    let b = f.file("b.yml", &bytes);
    f.context(json!({"catalogs":[a.clone(),b.clone()],"collisionPolicy":"last_wins"}));
    let v = value(f.named("p000", &[]));
    assert_eq!(v["promptCatalogResolution"]["declarationCount"], 512);
    assert_eq!(v["promptCatalogResolution"]["effectivePromptCount"], 256);
    let c = f.file("c.yml", b"prompts: {p000: excess}\n");
    f.context(json!({"catalogs":[a.clone(),b,c],"collisionPolicy":"last_wins"}));
    refuses(
        f.named("p000", &[]),
        "session_config_prompt_declaration_capacity",
    );
    let extra = f.file("extra.yml", b"prompts: {new: excess}\n");
    f.context(json!({"catalogs":[a,extra]}));
    refuses(
        f.named("p000", &[]),
        "session_config_effective_prompt_capacity",
    );
}
#[test]
fn combined_policy_and_catalog_bytes_share_the_existing_total_source_budget() {
    let f = Fixture::new();
    let mut raw = b"prompt: literal\n#".to_vec();
    raw.resize(64000, b'x');
    let mut files = Vec::new();
    for i in 0..31 {
        let file = f.file(&format!("mode{i}.yml"), &raw);
        files.push(json!({"path":file["path"],"sha256":file["sha256"],"target":{"kind":"mode","name":format!("m{i}")}}));
    }
    let catalog = f.file("catalog.yml", &vec![b'!'; 64000]);
    f.selected(json!({"hostProfile":{"context":"desktop-app","steering":{}},"files":files,"templateContext":{"catalogs":[catalog]}}));
    refuses(
        f.config("plan", &[]),
        "session_config_source_capacity_exceeded",
    );
}
#[test]
fn protocol_two_binds_named_text_and_protocol_one_retains_its_original_limits() {
    let c = json!({"templates":{"entry":"owned"}});
    let v = value(worker(
        &serde_json::to_vec(
            &json!({"schema":2,"context":c,"prompt":"owned","templateName":"entry"}),
        )
        .unwrap(),
    ));
    assert_eq!(v["entryTemplate"], "entry");
    assert_eq!(v["rendered"], "owned");
    for request in [
        json!({"schema":1,"context":c,"prompt":"owned","templateName":"entry"}),
        json!({"schema":2,"context":c,"prompt":"forged","templateName":"entry"}),
        json!({"schema":3,"context":{},"prompt":"x"}),
        json!({"schema":2,"context":{"catalogs":[]},"prompt":"x"}),
    ] {
        assert!(
            !worker(&serde_json::to_vec(&request).unwrap())
                .status
                .success()
        );
    }
    let templates: serde_json::Map<String, Value> =
        (0..33).map(|i| (format!("p{i}"), json!("x"))).collect();
    let request = json!({"schema":1,"context":{"templates":templates},"prompt":"x"});
    refuses(
        worker(&serde_json::to_vec(&request).unwrap()),
        "session_config_template_catalog_capacity",
    );
    let request =
        json!({"schema":1,"context":{"variables":{"large":"x".repeat(128001)}},"prompt":"x"});
    assert!(
        !worker(&serde_json::to_vec(&request).unwrap())
            .status
            .success()
    );
    let mut oversized = serde_json::to_vec(&json!({"schema":1,"context":{},"prompt":"x"})).unwrap();
    oversized.resize(256001, b' ');
    refuses(
        worker(&oversized),
        "session_config_template_request_invalid",
    );
}
#[test]
fn named_output_is_create_only_and_reply_and_render_failures_create_nothing() {
    let f = Fixture::new();
    let cat = f.file("cat.yml", b"prompts: {entry: '{{ subject }}'}\n");
    f.context(json!({"variables":{"subject":"owned"},"catalogs":[cat]}));
    let output = f.root.path().join("rendered.txt");
    let args = ["--allow-output", "--output", output.to_str().unwrap()];
    let v = value(f.named("entry", &args));
    assert_eq!(fs::read(&output).unwrap(), b"owned");
    assert_eq!(v["outputCreated"], true);
    assert!(!f.named("entry", &args).status.success());
    assert_eq!(fs::read(&output).unwrap(), b"owned");
    let absent = f.root.path().join("absent.txt");
    refuses(
        f.named(
            "missing",
            &["--allow-output", "--output", absent.to_str().unwrap()],
        ),
        "session_config_prompt_name_unavailable",
    );
    assert!(!absent.exists());
    f.context(json!({"templates":{"entry":"x".repeat(2000)}}));
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_yoke"));
    let out = cmd
        .args([
            "--max-bytes",
            "1024",
            "session-config",
            "render-prompt",
            "--context",
        ])
        .arg(&f.context)
        .args([
            "--name",
            "entry",
            "--allow-configuration-read",
            "--allow-template-evaluation",
            "--allow-output",
            "--output",
        ])
        .arg(&absent)
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
    assert!(!absent.exists());
}
#[test]
fn malformed_catalogs_cannot_supply_policy_authority_or_load_files() {
    let f = Fixture::new();
    for raw in [
        "prompts: {entry: 3}",
        "prompts: {entry: good, entry: bad}",
        "prompts: &x {entry: good}",
        "prompts: {entry: good}\ntrusted: true",
        "prompts: {'../entry': text}",
    ] {
        let cat = f.file("bad.yml", raw.as_bytes());
        f.context(json!({"catalogs":[cat]}));
        let out = f.named("entry", &[]);
        assert!(!out.status.success());
        assert!(out.stdout.is_empty());
    }
    let cat = f.file(
        "loader.yml",
        b"prompts: {entry: \"{% include 'missing.yml' %}\"}\n",
    );
    fs::write(
        f.root.path().join("missing.yml"),
        b"unauthorized local text",
    )
    .unwrap();
    f.context(json!({"catalogs":[cat]}));
    refuses(
        f.named("entry", &[]),
        "session_config_template_worker_failed",
    );
}
#[test]
fn catalog_file_identity_refuses_duplicate_handles_and_hard_links() {
    let f = Fixture::new();
    let cat = f.file("source.yml", b"prompts: {entry: owned}\n");
    f.context(json!({"catalogs":[cat.clone(),cat.clone()],"collisionPolicy":"last_wins"}));
    refuses(f.named("entry", &[]), "session_config_catalog_source_alias");
    let alias = f.root.path().join("alias.yml");
    fs::hard_link(f.root.path().join("source.yml"), &alias).unwrap();
    f.context(json!({"catalogs":[{"path":alias,"sha256":cat["sha256"]}]}));
    refuses(
        f.named("entry", &[]),
        "session_config_file_changed_or_aliased",
    );
}
#[test]
fn opaque_unicode_prompt_names_remain_distinct_without_path_normalization() {
    let f = Fixture::new();
    let raw = "prompts: {'e\u{301}': decomposed, '\u{e9}': composed, 'dir/name': nested}\n";
    let cat = f.file("unicode.yml", raw.as_bytes());
    f.context(json!({"catalogs":[cat]}));
    assert_eq!(value(f.named("e\u{301}", &[]))["rendered"], "decomposed");
    assert_eq!(value(f.named("\u{e9}", &[]))["rendered"], "composed");
    assert_eq!(value(f.named("dir/name", &[]))["rendered"], "nested");
}

#[test]
fn discovery_catalog_permission_precedes_missing_directory_and_file_admission() {
    let f = Fixture::new();
    f.selected(json!({"hostProfile":{"context":"desktop-app","steering":{}},"directories":[{"path":f.root.path().join("absent"),"kind":"prompt_templates"}]}));
    let out = command()
        .args(["discover", "--selection"])
        .arg(&f.selection)
        .args(["--allow-configuration-read", "--allow-policy-projection"])
        .output()
        .unwrap();
    refuses(out, "session_config_template_permission_required");
    let cat = f.file("large.yml", &vec![b'x'; 65537]);
    f.context(json!({"catalogs":[cat]}));
    refuses(
        f.named("entry", &[]),
        "session_config_file_type_or_capacity_refused",
    );
}
#[test]
#[cfg(windows)]
fn catalog_junction_parent_is_refused_without_developer_mode() {
    let f = Fixture::new();
    let real = f.root.path().join("real");
    fs::create_dir(&real).unwrap();
    let raw = b"prompts: {entry: owned}\n";
    fs::write(real.join("source.yml"), raw).unwrap();
    let junction = f.root.path().join("junction");
    let out = Command::new("node")
        .args([
            "-e",
            "require('node:fs').symlinkSync(process.argv[1],process.argv[2],'junction')",
        ])
        .arg(&real)
        .arg(&junction)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    f.context(
        json!({"catalogs":[{"path":junction.join("source.yml"),"sha256":yoke::model::hash(raw)}]}),
    );
    let out = f.named("entry", &[]);
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
    assert_eq!(fs::read(real.join("source.yml")).unwrap(), raw);
}
