use std::collections::BTreeMap;
use yoke::{
    analysis,
    capture::Deadline,
    model::{Graph, Snapshot, SourceFile, hash},
};

fn graph(files: &[(&str, &str, &str)]) -> Graph {
    let source_files: Vec<_> = files
        .iter()
        .map(|(path, language, text)| SourceFile {
            path: (*path).into(),
            language: (*language).into(),
            hash: hash(text.as_bytes()),
            size: text.len(),
            mode: 0o644,
        })
        .collect();
    let snapshot = Snapshot {
        id: "a".repeat(64),
        workspace_id: "b".repeat(64),
        kind: "saved".into(),
        files: source_files.clone(),
        omissions: vec![],
        capture_consistency: "test_source".into(),
    };
    let deadline = Deadline::new(10000);
    let extractions = source_files
        .iter()
        .zip(files)
        .map(|(file, (_, _, text))| analysis::extract(file, text, &deadline).unwrap())
        .collect();
    analysis::assemble(&snapshot, extractions, &deadline).unwrap()
}

fn targets<'a>(
    graph: &'a Graph,
    relation: &str,
    source: &str,
    anchor: &str,
    texts: &BTreeMap<&str, &str>,
) -> Vec<&'a yoke::model::Entity> {
    graph
        .observations
        .iter()
        .filter(|edge| edge.relation == relation)
        .filter(|edge| {
            let entity = graph
                .entities
                .iter()
                .find(|entity| entity.id == edge.source)
                .unwrap();
            if entity.name != source {
                return false;
            }
            let evidence = graph
                .evidence
                .iter()
                .find(|e| edge.evidence_ids.contains(&e.id))
                .unwrap();
            &texts[evidence.path.as_str()][evidence.start_byte..evidence.end_byte] == anchor
        })
        .filter_map(|edge| edge.target.as_ref())
        .map(|id| graph.entities.iter().find(|e| &e.id == id).unwrap())
        .collect()
}

#[test]
fn multiline_signatures_preserve_the_complete_header_without_the_body() {
    let rust = "pub fn calculate(\n    amount: i64,\n    tax: i64,\n) -> i64 { amount + tax }\n";
    let python =
        "def calculate(\n    amount: int,\n    tax: int,\n) -> int:\n    return amount + tax\n";
    let g = graph(&[("a.rs", "rust", rust), ("a.py", "python", python)]);
    let r = g
        .entities
        .iter()
        .find(|e| e.path == "a.rs" && e.name == "calculate")
        .unwrap();
    assert_eq!(
        r.signature,
        "pub fn calculate(\n    amount: i64,\n    tax: i64,\n) -> i64"
    );
    let p = g
        .entities
        .iter()
        .find(|e| e.path == "a.py" && e.name == "calculate")
        .unwrap();
    assert_eq!(
        p.signature,
        "def calculate(\n    amount: int,\n    tax: int,\n) -> int:"
    );
}

#[test]
fn imported_aliases_select_the_declared_module_instead_of_another_same_named_symbol() {
    let ts = "import { run as execute } from './worker'; export function main() { execute(); }";
    let py = "from worker import run as execute\ndef main():\n    execute()\n";
    let rs = "mod worker; use crate::worker::run as execute; fn main(){ execute(); }";
    let g = graph(&[
        ("main.ts", "typescript", ts),
        ("worker.ts", "typescript", "export function run(){}"),
        ("other.ts", "typescript", "export function run(){}"),
        ("main.py", "python", py),
        ("worker.py", "python", "def run():\n    pass\n"),
        ("other.py", "python", "def run():\n    pass\n"),
        ("main.rs", "rust", rs),
        ("worker.rs", "rust", "pub fn run(){}"),
        ("other.rs", "rust", "pub fn run(){}"),
    ]);
    let texts = BTreeMap::from([("main.ts", ts), ("main.py", py), ("main.rs", rs)]);
    let mut paths: Vec<_> = targets(&g, "calls", "main", "execute()", &texts)
        .into_iter()
        .map(|e| e.path.as_str())
        .collect();
    paths.sort();
    assert_eq!(paths, ["worker.py", "worker.rs", "worker.ts"]);
}

