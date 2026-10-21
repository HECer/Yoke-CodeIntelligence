use yoke as test_yoke;
#[path = "support/git_impact_fixture.rs"]
mod fixture;
use fixture::{Fixture, state};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::PathBuf, process::Command};
use yoke::{
    capture::Deadline,
    git_workflows::{
        self,
        compiler_affected::{self, Options},
    },
    projects::CompilerSpec,
};
const P: &str = "src/π-😀.ts";
const SOURCE: &str = concat!(
    "// π 😀 e\u{301} original UTF-8\r\n",
    "export interface Service { save(v: string): string; }\r\n",
    "export class Base implements Service {\r\n protected brand = 'base';\r\n save(v: string) { return 'BASE_BODY' + v; }\r\n static staticSave(v: string) { return v; }\r\n}\r\n",
    "export class Derived extends Base {\r\n override save(v: string) { return 'DERIVED_BASE' + v; }\r\n forward(v: string) { return super.save(v); }\r\n static override staticSave(v: string) { return v + 'd'; }\r\n}\r\n",
    "export class Other { save(v: number) { return String(v); } }\r\n",
    "export class Implicit { save(v: string) { return v; } }\r\n",
    "export function viaBase(x: Base) { x.save('a'); x.save('b'); }\r\n",
    "export function viaService(x: Service) { return x.save('c'); }\r\n",
    "export function viaOther(x: Other) { return x.save(1); }\r\n",
    "export function indirect(x: Base) { return viaBase(x); }\r\n",
    "export function staticCall() { return Base.staticSave('s'); }\r\n"
);
fn spec() -> CompilerSpec {
    let node = std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|p| p.join(if cfg!(windows) { "node.exe" } else { "node" }))
        .find(|p| p.is_file())
        .unwrap()
        .canonicalize()
        .unwrap();
    CompilerSpec {
        node,
        typescript: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../node_modules/typescript/lib/typescript.js")
            .canonicalize()
            .unwrap(),
        trusted: true,
    }
}
fn configured(source: &str, head: &str) -> Fixture {
    let config=json!({"compilerOptions":{"strict":true,"target":"ES2022","module":"CommonJS","noEmit":true},"files":[P]}).to_string();
    Fixture::new(
        &[(P, source), ("tsconfig.json", &config)],
        &[(P, head), ("tsconfig.json", &config)],
    )
}
fn run(f: &Fixture, options: &Options) -> anyhow::Result<Value> {
    compiler_affected::derive(
        &f.engine.store,
        f.report["artifactId"].as_str().unwrap(),
        &spec(),
        options,
        &Deadline::new(60000),
    )
}
fn context(v: &Value) -> &Value {
    &v["head"]["contexts"][0]
}
fn node_id(c: &Value, name: &str, body: &str) -> String {
    let nodes: Vec<_> = c["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|n| n["name"] == name && n["declaration"]["text"].as_str().unwrap().contains(body))
        .collect();
    assert_eq!(
        nodes.len(),
        1,
        "Node oracle must identify one source declaration"
    );
    nodes[0]["id"].as_str().unwrap().into()
}
fn edges<'a>(c: &'a Value, relation: &str, from: &str, to: &str) -> Vec<&'a Value> {
    c["relations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["relation"] == relation && e["source"] == from && e["target"] == to)
        .collect()
}
#[test]
fn real_compiler_distinguishes_overrides_structural_dispatch_and_parallel_calls() {
    let f = configured(SOURCE, &SOURCE.replace("DERIVED_BASE", "DERIVED_HEAD"));
    let original = state(&f.engine);
    let old = git_workflows::affected_details::derive(
        &f.engine.store,
        f.report["artifactId"].as_str().unwrap(),
        &Default::default(),
        &Deadline::new(30000),
    )
    .unwrap()
    .canonical_bytes()
    .to_vec();
    let result = run(&f, &Default::default()).unwrap();
    let c = context(&result);
    let derived = node_id(c, "save", "DERIVED_HEAD");
    let base = node_id(c, "save", "BASE_BODY");
    let caller = node_id(c, "viaBase", "x.save('a')");
    let service = node_id(c, "viaService", "x.save('c')");
    let implicit = node_id(c, "save", "return v;");
    let other = node_id(c, "save", "String(v)");
    assert_eq!(edges(c, "overrides", &derived, &base).len(), 1);
    assert_eq!(edges(c, "calls", &caller, &base).len(), 2);
    let parallel = edges(c, "possibleDispatch", &caller, &derived);
    assert_eq!(parallel.len(), 2);
    assert_ne!(parallel[0]["id"], parallel[1]["id"]);
    assert_ne!(
        parallel[0]["proof"]["startByte"],
        parallel[1]["proof"]["startByte"]
    );
    assert_eq!(
        edges(c, "possibleDispatch", &caller, &implicit).len(),
        0,
        "Protected brand prohibits unrelated Base receiver assignment"
    );
    assert_eq!(
        edges(c, "possibleDispatch", &service, &implicit).len(),
        1,
        "Structural type compatibility is distinct from explicit implements"
    );
    assert!(
        edges(c, "possibleDispatch", &service, &other).is_empty(),
        "Same spelling with incompatible parameter type is not a target"
    );
    assert!(
        c["relations"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["proof"]["text"].as_str().unwrap().contains("super.save"))
            .all(|e| e["relation"] == "calls")
    );
    let static_derived = node_id(c, "staticSave", "+ 'd'");
    let static_base = node_id(c, "staticSave", "return v;");
    assert_eq!(
        edges(c, "overrides", &static_derived, &static_base).len(),
        1
    );
    assert!(
        c["relations"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["proof"]["text"]
                .as_str()
                .unwrap()
                .contains("Base.staticSave"))
            .all(|e| e["relation"] == "calls")
    );
    assert_eq!(result["runtimeDispatchEstablished"], false);
    assert_eq!(result["complete"], false);
    assert!(
        result["incomingProofs"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["sourceNodeId"] == caller)
    );
    assert_eq!(
        state(&f.engine),
        original,
        "Read-only query published compiler artifacts or changed sources"
    );
    assert_eq!(
        git_workflows::affected_details::derive(
            &f.engine.store,
            f.report["artifactId"].as_str().unwrap(),
            &Default::default(),
            &Deadline::new(30000)
        )
        .unwrap()
        .canonical_bytes(),
        old
    );
}
#[test]
fn aliases_generic_heritage_diamonds_and_overload_implementation_keep_distinct_proofs() {
    let source = concat!(
        "// π 😀\r\nexport interface I { run(v:string):string; }\r\ntype Alias = I;\r\ninterface J extends Alias {}\r\ninterface K extends I {}\r\ninterface L extends J,K {}\r\n",
        "class Generic<T> { protected brand!:T; run(v:T):T { return v; } }\r\nclass Concrete extends Generic<string> { override run(v:string) { return 'OLD' + v; } }\r\n",
        "class Contract implements L { run(v:string) { return v; } }\r\nfunction byGeneric(x:Generic<string>) { return x.run('g'); }\r\nfunction byContract(x:L) { return x.run('i'); }\r\n",
        "function overloaded(v:string):string;\r\nfunction overloaded(v:number):string;\r\nfunction overloaded(v:string|number) { return 'OLD' + v; }\r\nfunction invokeOverload() { return overloaded('x'); }\r\n"
    );
    let f = configured(source, &source.replace("'OLD'", "'NEW'"));
    let r = run(&f, &Default::default()).unwrap();
    let c = context(&r);
    let concrete = node_id(c, "run", "'NEW'");
    let generic = node_id(c, "run", "v:T");
    assert_eq!(edges(c, "overrides", &concrete, &generic).len(), 1);
    assert!(
        c["relations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["relation"] == "extends" && e["proof"]["text"] == "Alias")
    );
    assert!(
        c["unavailable"]
            .as_array()
            .unwrap()
            .iter()
            .any(|u| u["reason"] == "generic_candidate_instantiations_not_enumerated")
    );
    assert_eq!(
        c["relations"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["relation"] == "signatureImplementation")
            .count(),
        2
    );
    let caller = node_id(c, "invokeOverload", "overloaded('x')");
    assert!(
        r["states"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["nodeId"] == caller)
    );
}
#[test]
fn context_dependent_assignability_is_not_merged_across_projects() {
    let source = "export interface Service { run(v:string):string; }\nexport class Nullable { run(v:string):string|null { return v ? 'OLD' : null; } }\nexport function caller(x:Service) { return x.run('x'); }\n";
    let strict =
        json!({"compilerOptions":{"strict":true,"target":"ES2022","noEmit":true},"files":[P]})
            .to_string();
    let loose=json!({"compilerOptions":{"strict":false,"strictNullChecks":false,"target":"ES2022","noEmit":true},"files":[format!("../{P}")]}).to_string();
    let head = source.replace("'OLD'", "'NEW'");
    let f = Fixture::new(
        &[
            (P, source),
            ("tsconfig.json", &strict),
            ("loose/tsconfig.json", &loose),
        ],
        &[
            (P, &head),
            ("tsconfig.json", &strict),
            ("loose/tsconfig.json", &loose),
        ],
    );
    let r = run(&f, &Default::default()).unwrap();
    let contexts = r["head"]["contexts"].as_array().unwrap();
    assert_eq!(contexts.len(), 2);
    let mut counts = BTreeMap::new();
    for c in contexts {
        let caller = node_id(c, "caller", "x.run");
        let nullable = node_id(c, "run", "null;");
        counts.insert(
            c["config"].as_str().unwrap(),
            edges(c, "possibleDispatch", &caller, &nullable).len(),
        );
    }
    assert_eq!(counts["tsconfig.json"], 0);
    assert_eq!(counts["loose/tsconfig.json"], 1);
}

#[test]
fn computed_names_bind_through_the_checker_while_private_slots_never_become_overrides() {
    let source = "// π 😀\r\nclass Base { #secret() { return 'BASE_PRIVATE'; } read() { return this.#secret(); } ['go'](v:string) { return v; } }\r\nclass Derived extends Base { #secret() { return 'DERIVED_PRIVATE'; } override read() { return this.#secret(); } override ['go'](v:string) { return 'COMPUTED_BASE'+v; } }\r\nfunction byComputed(x:Base) { return x['go']('c'); }\r\n";
    let f = configured(
        source,
        &source
            .replace("COMPUTED_BASE", "COMPUTED_HEAD")
            .replace("DERIVED_PRIVATE", "DERIVED_CHANGED"),
    );
    let r = run(&f, &Default::default()).unwrap();
    let c = context(&r);
    let derived = node_id(c, "['go']", "COMPUTED_HEAD");
    let base = node_id(c, "['go']", "return v;");
    let caller = node_id(c, "byComputed", "x['go']");
    assert_eq!(edges(c, "overrides", &derived, &base).len(), 1);
    assert_eq!(edges(c, "possibleDispatch", &caller, &derived).len(), 1);
    let private: Vec<_> = c["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|n| n["name"] == "#secret")
        .map(|n| n["id"].clone())
        .collect();
    assert_eq!(private.len(), 2);
    assert!(
        c["relations"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["relation"] == "overrides" || e["relation"] == "possibleDispatch")
            .all(|e| !private.contains(&e["source"]) && !private.contains(&e["target"]))
    );
}
#[test]
fn unsupported_invocations_and_original_capture_limits_remain_visible() {
    let source = concat!(
        "class Base { static go() { return 'OLD'; } }\n",
        "class Derived extends Base { static override go() { return 'DERIVED'; } }\n",
        "function construct(C: typeof Base) { return new C(); }\n",
        "function direct() { return Base.go(); }\n",
        "function selected(C: typeof Base) { return C.go(); }\n",
        "function tag(parts: TemplateStringsArray) { return parts[0]; }\n",
        "function tagged() { return tag`literal`; }\n"
    );
    let config =
        json!({"compilerOptions":{"strict":true,"target":"ES2022","noEmit":true},"files":[P]})
            .to_string();
    let head = source.replace("'OLD'", "'NEW'");
    let f = Fixture::new(
        &[
            (P, source),
            ("tsconfig.json", &config),
            ("binary.bin", "\0owned"),
        ],
        &[
            (P, &head),
            ("tsconfig.json", &config),
            ("binary.bin", "\0owned"),
        ],
    );
    let before = state(&f.engine);
    let r = run(&f, &Default::default()).unwrap();
    let c = context(&r);
    let coverage = c["unavailable"].as_array().unwrap();
    for (reason, text) in [
        ("construct_signature_not_enumerated", "new C()"),
        ("tagged_template_signature_not_enumerated", "tag`literal`"),
        ("static_possible_dispatch_not_enumerated", "Base.go()"),
        ("static_possible_dispatch_not_enumerated", "C.go()"),
    ] {
        assert!(
            coverage
                .iter()
                .any(|u| u["reason"] == reason && u["anchor"]["text"] == text)
        );
    }
    let base = node_id(c, "go", "'NEW'");
    let direct = node_id(c, "direct", "Base.go()");
    let selected = node_id(c, "selected", "C.go()");
    assert_eq!(edges(c, "calls", &direct, &base).len(), 1);
    assert_eq!(edges(c, "calls", &selected, &base).len(), 1);
    assert!(
        c["relations"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["relation"] == "possibleDispatch")
            .all(|e| e["source"] != direct && e["source"] != selected)
    );
    let snapshot = f
        .engine
        .store
        .snapshot(f.report["headSnapshotId"].as_str().unwrap())
        .unwrap();
    assert!(!snapshot.omissions.is_empty());
    assert_eq!(
        r["head"]["captureOmissions"],
        serde_json::to_value(&snapshot.omissions).unwrap()
    );
    assert_eq!(
        r["head"]["captureConsistency"],
        snapshot.capture_consistency
    );
    assert!(
        !r["head"]["compilerLimitations"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(state(&f.engine), before);
}
#[test]
fn trust_transaction_source_and_output_bounds_fail_without_writing_state() {
    let f = configured(SOURCE, &SOURCE.replace("DERIVED_BASE", "DERIVED_HEAD"));
    let before = state(&f.engine);
    let mut untrusted = spec();
    untrusted.trusted = false;
    assert!(
        compiler_affected::derive(
            &f.engine.store,
            f.report["artifactId"].as_str().unwrap(),
            &untrusted,
            &Default::default(),
            &Deadline::new(30000)
        )
        .is_err()
    );
    let transaction = f.engine.store.db.unchecked_transaction().unwrap();
    assert!(
        run(&f, &Default::default())
            .unwrap_err()
            .to_string()
            .contains("caller transaction")
    );
    transaction.rollback().unwrap();
    assert!(
        compiler_affected::derive(
            &f.engine.store,
            "../foreign",
            &spec(),
            &Default::default(),
            &Deadline::new(30000)
        )
        .is_err()
    );
    for options in [
        Options {
            max_output_bytes: 1024,
            ..Default::default()
        },
        Options {
            max_pair_visits: 0,
            ..Default::default()
        },
        Options {
            max_nodes: 0,
            ..Default::default()
        },
    ] {
        assert!(run(&f, &options).is_err());
    }
    let depth = run(
        &f,
        &Options {
            max_depth: 0,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(depth["incomingProofs"].as_array().unwrap().is_empty());
    assert!(
        depth["states"]
            .as_array()
            .unwrap()
            .iter()
            .all(|s| s["boundary"] == true)
    );
    assert_eq!(state(&f.engine), before);
}
#[test]
fn corrupt_captured_source_cannot_be_replaced_by_current_disk() {
    let f = configured(SOURCE, &SOURCE.replace("DERIVED_BASE", "DERIVED_HEAD"));
    let before = state(&f.engine);
    let snapshot = f
        .engine
        .store
        .snapshot(f.report["headSnapshotId"].as_str().unwrap())
        .unwrap();
    let file = snapshot.files.iter().find(|p| p.path == P).unwrap();
    let path = f.engine.store.dir.join("blobs").join(&file.hash);
    let original = fs::read(&path).unwrap();
    let mut changed = original.clone();
    changed[0] ^= 1;
    fs::write(&path, &changed).unwrap();
    assert!(run(&f, &Default::default()).is_err());
    fs::write(&path, &original).unwrap();
    assert_eq!(state(&f.engine), before);
}
#[test]
fn compiler_diagnostics_are_refusal_and_captured_javascript_is_never_imported() {
    let bad = "export function invalid():string { return 42; }\n";
    let f = configured(bad, bad);
    let before = state(&f.engine);
    assert!(
        run(&f, &Default::default())
            .unwrap_err()
            .to_string()
            .contains("passed captured compiler contexts")
    );
    assert_eq!(state(&f.engine), before);
    let sentinel = tempfile::tempdir().unwrap();
    let marker = sentinel.path().join("must-not-exist");
    let js = format!(
        "require('node:fs').writeFileSync({},'executed'); exports.value = 1;\n",
        serde_json::to_string(marker.to_str().unwrap()).unwrap()
    );
    let cfg = r#"{"compilerOptions":{"allowJs":true,"checkJs":false,"strict":false,"target":"ES2022","module":"CommonJS","noEmit":true},"files":["main.ts","evil.js"]}"#;
    let src = "import './evil.js'; export function safe() { return 'OLD'; }\n";
    let head = src.replace("'OLD'", "'NEW'");
    let f = Fixture::new(
        &[("main.ts", src), ("evil.js", &js), ("tsconfig.json", cfg)],
        &[("main.ts", &head), ("evil.js", &js), ("tsconfig.json", cfg)],
    );
    let before = state(&f.engine);
    assert!(!marker.exists());
    run(&f, &Default::default()).unwrap();
    assert!(!marker.exists(), "Compiler executed captured source");
    assert_eq!(state(&f.engine), before);
}
#[test]
fn actual_cli_matches_library_and_refuses_authority_overrides_before_store_creation() {
    let f = configured(SOURCE, &SOURCE.replace("DERIVED_BASE", "DERIVED_HEAD"));
    let before = state(&f.engine);
    let s = spec();
    let expected = run(&f, &Default::default()).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_yoke"))
        .args([
            "--root",
            f.dir.path().to_str().unwrap(),
            "--max-bytes",
            "4000000",
            "git",
            "compiler-affected",
            f.report["artifactId"].as_str().unwrap(),
            "--node",
            s.node.to_str().unwrap(),
            "--typescript",
            s.typescript.to_str().unwrap(),
            "--trust-compiler",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap(),
        expected
    );
    assert_eq!(state(&f.engine), before);
    let root = tempfile::tempdir().unwrap();
    let missing = root.path().join("missing");
    for grant in [false, true] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_yoke"));
        command.args(["--root", missing.to_str().unwrap()]);
        if grant {
            command.arg("--allow-writes");
        }
        command.args([
            "git",
            "compiler-affected",
            &"a".repeat(64),
            "--node",
            s.node.to_str().unwrap(),
            "--typescript",
            s.typescript.to_str().unwrap(),
        ]);
        if grant {
            command.arg("--trust-compiler");
        }
        let output = command.output().unwrap();
        assert!(!output.status.success());
        assert!(!missing.exists());
    }
}
