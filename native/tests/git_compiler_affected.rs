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
fn invocation_bindings_and_original_capture_limits_remain_visible() {
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
    assert!(coverage.iter().any(|u| u["reason"]
        == "construct_signature_has_no_source_declaration"
        && u["anchor"]["text"] == "new C()"));
    assert!(coverage.iter().all(|u| !matches!(
        u["reason"].as_str(),
        Some(
            "construct_signature_not_enumerated"
                | "tagged_template_signature_not_enumerated"
                | "static_possible_dispatch_not_enumerated"
        )
    )));
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
            .all(|e| e["source"] != direct)
    );
    let derived = node_id(c, "go", "'DERIVED'");
    let possible = edges(c, "possibleDispatch", &selected, &derived);
    assert_eq!(possible.len(), 1);
    assert_eq!(
        possible[0]["basis"],
        "compiler_constructor_type_assignability"
    );
    let tagged = node_id(c, "tagged", "tag`literal`");
    let tag = node_id(c, "tag", "return parts[0]");
    assert_eq!(
        edges(c, "calls", &tagged, &tag)[0]["invocationKind"],
        "taggedTemplate"
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
fn constructor_signatures_bodies_result_types_and_implicit_construction_keep_separate_roles() {
    let source = concat!(
        "\u{feff}// π 😀 e\u{301}\r\n",
        "class Base { protected brand = true; value:string; constructor(v:string); constructor(v:number); constructor(v:string|number) { this.value = 'BASE_CTOR_OLD'+String(v); } }\r\n",
        "class Derived extends Base {}\r\n",
        "class Own extends Base { constructor(v:string|number) { super(String(v)); this.value += 'OWN_CTOR_OLD'; } }\r\n",
        "class Implicit {}\r\n",
        "class Wrong { constructor(v:number) {} }\r\n",
        "class PrivateCtor extends Base { private constructor(v:string|number) { super(String(v)); } }\r\n",
        "abstract class AbstractCtor extends Base {}\r\n",
        "function direct() { new Derived('d'); new Own('o'); new Implicit(); }\r\n",
        "function selected(C:typeof Base) { new C('a'); new C('b'); }\r\n"
    );
    let f = configured(source, &source.replace("CTOR_OLD", "CTOR_HEAD"));
    let before = state(&f.engine);
    let r = run(&f, &Default::default()).unwrap();
    let c = context(&r);
    let base_body = node_id(c, "constructor", "BASE_CTOR_HEAD");
    let base_class = node_id(c, "Base", "BASE_CTOR_HEAD");
    let own_body = node_id(c, "constructor", "OWN_CTOR_HEAD");
    let derived = node_id(c, "Derived", "extends Base");
    let own = node_id(c, "Own", "OWN_CTOR_HEAD");
    let implicit = node_id(c, "Implicit", "class Implicit");
    let direct = node_id(c, "direct", "new Derived");
    let selected = node_id(c, "selected", "new C('a')");
    let relations = c["relations"].as_array().unwrap();
    assert_eq!(
        relations
            .iter()
            .filter(|e| e["relation"] == "signatureImplementation" && e["target"] == base_body)
            .count(),
        2
    );
    let inherited: Vec<_> = relations
        .iter()
        .filter(|e| e["relation"] == "calls" && e["proof"]["text"] == "new Derived('d')")
        .collect();
    assert_eq!(inherited.len(), 1);
    assert_eq!(inherited[0]["invocationKind"], "new");
    assert_ne!(
        inherited[0]["target"], derived,
        "Inherited constructor declaration is not the constructed result class"
    );
    assert!(relations.iter().any(|e| e["relation"] == "constructResult"
        && e["source"] == direct
        && e["target"] == derived
        && e["resolvedTarget"] == inherited[0]["target"]));
    assert_eq!(edges(c, "calls", &direct, &own_body).len(), 1);
    assert!(relations.iter().any(|e| e["relation"] == "calls"
        && e["source"] == own_body
        && e["invocationKind"] == "super"
        && e["proof"]["text"] == "super(String(v))"));
    assert_eq!(edges(c, "constructResult", &direct, &implicit).len(), 1);
    assert!(
        relations
            .iter()
            .filter(|e| e["proof"]["text"] == "new Implicit()")
            .all(|e| e["relation"] != "calls")
    );
    assert!(
        c["unavailable"]
            .as_array()
            .unwrap()
            .iter()
            .any(
                |u| u["reason"] == "construct_signature_has_no_source_declaration"
                    && u["anchor"]["text"] == "new Implicit()"
            )
    );
    let possible = edges(c, "possibleConstruct", &selected, &own_body);
    assert_eq!(possible.len(), 2);
    assert_ne!(possible[0]["id"], possible[1]["id"]);
    let base_candidates = edges(c, "possibleConstruct", &selected, &base_body);
    assert_eq!(
        base_candidates.len(),
        4,
        "Two original calls retain Base and Derived owners even when both share one inherited constructor body"
    );
    for owner in [&base_class, &derived] {
        let owned: Vec<_> = base_candidates
            .iter()
            .filter(|e| e["candidateOwner"] == *owner)
            .collect();
        assert_eq!(
            owned.len(),
            2,
            "One body per candidate owner and original call, without multiplying overload signatures"
        );
        for quote in ["new C('a')", "new C('b')"] {
            assert_eq!(
                owned.iter().filter(|e| e["proof"]["text"] == quote).count(),
                1
            );
        }
    }
    assert!(possible.iter().all(|e| e["candidateOwner"] == own
        && e["basis"] == "compiler_constructor_type_assignability"
        && e["invocationKind"] == "new"));
    let forbidden: Vec<_> = c["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|n| {
            matches!(
                n["name"].as_str(),
                Some("Wrong" | "PrivateCtor" | "AbstractCtor")
            )
        })
        .map(|n| n["id"].clone())
        .collect();
    assert!(
        relations
            .iter()
            .filter(|e| e["relation"] == "possibleConstruct")
            .all(|e| !forbidden.contains(&e["candidateOwner"]))
    );
    assert!(
        relations
            .iter()
            .filter(|e| e["relation"] == "possibleConstruct")
            .all(|e| e["source"] != direct)
    );
    assert!(
        r["incomingProofs"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["sourceNodeId"] == selected)
    );
    assert_eq!(state(&f.engine), before);
}
#[test]
fn tagged_and_static_dispatch_use_both_receiver_sides_without_equating_literal_class_references() {
    let source = concat!(
        "interface Service { run(v:string):string; }\n",
        "class Base { run(v:string){return 'INSTANCE_BASE'+v;} static run(v:string){return 'STATIC_BASE'+v;} tag(s:TemplateStringsArray){return s[0];} static tag(s:TemplateStringsArray){return s[0];} }\n",
        "class Derived extends Base { override run(v:string){return 'INSTANCE_OLD'+v;} static override run(v:string){return 'STATIC_OLD'+v;} override tag(s:TemplateStringsArray){return 'TAG_OLD'+s[0];} static override tag(s:TemplateStringsArray){return 'STATIC_TAG_OLD'+s[0];} }\n",
        "class StaticOnly { static run(v:string){return 'STATIC_ONLY'+v;} }\n",
        "class WrongCtor { constructor(v:number){} static run(v:string){return 'WRONG_CTOR'+v;} }\n",
        "class WrongType { static run(v:number){return String(v);} }\n",
        "class Generic<T> { field?:T; static run(v:string){return 'GENERIC_STATIC'+v;} }\n",
        "function selected(C:typeof Base){C.run('a');C.tag`static`; }\n",
        "function literal(){Base.run('b');Base.tag`literal`; }\n",
        "function structural(x:Service){return x.run('s');}\n",
        "function instance(x:Base|undefined){x?.run('o');return x?.tag;}\n",
        "function methodTag(x:Base){return x.tag`method`;}\n",
        "function tag(s:TemplateStringsArray,...v:unknown[]):string; function tag(s:TemplateStringsArray,...v:unknown[]){return 'FREE_TAG_OLD'+s[0];}\n",
        "function tagged(){return tag`π ${1}`;}\n"
    );
    let f = configured(source, &source.replace("_OLD", "_HEAD"));
    let before = state(&f.engine);
    let r = run(&f, &Default::default()).unwrap();
    let c = context(&r);
    let selected = node_id(c, "selected", "C.run");
    let literal = node_id(c, "literal", "Base.run");
    assert_eq!(
        c["relations"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["source"] == literal && e["relation"] == "calls")
            .count(),
        2
    );
    let structural = node_id(c, "structural", "x.run");
    let static_run = node_id(c, "run", "'STATIC_HEAD'");
    let instance_run = node_id(c, "run", "'INSTANCE_HEAD'");
    assert_eq!(
        edges(c, "possibleDispatch", &selected, &static_run).len(),
        1
    );
    assert_eq!(
        edges(c, "possibleDispatch", &selected, &instance_run).len(),
        0
    );
    assert!(
        c["relations"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["source"] == literal)
            .all(|e| e["relation"] != "possibleDispatch")
    );
    for body in ["STATIC_ONLY", "WRONG_CTOR", "GENERIC_STATIC"] {
        let target = node_id(c, "run", body);
        let found = edges(c, "possibleDispatch", &structural, &target);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0]["basis"], "compiler_constructor_type_assignability");
    }
    assert!(
        edges(
            c,
            "possibleDispatch",
            &selected,
            &node_id(c, "run", "WRONG_CTOR")
        )
        .is_empty()
    );
    assert!(
        edges(
            c,
            "possibleDispatch",
            &structural,
            &node_id(c, "run", "String(v)")
        )
        .is_empty()
    );
    let method_caller = node_id(c, "methodTag", "x.tag`method`");
    let method_target = node_id(c, "tag", "'TAG_HEAD'");
    assert_eq!(
        edges(c, "possibleDispatch", &method_caller, &method_target)[0]["invocationKind"],
        "taggedTemplate"
    );
    let static_target = node_id(c, "tag", "'STATIC_TAG_HEAD'");
    assert_eq!(
        edges(c, "possibleDispatch", &selected, &static_target)[0]["invocationKind"],
        "taggedTemplate"
    );
    let optional = node_id(c, "instance", "x?.run");
    assert_eq!(
        edges(c, "possibleDispatch", &optional, &instance_run)[0]["conditional"],
        true
    );
    let body = node_id(c, "tag", "'FREE_TAG_HEAD'");
    let tagged = node_id(c, "tagged", "tag`π");
    assert!(
        r["states"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["nodeId"] == tagged)
    );
    assert!(
        c["relations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["relation"] == "signatureImplementation" && e["target"] == body)
    );
    assert_eq!(state(&f.engine), before);
}
#[test]
fn imported_constructor_aliases_and_union_result_types_keep_original_declaration_identity() {
    let source = "import { A as Alias, B } from './types';\r\nfunction direct(){return new Alias('a');}\r\nfunction selected(C:typeof Alias|typeof B){return new C('u');}\r\nfunction staticSelected(C:typeof Alias|typeof B){return C.pick('p');}\r\nfunction staticDirect(){return Alias.pick('d');}\r\n";
    let types = "export class A { private a=true; constructor(public value:string){ this.value+='A_OLD'; } static pick(v:string){return 'A_STATIC'+v;} }\r\nexport class B { private b=true; constructor(public value:string){ this.value+='B_OLD'; } static pick(v:string){return 'B_STATIC'+v;} }\r\nexport class Wrong { constructor(public value:number){} static pick(v:number){return v;} }\r\n";
    let config=json!({"compilerOptions":{"strict":true,"target":"ES2022","module":"CommonJS","noEmit":true},"files":[P]}).to_string();
    let head = types.replace("_OLD", "_HEAD");
    let f = Fixture::new(
        &[
            (P, source),
            ("src/types.ts", types),
            ("tsconfig.json", &config),
        ],
        &[
            (P, source),
            ("src/types.ts", &head),
            ("tsconfig.json", &config),
        ],
    );
    let before = state(&f.engine);
    let r = run(&f, &Default::default()).unwrap();
    let c = context(&r);
    let a = node_id(c, "A", "A_HEAD");
    let b = node_id(c, "B", "B_HEAD");
    let selected = node_id(c, "selected", "new C('u')");
    let direct = node_id(c, "direct", "new Alias");
    assert_eq!(edges(c, "constructResult", &selected, &a).len(), 1);
    assert_eq!(edges(c, "constructResult", &selected, &b).len(), 1);
    assert_eq!(edges(c, "constructResult", &direct, &a).len(), 1);
    assert!(edges(c, "constructResult", &direct, &b).is_empty());
    let ac = node_id(c, "constructor", "A_HEAD");
    let bc = node_id(c, "constructor", "B_HEAD");
    assert_eq!(edges(c, "possibleConstruct", &selected, &ac).len(), 1);
    assert_eq!(edges(c, "possibleConstruct", &selected, &bc).len(), 1);
    assert!(
        c["relations"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["source"] == direct)
            .all(|e| e["relation"] != "possibleConstruct")
    );
    let static_direct = node_id(c, "staticDirect", "Alias.pick");
    assert_eq!(
        c["relations"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["source"] == static_direct && e["relation"] == "calls")
            .count(),
        1
    );
    assert!(
        c["relations"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["source"] == static_direct)
            .all(|e| e["relation"] != "possibleDispatch")
    );
    let static_selected = node_id(c, "staticSelected", "C.pick");
    let ast = node_id(c, "pick", "A_STATIC");
    let bst = node_id(c, "pick", "B_STATIC");
    assert!(
        c["relations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["source"] == static_selected
                && (e["target"] == ast || e["target"] == bst)
                && e["relation"] == "possibleDispatch")
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