#[test]
fn namespace_import_member_calls_and_non_call_references_keep_exact_utf8_anchors() {
    let ts = "// 🌍\nimport * as api from './worker';\nfunction main(){ const café = api.run; api.run(); }";
    let g = graph(&[
        ("main.ts", "typescript", ts),
        ("worker.ts", "typescript", "export function run(){}"),
    ]);
    let texts = BTreeMap::from([("main.ts", ts)]);
    assert_eq!(
        targets(&g, "calls", "main", "api.run()", &texts)[0].path,
        "worker.ts"
    );
    assert_eq!(
        targets(&g, "references", "main", "api.run", &texts)[0].name,
        "run"
    );
    for e in &g.evidence {
        if e.path == "main.ts" {
            assert!(ts.is_char_boundary(e.start_byte) && ts.is_char_boundary(e.end_byte));
        }
    }
    assert!(
        g.observations
            .iter()
            .all(|e| e.resolution != "exact_binding")
    );
}

#[test]
fn typed_receivers_do_not_cross_bind_methods_with_the_same_name() {
    let ts = "class Alpha { save(){} } class Beta { save(){} } function main(){const a: Alpha = new Alpha(); const b = new Beta(); a.save(); b.save();}";
    let py = "class Alpha:\n    def save(self):\n        pass\nclass Beta:\n    def save(self):\n        pass\ndef main(a: Alpha):\n    b = Beta()\n    a.save()\n    b.save()\n";
    let cpp = "struct Alpha { void save() {} }; struct Beta { void save() {} }; void main(){Alpha a; Beta b; a.save(); b.save();}";
    let g = graph(&[
        ("a.ts", "typescript", ts),
        ("a.py", "python", py),
        ("a.cpp", "cpp", cpp),
    ]);
    let texts = BTreeMap::from([("a.ts", ts), ("a.py", py), ("a.cpp", cpp)]);
    for (anchor, want) in [("a.save()", "Alpha"), ("b.save()", "Beta")] {
        let candidates = targets(&g, "calls", "main", anchor, &texts);
        assert_eq!(candidates.len(), 3, "{anchor}: {candidates:?}");
        for target in candidates {
            let parent = g
                .entities
                .iter()
                .find(|e| Some(&e.key) == target.parent_key.as_ref())
                .unwrap();
            assert_eq!(parent.name, want, "{target:?}");
        }
    }
}

#[test]
fn lexical_shadowing_and_reassignment_block_unique_global_name_guesses() {
    let ts = "function execute(){} function main(){execute(); { const execute = 42; execute(); } execute();} function parameter(execute:unknown){execute();} class Alpha{ save(){} } function rebound(){let a = new Alpha(); a = unknown(); a.save();}";
    let g = graph(&[("a.ts", "typescript", ts)]);
    let texts = BTreeMap::from([("a.ts", ts)]);
    assert_eq!(targets(&g, "calls", "main", "execute()", &texts).len(), 2);
    assert!(targets(&g, "calls", "parameter", "execute()", &texts).is_empty());
    assert!(targets(&g, "calls", "rebound", "a.save()", &texts).is_empty());
    let unknowns: Vec<_> = g
        .observations
        .iter()
        .filter(|e| e.relation == "calls" && e.target_label == "execute" && e.target.is_none())
        .collect();
    assert_eq!(unknowns.len(), 2);
}

#[test]
fn union_receiver_candidates_preserve_parallel_possible_dispatch() {
    let ts = "class Alpha{save(){}} class Beta{save(){}} function main(a:Alpha|Beta){a.save();}";
    let g = graph(&[("a.ts", "typescript", ts)]);
    let texts = BTreeMap::from([("a.ts", ts)]);
    let candidates = targets(&g, "calls", "main", "a.save()", &texts);
    assert_eq!(candidates.len(), 2);
    assert!(
        g.observations
            .iter()
            .filter(|e| e.relation == "calls")
            .all(|e| e.resolution == "heuristic")
    );
}

