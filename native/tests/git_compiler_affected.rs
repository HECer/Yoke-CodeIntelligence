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
fn changed_method_row(r: &Value, c: &Value, method: &str) -> usize {
    let node = c["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == method)
        .unwrap();
    let rows: Vec<_> = r["originalChangedSymbols"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .filter(|(_, row)| {
            row["side"] == "head"
                && row["kind"] == "method"
                && node["baseEntityIds"]
                    .as_array()
                    .unwrap()
                    .contains(&row["id"])
        })
        .map(|(index, _)| index)
        .collect();
    assert_eq!(rows.len(), 1, "Exact changed method binding is required");
    rows[0]
}

#[test]
fn unobserved_generic_overrides_retain_source_contract_callers_without_invented_instances() {
    let calls = (0..20)
        .map(|n| format!("x.run('p{n}');"))
        .collect::<String>();
    let source = format!(
        "\u{feff}// π 😀 e\u{301}\r\nexport class Base{{run(v:string):string{{return 'BASE_BODY'+v;}}}}\r\nexport class Generic<T> extends Base{{value!:T;override run(v:string):string{{return 'GENERIC_OLD'+v;}}}}\r\nfunction caller(x:Base){{{calls}}}\r\n"
    );
    let f = configured(&source, &source.replace("GENERIC_OLD", "GENERIC_HEAD"));
    let before = state(&f.engine);
    let r = run(&f, &Default::default()).unwrap();
    let c = context(&r);
    let derived = node_id(c, "run", "GENERIC_HEAD");
    let base = node_id(c, "run", "BASE_BODY");
    let owner = node_id(c, "Generic", "GENERIC_HEAD");
    let caller = node_id(c, "caller", "p19");
    let row = changed_method_row(&r, c, &derived);
    assert!(
        c["instances"]
            .as_array()
            .unwrap()
            .iter()
            .all(|i| i["owner"] != owner)
    );
    assert!(edges(c, "possibleDispatch", &caller, &derived).is_empty());
    assert!(edges(c, "possiblePropertyAccess", &caller, &derived).is_empty());
    let binding = edges(c, "overrides", &derived, &base);
    assert_eq!(binding.len(), 1);
    let expansion: Vec<_> = r["contractExpansions"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p["changedRow"] == row && p["contextId"] == c["contextId"])
        .collect();
    assert_eq!(expansion.len(), 1);
    assert_eq!(expansion[0]["relationId"], binding[0]["id"]);
    assert_eq!(expansion[0]["sourceNodeId"], derived);
    assert_eq!(expansion[0]["targetNodeId"], base);
    assert_eq!(expansion[0]["direction"], "outgoing_compiler_binding");
    assert_eq!(expansion[0]["distance"], 0);
    assert_eq!(expansion[0]["runtimeDispatchEstablished"], false);
    assert!(
        r["states"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["changedRow"] == row
                && s["nodeId"] == base
                && s["distance"] == 0
                && s["originalSeed"] == false)
    );
    assert!(
        r["states"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["changedRow"] == row && s["nodeId"] == caller && s["distance"] == 1)
    );
    let call_ids: Vec<_> = edges(c, "calls", &caller, &base)
        .iter()
        .map(|e| e["id"].clone())
        .collect();
    assert_eq!(call_ids.len(), 20);
    assert_eq!(
        r["incomingProofs"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|p| p["changedRow"] == row && call_ids.contains(&p["relationId"]))
            .count(),
        20
    );
    let capacity = r["incomingProofs"].as_array().unwrap().len()
        + r["contractExpansions"].as_array().unwrap().len()
        - 1;
    assert!(capacity >= r["incomingProofs"].as_array().unwrap().len());
    for side in ["base", "head"] {
        for context in r[side]["contexts"].as_array().unwrap() {
            assert!(
                context["relations"].as_array().unwrap().len() <= capacity
                    && context["unavailable"].as_array().unwrap().len() <= capacity
            );
        }
    }
    assert!(
        run(
            &f,
            &Options {
                max_relations: capacity,
                ..Default::default()
            }
        )
        .unwrap_err()
        .to_string()
        .contains("capacity")
    );
    assert_eq!(state(&f.engine), before);
}

#[test]
fn zero_distance_contract_steps_improve_enqueued_paths_and_keep_alternative_proofs_and_boundaries()
{
    let source = "export class Base{run():string{return Child.step();}}\nexport class Child<T> extends Base{value!:T;static step(){return 'STEP_OLD';}override run(){return 'OVERRIDE_OLD';}}\nfunction caller(x:Base){return x.run();}\n";
    let f = configured(source, &source.replace("_OLD", "_HEAD"));
    let r = run(&f, &Default::default()).unwrap();
    let c = context(&r);
    let owner = node_id(c, "Child", "OVERRIDE_HEAD");
    let base = node_id(c, "run", "Child.step");
    let step = node_id(c, "step", "STEP_HEAD");
    let caller = node_id(c, "caller", "x.run");
    let entity = c["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == owner)
        .unwrap();
    let row = r["originalChangedSymbols"]
        .as_array()
        .unwrap()
        .iter()
        .position(|p| {
            p["side"] == "head"
                && p["kind"] == "class"
                && entity["baseEntityIds"]
                    .as_array()
                    .unwrap()
                    .contains(&p["id"])
        })
        .unwrap();
    let base_states: Vec<_> = r["states"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|s| s["changedRow"] == row && s["nodeId"] == base)
        .collect();
    assert_eq!(
        base_states.len(),
        1,
        "The superseded queue entry cannot emit a duplicate state"
    );
    assert_eq!(base_states[0]["distance"], 0);
    assert_eq!(base_states[0]["originalSeed"], false);
    assert!(
        r["states"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["changedRow"] == row && s["nodeId"] == caller && s["distance"] == 1)
    );
    let earlier = edges(c, "calls", &base, &step);
    assert_eq!(earlier.len(), 1);
    assert!(
        r["incomingProofs"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["changedRow"] == row
                && p["relationId"] == earlier[0]["id"]
                && p["distance"] == 1),
        "The longer original proof remains even when the state gets a shorter contract path"
    );
    let zero = run(
        &f,
        &Options {
            max_depth: 0,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(zero["contractExpansions"].as_array().unwrap().is_empty());
    assert!(zero["incomingProofs"].as_array().unwrap().is_empty());
    assert!(
        zero["states"]
            .as_array()
            .unwrap()
            .iter()
            .all(|s| s["originalSeed"] == true)
    );
    assert!(
        zero["states"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["unexaminedContractBindings"].as_u64().unwrap() > 0)
    );
    let one = run(
        &f,
        &Options {
            max_depth: 1,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(
        one["states"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["changedRow"] == row
                && s["nodeId"] == caller
                && s["distance"] == 1
                && s["boundary"] == true)
    );
}

#[test]
fn overload_diamond_contracts_and_fixed_static_private_bindings_keep_distinct_original_roles() {
    let source = concat!(
        "export interface Root{run(v:string):string;run(v:number):number;}interface Left extends Root{}interface Right extends Root{}\n",
        "class Generic<T> implements Left,Right{value!:T;run(v:string):string;run(v:number):number;run(v:string|number){const marker='GENERIC_OLD';return v;}}\n",
        "function caller(x:Root){x.run('a');x.run(1);}\n",
        "class StaticBase{static run(){return 'BASE';}#secret(){return 'PRIVATE_BASE';}read(){return this.#secret();}}\n",
        "class StaticChild extends StaticBase{static override run(){return 'STATIC_OLD';}#secret(){return 'PRIVATE_OLD';}override read(){return this.#secret();}}\n",
        "function fixed(){return StaticBase.run();}\n"
    );
    let f = configured(source, &source.replace("_OLD", "_HEAD"));
    let r = run(&f, &Default::default()).unwrap();
    let c = context(&r);
    let implementation = node_id(c, "run", "GENERIC_HEAD");
    let row = changed_method_row(&r, c, &implementation);
    let expansions: Vec<_> = r["contractExpansions"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p["changedRow"] == row && p["sourceNodeId"] == implementation)
        .collect();
    assert_eq!(
        expansions.len(),
        4,
        "Both original clauses and both overload declarations remain parallel proofs"
    );
    let ids: std::collections::BTreeSet<_> = expansions
        .iter()
        .map(|p| p["relationId"].as_str().unwrap())
        .collect();
    assert_eq!(ids.len(), 4);
    for p in expansions {
        let original = c["relations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["id"] == p["relationId"])
            .unwrap();
        assert_eq!(original["relation"], "implementsMember");
        assert_eq!(original["source"], p["sourceNodeId"]);
        assert_eq!(original["target"], p["targetNodeId"]);
        assert!(original["proof"]["text"] == "Left" || original["proof"]["text"] == "Right");
    }
    let fixed = node_id(c, "fixed", "StaticBase.run");
    let base = node_id(c, "run", "'BASE'");
    let derived = node_id(c, "run", "STATIC_HEAD");
    assert_eq!(edges(c, "calls", &fixed, &base).len(), 1);
    assert!(edges(c, "calls", &fixed, &derived).is_empty());
    assert!(edges(c, "possibleDispatch", &fixed, &derived).is_empty());
    for p in r["contractExpansions"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p["contextId"] == c["contextId"])
    {
        let node = c["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["id"] == p["sourceNodeId"])
            .unwrap();
        assert!(!node["name"].as_str().unwrap().starts_with('#'));
        assert_eq!(p["runtimeDispatchEstablished"], false);
    }
}

#[test]
fn compiler_contract_expansion_proofs_and_states_remain_context_and_side_scoped() {
    let source = "export interface Root{run(v:string):string;}class Generic<T> implements Root{value!:T;run(v:string){return 'OLD'+v;}}function caller(x:Root){return x.run('a');}\n";
    let head = source.replace("'OLD'", "'HEAD'");
    let strict =
        json!({"compilerOptions":{"strict":true,"target":"ES2022","noEmit":true},"files":[P]})
            .to_string();
    let loose=json!({"compilerOptions":{"strict":false,"target":"ES2022","noEmit":true},"files":[format!("../{P}")]}).to_string();
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
    let before = state(&f.engine);
    let r = run(&f, &Default::default()).unwrap();
    let mut all_ids = std::collections::BTreeSet::new();
    for side in ["base", "head"] {
        assert_eq!(r[side]["contexts"].as_array().unwrap().len(), 2);
        for c in r[side]["contexts"].as_array().unwrap() {
            let ids: std::collections::BTreeSet<_> = c["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|n| n["id"].as_str().unwrap())
                .collect();
            assert!(all_ids.is_disjoint(&ids));
            all_ids.extend(ids.iter().copied());
            let expansions: Vec<_> = r["contractExpansions"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|p| p["side"] == side && p["contextId"] == c["contextId"])
                .collect();
            assert!(!expansions.is_empty());
            for p in expansions {
                assert!(
                    ids.contains(p["sourceNodeId"].as_str().unwrap())
                        && ids.contains(p["targetNodeId"].as_str().unwrap())
                );
            }
        }
    }
    assert_eq!(state(&f.engine), before);
}
fn instance_witnesses<'a>(
    c: &'a Value,
    owner: &str,
    anchor: &str,
    argument: &str,
) -> Vec<&'a Value> {
    c["instances"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|w| {
            w["owner"] == owner
                && w["anchor"]["text"] == anchor
                && w["arguments"][0]["display"] == argument
        })
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
            .all(|e| e["relation"] == "calls"
                || (e["relation"] == "accessesProperty"
                    && e["target"] == base
                    && e["accessMode"] == "read"
                    && e["conditional"] == false))
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
            .all(|e| e["relation"] == "calls"
                || (e["relation"] == "accessesProperty"
                    && e["target"] == static_base
                    && e["accessMode"] == "read"
                    && e["conditional"] == false))
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
            .any(|u| u["reason"] == "unobserved_generic_instantiations_not_enumerated")
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
fn class_only_changes_reach_dynamic_constructor_callers_when_the_body_is_inherited() {
    let source = concat!(
        "\u{feff}// π 😀\r\nexport class Base { protected brand=1; constructor(){void 'BASE_CTOR';} }\r\n",
        "export class Derived extends Base { field='OLD'; }\r\n",
        "export class Other { private other=1; constructor(){} }\r\n",
        "export function selected(C:typeof Base){new C();new C();}\r\n",
        "export function literal(){return new Derived();}\r\n",
        "export function unrelated(C:typeof Other){return new C();}\r\n"
    );
    let f = configured(source, &source.replace("'OLD'", "'NEW'"));
    let before = state(&f.engine);
    let r = run(&f, &Default::default()).unwrap();
    let c = context(&r);
    let derived = node_id(c, "Derived", "field='NEW'");
    let selected = node_id(c, "selected", "new C();new C()");
    let literal = node_id(c, "literal", "new Derived()");
    let unrelated = node_id(c, "unrelated", "typeof Other");
    let row = r["originalChangedSymbols"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .find(|(_, v)| v["side"] == "head" && v["kind"] == "class" && v["name"] == "Derived")
        .unwrap()
        .0;
    let values = edges(c, "possibleConstructValue", &selected, &derived);
    assert_eq!(values.len(), 2);
    assert_ne!(values[0]["id"], values[1]["id"]);
    assert_ne!(
        values[0]["proof"]["startByte"],
        values[1]["proof"]["startByte"]
    );
    assert!(values.iter().all(|e| e["candidateOwner"] == derived
        && e["basis"] == "compiler_constructor_type_assignability"
        && e["invocationKind"] == "new"));
    let body = node_id(c, "constructor", "'BASE_CTOR'");
    assert!(
        edges(c, "possibleConstruct", &selected, &body)
            .iter()
            .any(|e| e["candidateOwner"] == derived)
    );
    for caller in [&selected, &literal] {
        assert!(
            r["states"]
                .as_array()
                .unwrap()
                .iter()
                .any(|s| s["changedRow"] == row && s["side"] == "head" && s["nodeId"] == *caller)
        );
    }
    assert!(
        !r["states"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["changedRow"] == row && s["side"] == "head" && s["nodeId"] == unrelated)
    );
    assert_eq!(edges(c, "constructResult", &literal, &derived).len(), 1);
    assert!(edges(c, "possibleConstructValue", &literal, &derived).is_empty());
    assert_eq!(r["runtimeDispatchEstablished"], false);
    assert_eq!(state(&f.engine), before);
}
#[test]
fn captured_generic_instances_preserve_arguments_aliases_union_intersection_and_parallel_witnesses()
{
    let source = concat!(
        "\u{feff}// π 😀 e\u{301}\r\nexport interface Runner {run(v:string):string;}\r\ninterface Extra extends Runner {extra:true;}\r\ninterface ObjectRunner {run(v:{value:string}):{value:string};}\r\n",
        "class Box<T>{constructor(public value:T){}run(v:T):T{const marker='BOX_OLD';return v;}clone():Box<T>{return this;}}\r\ninterface Box<T>{ optional?:T; }\r\n",
        "class Phantom<T>{run(v:string){return 'PHANTOM_OLD'+v;}}\r\nclass Missing<T>{run(v:T):T{return v;}}\r\n",
        "type TextBox=Box<string>;type Both=Box<string>|Box<number>;type WithExtra=Box<string>&{extra:true};\r\n",
        "const inferred=new Box('x');const numberBox=new Box<number>(1);let alias!:TextBox;let both!:Both;let extended!:WithExtra;let phantom!:Phantom<number>;let anyBox!:Box<any>;let unknownBox!:Box<unknown>;let objectBox!:Box<{value:string}>;\r\n",
        "function caller(x:Runner){x.run('a');x.run('b');}\r\nfunction extraCaller(x:Extra){return x.run('e');}\r\nfunction objectCaller(x:ObjectRunner){return x.run({value:'v'});}\r\n"
    );
    let f = configured(source, &source.replace("_OLD", "_HEAD"));
    let before = state(&f.engine);
    let r = run(&f, &Default::default()).unwrap();
    let c = context(&r);
    let owner = node_id(c, "Box", "BOX_HEAD");
    let body = node_id(c, "run", "BOX_HEAD");
    let caller = node_id(c, "caller", "x.run('a')");
    let extra = node_id(c, "extraCaller", "x.run('e')");
    let object = node_id(c, "objectCaller", "x.run({value");
    let inferred = instance_witnesses(c, &owner, "new Box('x')", "string");
    assert_eq!(inferred.len(), 1);
    let id = &inferred[0]["id"];
    let from_inferred: Vec<_> = edges(c, "possibleDispatch", &caller, &body)
        .into_iter()
        .filter(|e| e["candidateInstance"] == *id)
        .collect();
    assert_eq!(from_inferred.len(), 2);
    assert_ne!(from_inferred[0]["id"], from_inferred[1]["id"]);
    assert_ne!(
        from_inferred[0]["proof"]["startByte"],
        from_inferred[1]["proof"]["startByte"]
    );
    assert!(
        from_inferred
            .iter()
            .all(|e| e["basis"] == "compiler_captured_instance_assignability"
                && e["candidateOwner"] == owner)
    );
    for witness in c["instances"].as_array().unwrap().iter().filter(|w| {
        w["owner"] == owner
            && matches!(
                w["arguments"][0]["display"].as_str(),
                Some("number" | "unknown")
            )
    }) {
        assert!(
            edges(c, "possibleDispatch", &caller, &body)
                .iter()
                .all(|e| e["candidateInstance"] != witness["id"]),
            "An incompatible instance was admitted by its generic class name"
        );
    }
    let union_string = instance_witnesses(c, &owner, "Both", "string");
    let union_number = instance_witnesses(c, &owner, "Both", "number");
    assert_eq!(union_string.len(), 1);
    assert_eq!(union_number.len(), 1);
    assert!(
        edges(c, "possibleDispatch", &caller, &body)
            .iter()
            .any(|e| e["candidateInstance"] == union_string[0]["id"])
    );
    let intersection = instance_witnesses(c, &owner, "WithExtra", "string");
    assert_eq!(intersection.len(), 1);
    assert!(
        edges(c, "possibleDispatch", &extra, &body)
            .iter()
            .any(|e| e["candidateInstance"] == intersection[0]["id"])
    );
    assert!(
        edges(c, "possibleDispatch", &extra, &body)
            .iter()
            .all(|e| e["candidateInstance"] != *id),
        "Intersection properties must not be erased to the class component"
    );
    let any = instance_witnesses(c, &owner, "Box<any>", "any");
    assert_eq!(any.len(), 1);
    assert_eq!(any[0]["arguments"][0]["flags"], 1);
    assert!(
        edges(c, "possibleDispatch", &caller, &body)
            .iter()
            .any(|e| e["candidateInstance"] == any[0]["id"]),
        "Source-authored any stays explicit rather than being invented or hidden"
    );
    let object_witness = c["instances"]
        .as_array()
        .unwrap()
        .iter()
        .find(|w| w["owner"] == owner && w["anchor"]["text"] == "Box<{value:string}>")
        .unwrap();
    assert!(
        edges(c, "possibleDispatch", &object, &body)
            .iter()
            .any(|e| e["candidateInstance"] == object_witness["id"])
    );
    let phantom_owner = node_id(c, "Phantom", "PHANTOM_HEAD");
    let phantom_body = node_id(c, "run", "PHANTOM_HEAD");
    let phantom = instance_witnesses(c, &phantom_owner, "Phantom<number>", "number");
    assert_eq!(phantom.len(), 1);
    assert!(
        edges(c, "possibleDispatch", &caller, &phantom_body)
            .iter()
            .any(|e| e["candidateInstance"] == phantom[0]["id"]),
        "Unused type parameters do not invent nominal incompatibility"
    );
    let missing = node_id(c, "Missing", "class Missing");
    assert!(
        c["instances"]
            .as_array()
            .unwrap()
            .iter()
            .all(|w| w["owner"] != missing)
    );
    assert!(
        c["relations"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["relation"] == "possibleDispatch")
            .all(|e| e["candidateOwner"] != missing),
        "An unobserved generic family cannot be filled with guessed any"
    );
    assert!(
        r["incomingProofs"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["sourceNodeId"] == caller)
    );
    assert_eq!(r["runtimeDispatchEstablished"], false);
    assert_eq!(r["complete"], false);
    assert_eq!(state(&f.engine), before);
}
#[test]
fn generic_witnesses_and_assignability_remain_separate_per_compiler_context() {
    let source = "export interface Runner{run(v:string):string;}\nclass Generic<T>{constructor(public value:T){}run(v:T):T|null{const marker='OLD';return v;}}\nconst observed=new Generic('s');\nfunction caller(x:Runner){return x.run('s');}\n";
    let head = source.replace("'OLD'", "'HEAD'");
    let strict =
        json!({"compilerOptions":{"strict":true,"target":"ES2022","noEmit":true},"files":[P]})
            .to_string();
    let loose=json!({"compilerOptions":{"strict":false,"strictNullChecks":false,"target":"ES2022","noEmit":true},"files":[format!("../{P}")]}).to_string();
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
    let before = state(&f.engine);
    let r = run(&f, &Default::default()).unwrap();
    let mut contexts = BTreeMap::new();
    let mut identities = vec![];
    for c in r["head"]["contexts"].as_array().unwrap() {
        let caller = node_id(c, "caller", "x.run");
        let body = node_id(c, "run", "'HEAD'");
        let owner = node_id(c, "Generic", "'HEAD'");
        let witnesses = instance_witnesses(c, &owner, "new Generic('s')", "string");
        assert_eq!(witnesses.len(), 1);
        let count = edges(c, "possibleDispatch", &caller, &body)
            .iter()
            .filter(|e| e["candidateInstance"] == witnesses[0]["id"])
            .count();
        contexts.insert(c["config"].as_str().unwrap(), count);
        identities.push(
            c["instances"]
                .as_array()
                .unwrap()
                .iter()
                .map(|w| w["id"].as_str().unwrap())
                .collect::<std::collections::BTreeSet<_>>(),
        );
    }
    assert_eq!(contexts["tsconfig.json"], 0);
    assert_eq!(contexts["loose/tsconfig.json"], 1);
    assert!(identities[0].is_disjoint(&identities[1]));
    assert_eq!(state(&f.engine), before);
}
#[test]
fn imported_alias_shadowing_and_jsdoc_generic_classes_bind_to_their_actual_class_symbols() {
    let source = "import {Box as Imported} from './types';\r\nimport {JsBox} from './box.js';\r\nexport interface Runner{run(v:string):string;}\r\nclass Box<T>{constructor(public value:T){}run(v:T):T{const marker='LOCAL_OLD';return v;}}\r\nconst good=new Imported('s');const bad=new Box(1);const js=new JsBox('j');\r\nfunction caller(x:Runner){return x.run('c');}\r\n";
    let types = "export class Box<T>{constructor(public value:T){}run(v:T):T{const marker='IMPORTED_OLD';return v;}}\r\n";
    let js = "/** @template T */\r\nexport class JsBox{\r\n/** @param {T} value */\r\nconstructor(value){this.value=value;}\r\n/**\r\n * @param {T} v\r\n * @returns {T}\r\n */\r\nrun(v){const marker='JS_OLD';return v;}\r\n}\r\n";
    let cfg=json!({"compilerOptions":{"strict":true,"allowJs":true,"checkJs":true,"target":"ES2022","module":"CommonJS","noEmit":true},"files":[P,"src/types.ts","src/box.js"]}).to_string();
    let source_head = source.replace("_OLD", "_HEAD");
    let types_head = types.replace("_OLD", "_HEAD");
    let js_head = js.replace("_OLD", "_HEAD");
    let f = Fixture::new(
        &[
            (P, source),
            ("src/types.ts", types),
            ("src/box.js", js),
            ("tsconfig.json", &cfg),
        ],
        &[
            (P, &source_head),
            ("src/types.ts", &types_head),
            ("src/box.js", &js_head),
            ("tsconfig.json", &cfg),
        ],
    );
    let before = state(&f.engine);
    let r = run(&f, &Default::default()).unwrap();
    let c = context(&r);
    let caller = node_id(c, "caller", "x.run");
    for (class, body, anchor) in [
        ("Box", "IMPORTED_HEAD", "new Imported('s')"),
        ("JsBox", "JS_HEAD", "new JsBox('j')"),
    ] {
        let owner = node_id(c, class, body);
        let target = node_id(c, "run", body);
        let witness = instance_witnesses(c, &owner, anchor, "string");
        assert_eq!(witness.len(), 1);
        assert!(
            edges(c, "possibleDispatch", &caller, &target)
                .iter()
                .any(|e| e["candidateInstance"] == witness[0]["id"])
        );
    }
    assert!(
        edges(
            c,
            "possibleDispatch",
            &caller,
            &node_id(c, "run", "LOCAL_HEAD")
        )
        .is_empty()
    );
    assert_eq!(state(&f.engine), before);
}
#[test]
fn generic_type_displays_are_bounded_without_silent_truncation_or_state_publication() {
    let properties = (0..600)
        .map(|i| format!("p{i}:string;"))
        .collect::<String>();
    let source = format!(
        "export class Box<T>{{run(v:T):T{{return v;}}}}\nlet observed!:Box<{{{properties}}}>;\n"
    );
    let f = configured(&source, &source);
    let before = state(&f.engine);
    let error = run(&f, &Default::default()).unwrap_err().to_string();
    assert!(error.contains("type display capacity"), "{error}");
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
fn property_slots_accessor_effects_and_returned_function_calls_remain_separate() {
    let source = concat!(
        "\u{feff}// π 😀 e\u{301}\r\nexport interface Runner { run(v:string):string; }\r\n",
        "class Field { run = (v:string) => 'FIELD_OLD'+v; }\r\n",
        "class Accessor { get run(): (v:string)=>string { const marker='GET_OLD'; return v=>'RETURNED'+v; } }\r\n",
        "class Wrong { run = (v:number) => v; }\r\n",
        "function via(x:Runner) { x.run('a'); x.run('b'); }\r\n",
        "function direct(x:Accessor) { return x.run('c'); }\r\n"
    );
    let f = configured(source, &source.replace("_OLD", "_HEAD"));
    let before = state(&f.engine);
    let r = run(&f, &Default::default()).unwrap();
    let c = context(&r);
    let caller = node_id(c, "via", "x.run");
    let field = node_id(c, "run", "FIELD_HEAD");
    let getter = node_id(c, "run", "GET_HEAD");
    let wrong = node_id(c, "run", "v:number");
    assert_eq!(edges(c, "possiblePropertyAccess", &caller, &field).len(), 2);
    assert_eq!(edges(c, "possibleGets", &caller, &getter).len(), 2);
    assert!(edges(c, "possiblePropertyAccess", &caller, &wrong).is_empty());
    let direct = node_id(c, "direct", "x.run");
    assert_eq!(edges(c, "gets", &direct, &getter).len(), 1);
    assert!(
        edges(c, "calls", &direct, &getter).is_empty(),
        "Getter body is not the returned function invocation"
    );
    let invocation = c["relations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["relation"] == "calls" && e["source"] == direct)
        .unwrap();
    let signature = c["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == invocation["target"])
        .unwrap();
    assert_eq!(signature["kind"], "FunctionType");
    assert!(
        c["relations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["relation"] == "propertyInitializer" && e["source"] == field)
    );
    assert!(
        !c["relations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["relation"] == "propertyInitializer" && e["source"] == getter)
    );
    for edge in c["relations"].as_array().unwrap() {
        let proof = &edge["proof"];
        let head = source.replace("_OLD", "_HEAD");
        assert_eq!(
            &head[proof["startByte"].as_u64().unwrap() as usize
                ..proof["endByte"].as_u64().unwrap() as usize],
            proof["text"].as_str().unwrap()
        );
    }
    assert!(
        r["states"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["nodeId"] == caller)
    );
    assert_eq!(state(&f.engine), before);
}

#[test]
fn property_read_write_delete_and_optional_chain_effects_preserve_the_original_access() {
    let source = concat!(
        "export class Box { get value():number {return 1;} set value(v:number) {const marker='SET_OLD';} get fn(): (()=>number)|undefined{return ()=>1;} }\n",
        "function effects(x:Box,y:Box|undefined) { x.value; x.value=2; x.value+=1; x.value++; x.value ||= 3; x.value &&= 4; x.value ??= 5; x.fn?.(); y?.fn?.(); y?.fn?.name; ({a:x.value}={a:1}); [x.value]=[2]; for(x.value of [3]){} }\n",
        "interface Optional {value?:number;} function remove(x:Optional){delete x.value;}\n"
    );
    let f = configured(source, &source.replace("SET_OLD", "SET_HEAD"));
    let before = state(&f.engine);
    let r = run(&f, &Default::default()).unwrap();
    let c = context(&r);
    let getter = node_id(c, "value", "return 1");
    let setter = node_id(c, "value", "SET_HEAD");
    let caller = node_id(c, "effects", "x.value");
    let gets = edges(c, "gets", &caller, &getter);
    let sets = edges(c, "sets", &caller, &setter);
    assert_eq!(
        gets.len(),
        6,
        "read, compound, increment and three logical assignments read"
    );
    assert_eq!(
        sets.len(),
        9,
        "assignment, compound, increment, three logical, two patterns and for-of write"
    );
    for edge in gets.iter().chain(sets.iter()) {
        assert_eq!(edge["proof"]["text"], "x.value");
        assert_eq!(edge["conditional"], false);
    }
    assert_eq!(
        sets.iter()
            .filter(|e| e["writeConditional"] == true)
            .count(),
        3
    );
    let fn_getter = node_id(c, "fn", "return ()=>1");
    let optional_gets = edges(c, "gets", &caller, &fn_getter);
    assert_eq!(optional_gets.len(), 3);
    assert_eq!(
        optional_gets
            .iter()
            .filter(|e| e["conditional"] == false)
            .count(),
        1,
        "x.fn?.() still reads fn before the invocation null check"
    );
    let remove = node_id(c, "remove", "delete x.value");
    assert!(
        c["relations"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["source"] == remove)
            .all(|e| e["relation"] != "gets" && e["relation"] != "sets")
    );
    assert!(
        c["relations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["source"] == remove
                && e["relation"] == "accessesProperty"
                && e["accessMode"] == "delete")
    );
    assert_eq!(state(&f.engine), before);
}

#[test]
fn computed_private_static_and_generic_property_slots_use_compiler_identity() {
    let source = concat!(
        "export {};declare const key:unique symbol;const literal='run';\n",
        "interface Runner{run(v:string):string;} class Generic<T>{constructor(public value:T){} run=(v:T):T=>v; [key]=(v:T):T=>v;}\n",
        "const good=new Generic('s');const bad=new Generic(1);\n",
        "class Base {static run=(v:string)=>'BASE_OLD'+v; #fn=()=>1; read(){return this.#fn();}}\n",
        "class Derived extends Base {static override run=(v:string)=>'DERIVED'+v; #fn=()=>2; override read(){return this.#fn();}}\n",
        "function via(x:Runner){x[literal]('x');} function symbolic(x:Generic<string>){return x[key]('s');}\n",
        "function fixed(){return Base.run('s');} function dynamic(C:typeof Base){return C.run('s');}\n"
    );
    let f = configured(source, &source.replace("BASE_OLD", "BASE_HEAD"));
    let r = run(&f, &Default::default()).unwrap();
    let c = context(&r);
    let caller = node_id(c, "via", "x[literal]");
    let field = node_id(c, "run", "v:T");
    let owner = node_id(c, "Generic", "v:T");
    let positive = edges(c, "possiblePropertyAccess", &caller, &field);
    assert!(!positive.is_empty());
    for edge in positive {
        let witness = c["instances"]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["id"] == edge["candidateInstance"])
            .unwrap();
        assert_eq!(witness["owner"], owner);
        assert_eq!(witness["arguments"][0]["display"], "string");
    }
    let symbolic = node_id(c, "symbolic", "x[key]");
    let symbol_field = node_id(c, "[key]", "v:T");
    assert_eq!(
        edges(c, "accessesProperty", &symbolic, &symbol_field).len(),
        1
    );
    let fixed = node_id(c, "fixed", "Base.run");
    let dynamic = node_id(c, "dynamic", "C.run");
    let derived = node_id(c, "run", "DERIVED");
    assert!(edges(c, "possiblePropertyAccess", &fixed, &derived).is_empty());
    assert_eq!(
        edges(c, "possiblePropertyAccess", &dynamic, &derived).len(),
        1
    );
    assert!(
        c["relations"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["relation"] == "possiblePropertyAccess")
            .all(|e| {
                let target = c["nodes"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|n| n["id"] == e["target"])
                    .unwrap();
                !target["name"].as_str().unwrap().starts_with('#')
            })
    );
}

#[test]
fn unsupported_property_values_and_implicit_effects_remain_visible_and_budgeted() {
    let source = concat!(
        "export {};function external(v:string){return v;}class Box{run=external; accessor value=1;get fn(){return external;}}\n",
        "interface Index{[key:string]:number;}function indexed(x:Index,key:string){return x[key];}\n",
        "function implicit(x:Box){const {value}=x;const copy={...x};for(const n of [1]){x.value=n;}return [value,copy];}\n"
    );
    let f = configured(source, &source.replace("value=1", "value=2"));
    let before = state(&f.engine);
    let r = run(&f, &Default::default()).unwrap();
    let c = context(&r);
    for reason in [
        "callable_property_initializer_not_direct_function",
        "returned_accessor_function_values_not_enumerated",
        "auto_accessor_implicit_bodies_not_enumerated",
        "property_key_not_finite_compiler_literal",
        "implicit_destructuring_or_spread_effects_not_enumerated",
        "implicit_iteration_effects_not_enumerated",
    ] {
        assert!(
            c["unavailable"]
                .as_array()
                .unwrap()
                .iter()
                .any(|u| u["reason"] == reason),
            "Missing coverage: {reason}"
        );
    }
    let indexed = node_id(c, "indexed", "x[key]");
    assert!(
        !c["relations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["source"] == indexed && e["relation"] == "accessesProperty")
    );
    assert!(
        run(
            &f,
            &Options {
                max_relations: 1,
                ..Default::default()
            }
        )
        .is_err()
    );
    assert_eq!(r["complete"], false);
    assert_eq!(state(&f.engine), before);
}

#[test]
fn forged_property_effects_instances_and_initializer_proofs_refuse_without_state_changes() {
    let source = "export interface Runner{run(v:string):string;} class Field{run=(v:string)=>v;} class Accessor{get run(){const marker='OLD';return(v:string)=>v;}} class Generic<T>{constructor(public value:T){}run=(v:T)=>v;} const observed=new Generic('s');function via(x:Runner){return x.run('s');} function direct(x:Accessor){return x.run('d');}\n";
    let f = configured(source, &source.replace("'OLD'", "'HEAD'"));
    let before = state(&f.engine);
    let host = tempfile::tempdir().unwrap();
    let mut selected = spec();
    let actual_path = selected.typescript.to_string_lossy();
    let actual = actual_path.strip_prefix(r"\\?\").unwrap_or(&actual_path);
    let module = host.path().join("owned-adversarial-compiler.cjs");
    let prefix = format!(
        "const ts=require({});Object.defineProperty(Object.prototype,'toJSON',{{configurable:true,value:function(){{const report=this;if(report.schema===1&&report.operation==='hierarchy'){{for(const c of report.contexts||[]){{const h=c.hierarchy;if(!h)continue;",
        serde_json::to_string(actual).unwrap()
    );
    let suffix = "}}return report;}});module.exports=ts;";
    for (mutation, expected) in [
        (
            "const direct=h.nodes.findIndex(n=>n.name==='direct');h.relations=h.relations.filter(e=>e.relation!=='accessesProperty'||e.source!==direct);",
            "Invalid direct property binding",
        ),
        (
            "for(const e of h.relations)if(e.relation==='possibleGets')e.accessMode='delete';",
            "Invalid compiler possible property binding",
        ),
        (
            "for(const e of h.relations)if(e.relation==='possiblePropertyAccess')e.candidateInstance=999999;",
            "Invalid compiler instance reference",
        ),
        (
            "for(const e of h.relations)if(e.relation==='propertyInitializer')e.proof=h.nodes.find(n=>n.kind==='GetAccessor').selection;",
            "Invalid captured property initializer",
        ),
    ] {
        fs::write(&module, format!("{prefix}{mutation}{suffix}")).unwrap();
        selected.typescript = module.clone();
        let error = match compiler_affected::derive(
            &f.engine.store,
            f.report["artifactId"].as_str().unwrap(),
            &selected,
            &Default::default(),
            &Deadline::new(60000),
        ) {
            Err(error) => error.to_string(),
            Ok(_) => panic!("Forged report accepted; expected {expected}"),
        };
        assert!(
            error.contains(expected),
            "Expected {expected}, received {error}"
        );
        assert_eq!(state(&f.engine), before);
    }
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