#[test]
fn extends_and_implements_remain_distinct_and_inherited_member_calls_are_candidates() {
    let ts = "interface Contract{save():void;} class Base{save(){}} class Derived extends Base implements Contract{main(){this.save();}}";
    let g = graph(&[("a.ts", "typescript", ts)]);
    let texts = BTreeMap::from([("a.ts", ts)]);
    assert_eq!(
        targets(&g, "extends", "Derived", "Base", &texts)[0].name,
        "Base"
    );
    assert_eq!(
        targets(&g, "implements", "Derived", "Contract", &texts)[0].name,
        "Contract"
    );
    let candidate = targets(&g, "calls", "main", "this.save()", &texts);
    assert_eq!(candidate.len(), 1);
    let owner = g
        .entities
        .iter()
        .find(|e| Some(&e.key) == candidate[0].parent_key.as_ref())
        .unwrap();
    assert_eq!(owner.name, "Base");
}

#[test]
fn nested_functions_reference_the_nearest_lexical_function() {
    let ts = "function helper(){} function outer(){function helper(){} function inner(){const f=helper; helper();} }";
    let g = graph(&[("a.ts", "typescript", ts)]);
    let texts = BTreeMap::from([("a.ts", ts)]);
    for (relation, anchor) in [("references", "helper"), ("calls", "helper()")] {
        let candidate = targets(&g, relation, "inner", anchor, &texts);
        assert_eq!(candidate.len(), 1);
        assert!(candidate[0].qualified_name.contains("outer/helper"));
    }
}

#[test]
fn rust_impl_receivers_and_explicit_type_paths_bind_to_the_same_method_owner() {
    let rs = "struct Alpha; impl Alpha { fn save(&self) {} fn main(&self){self.save();} } fn caller(a:&Alpha){a.save(); Alpha::save(&a);}";
    let g = graph(&[("a.rs", "rust", rs)]);
    let texts = BTreeMap::from([("a.rs", rs)]);
    for (source, anchor) in [
        ("main", "self.save()"),
        ("caller", "a.save()"),
        ("caller", "Alpha::save(&a)"),
    ] {
        let candidates = targets(&g, "calls", source, anchor, &texts);
        assert_eq!(candidates.len(), 1, "{source}/{anchor}: {candidates:?}");
        assert_eq!(candidates[0].name, "save");
        let parent = g
            .entities
            .iter()
            .find(|e| Some(&e.key) == candidates[0].parent_key.as_ref())
            .unwrap();
        assert_eq!(parent.name, "Alpha");
    }
}

#[test]
fn imported_receiver_types_and_constructor_fields_stay_in_the_imported_class() {
    let ts = "import {Client as Service} from './worker'; class Controller{service:Service; main(){this.service.send();}}";
    let py = "class Client:\n    def send(self):\n        pass\nclass Controller:\n    def __init__(self):\n        self.service = Client()\n    def main(self):\n        self.service.send()\n";
    let g = graph(&[
        ("a.ts", "typescript", ts),
        ("worker.ts", "typescript", "export class Client{send(){}}"),
        ("a.py", "python", py),
    ]);
    let texts = BTreeMap::from([("a.ts", ts), ("a.py", py)]);
    for anchor in ["this.service.send()", "self.service.send()"] {
        let candidates = targets(&g, "calls", "main", anchor, &texts);
        assert_eq!(candidates.len(), 1, "{anchor}: {candidates:?}");
        let parent = g
            .entities
            .iter()
            .find(|e| Some(&e.key) == candidates[0].parent_key.as_ref())
            .unwrap();
        assert_eq!(parent.name, "Client");
    }
}

#[test]
fn a_rebound_receiver_in_a_conditional_branch_cannot_keep_a_definite_type_candidate() {
    let ts = "class Alpha{save(){}} function main(a:Alpha){if(flag){a=unknown();} a.save();}";
    let py = "class Alpha:\n    def save(self):\n        pass\ndef main(a:Alpha):\n    if flag:\n        a=unknown()\n    a.save()\n";
    let g = graph(&[("a.ts", "typescript", ts), ("a.py", "python", py)]);
    let texts = BTreeMap::from([("a.ts", ts), ("a.py", py)]);
    assert!(targets(&g, "calls", "main", "a.save()", &texts).is_empty());
}

#[test]
fn identical_methods_in_separate_namespaces_do_not_cross_bind() {
    let cpp = "namespace one {struct Client{void save(){}};} namespace two {struct Client{void save(){}};} void main(one::Client c){c.save();}";
    let g = graph(&[("a.cpp", "cpp", cpp)]);
    let texts = BTreeMap::from([("a.cpp", cpp)]);
    let candidates = targets(&g, "calls", "main", "c.save()", &texts);
    assert_eq!(candidates.len(), 1, "{candidates:?}");
    assert!(candidates[0].qualified_name.contains("one/Client/save"));
}

#[test]
fn shared_inheritance_paths_cannot_amplify_work_past_the_original_request_budget() {
    let mut source = "interface A0{save():void;} interface A1 extends A0{} ".to_string();
    for i in 2..12 {
        source.push_str(&format!(
            "interface A{i} extends A{},A{}{{}} ",
            i - 1,
            i - 2
        ));
    }
    source.push_str("function main(a:A11){a.save();}");
    let file = SourceFile {
        path: "a.ts".into(),
        language: "typescript".into(),
        hash: hash(source.as_bytes()),
        size: source.len(),
        mode: 0o644,
    };
    let extraction = analysis::extract(&file, &source, &Deadline::new(10000)).unwrap();
    let snapshot = Snapshot {
        id: "a".repeat(64),
        workspace_id: "b".repeat(64),
        kind: "saved".into(),
        files: vec![file],
        omissions: vec![],
        capture_consistency: "test_source".into(),
    };
    let start = std::time::Instant::now();
    let result = analysis::assemble(&snapshot, vec![extraction], &Deadline::new(10));
    assert!(
        start.elapsed() < std::time::Duration::from_millis(250),
        "resolution exceeded request budget: {:?}",
        start.elapsed()
    );
    if let Ok(graph) = result {
        assert!(
            graph
                .observations
                .iter()
                .any(|e| e.relation == "calls" && e.target.is_some())
        );
    }
}

#[test]
fn named_reexports_follow_the_captured_module_chain() {
    let main = "import {task as execute} from './bridge'; function main(){execute();}";
    let g = graph(&[
        ("main.ts", "typescript", main),
        (
            "bridge.ts",
            "typescript",
            "export {run as task} from './worker';",
        ),
        ("worker.ts", "typescript", "export function run(){}"),
        ("other.ts", "typescript", "export function task(){}"),
    ]);
    let texts = BTreeMap::from([("main.ts", main)]);
    let found = targets(&g, "calls", "main", "execute()", &texts);
    assert_eq!(
        found.iter().map(|e| e.path.as_str()).collect::<Vec<_>>(),
        ["worker.ts"]
    );
}

#[test]
fn grouped_rust_imports_can_select_symbols_from_inline_modules() {
    let rs =
        "mod worker{pub fn run(){}} use crate::worker::{run as execute}; fn main(){execute();}";
    let g = graph(&[("main.rs", "rust", rs)]);
    let texts = BTreeMap::from([("main.rs", rs)]);
    let found = targets(&g, "calls", "main", "execute()", &texts);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].qualified_name, "main.rs/worker/run");
}

#[test]
fn destructured_locals_cannot_be_replaced_by_an_unrelated_global_function() {
    let ts = "function run(){} function main(){const {run}=unknown; run();}";
    let py = "def run():\n    pass\ndef main():\n    run,other=unknown\n    run()\n";
    let g = graph(&[("a.ts", "typescript", ts), ("a.py", "python", py)]);
    let texts = BTreeMap::from([("a.ts", ts), ("a.py", py)]);
    assert!(targets(&g, "calls", "main", "run()", &texts).is_empty());
}

#[test]
fn keyword_argument_and_object_key_spelling_are_not_symbol_references() {
    let py = "def run():\n    pass\ndef main():\n    unknown(run=1)\n";
    let g = graph(&[("a.py", "python", py)]);
    let texts = BTreeMap::from([("a.py", py)]);
    assert!(targets(&g, "references", "main", "run", &texts).is_empty());
}

#[test]
fn java_explicit_receiver_cannot_bind_to_the_callers_own_same_named_method() {
    let java = "class Alpha{void save(){} void main(Beta b){b.save();}} class Beta{void save(){}}";
    let g = graph(&[("A.java", "java", java)]);
    let texts = BTreeMap::from([("A.java", java)]);
    let found = targets(&g, "calls", "main", "b.save()", &texts);
    assert_eq!(found.len(), 1);
    let parent = g
        .entities
        .iter()
        .find(|e| Some(&e.key) == found[0].parent_key.as_ref())
        .unwrap();
    assert_eq!(parent.name, "Beta");
}

#[test]
fn explicit_interface_method_declarations_are_candidates_with_exact_signatures() {
    let ts = "interface Store{save(value:string):void;} function main(s:Store){s.save('x');}";
    let g = graph(&[("a.ts", "typescript", ts)]);
    let texts = BTreeMap::from([("a.ts", ts)]);
    let found = targets(&g, "calls", "main", "s.save('x')", &texts);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].signature, "save(value:string):void");
}

#[test]
fn relative_module_paths_that_escape_the_captured_root_remain_unresolved() {
    let py = "from ...worker import run as execute\ndef main():\n    execute()\n";
    let rs = "use super::super::worker::run as execute; fn main(){execute();}";
    let g = graph(&[
        ("main.py", "python", py),
        ("worker.py", "python", "def run():\n    pass\n"),
        ("main.rs", "rust", rs),
        ("worker.rs", "rust", "fn run(){}"),
    ]);
    let texts = BTreeMap::from([("main.py", py), ("main.rs", rs)]);
    assert!(targets(&g, "calls", "main", "execute()", &texts).is_empty());
}
#[test]
fn rust_future_non_hoisted_let_binding_does_not_shadow_an_earlier_call() {
    let text = "fn foo(){} fn main(){foo(); let foo=||1;foo();}";
    let g = graph(&[("a.rs", "rust", text)]);
    let texts = BTreeMap::from([("a.rs", text)]);
    assert_eq!(targets(&g, "calls", "main", "foo()", &texts).len(), 1);
    assert_eq!(
        g.observations
            .iter()
            .filter(|e| e.relation == "calls" && e.target.is_none())
            .count(),
        1
    );
}
#[test]
fn embedded_typescript_imports_bind_using_the_embedded_language_family() {
    let text = "<template>🌍</template><script lang=\"ts\">import {run as execute} from './worker'; function main(){execute();}</script>";
    let g = graph(&[
        ("main.vue", "vue", text),
        ("worker.ts", "typescript", "export function run(){}"),
        ("other.ts", "typescript", "export function run(){}"),
    ]);
    let texts = BTreeMap::from([("main.vue", text)]);
    let targets = targets(&g, "calls", "main", "execute()", &texts);
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].path, "worker.ts");
}

#[test]
fn containers_and_function_return_annotations_cannot_be_used_as_receiver_types() {
    let ts = "class Alpha{save(){}} function main(items:Alpha[],factory:()=>Alpha){items.save();factory.save();}";
    let py = "class Alpha:\n    def save(self):\n        pass\ndef main(items:list[Alpha]):\n    items.save()\n";
    let g = graph(&[("a.ts", "typescript", ts), ("a.py", "python", py)]);
    let texts = BTreeMap::from([("a.ts", ts), ("a.py", py)]);
    assert!(targets(&g, "calls", "main", "items.save()", &texts).is_empty());
    assert!(targets(&g, "calls", "main", "factory.save()", &texts).is_empty());
}
