//! Bounded structural dataflow candidates. No compiler binding or source execution.
use crate::{
    capture::Deadline,
    model::{Graph, Snapshot, SourceFile, identity},
    paths::line_starts,
    store::Store,
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use tree_sitter::{Node, ParseOptions, Parser};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Policy {
    pub sources: Vec<String>,
    pub sinks: BTreeMap<String, Vec<usize>>,
    pub sanitizers: Vec<String>,
    #[serde(default = "iterations")]
    pub max_iterations: usize,
    #[serde(default = "nodes")]
    pub max_nodes: usize,
    #[serde(default = "records")]
    pub max_records: usize,
}
fn iterations() -> usize {
    32
}
fn nodes() -> usize {
    100_000
}
fn records() -> usize {
    1000
}
impl Default for Policy {
    fn default() -> Self {
        Self {
            sources: vec![
                "req.query".into(),
                "req.body".into(),
                "req.params".into(),
                "location.search".into(),
            ],
            sinks: BTreeMap::from([
                ("eval".into(), vec![0]),
                ("exec".into(), vec![0]),
                ("query".into(), vec![0]),
            ]),
            sanitizers: vec![],
            max_iterations: iterations(),
            max_nodes: nodes(),
            max_records: records(),
        }
    }
}
type Taint = BTreeSet<String>;
#[derive(Clone, Debug, PartialEq, Eq)]
struct Scope {
    function: bool,
    values: BTreeMap<String, Taint>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Env(Vec<Scope>);
impl Env {
    fn new() -> Self {
        Self(vec![Scope {
            function: true,
            values: BTreeMap::new(),
        }])
    }
    fn push(&mut self, function: bool) {
        self.0.push(Scope {
            function,
            values: BTreeMap::new(),
        });
    }
    fn declare(&mut self, name: &str, value: Taint, var: bool) {
        let index = if var {
            self.0.iter().rposition(|s| s.function).unwrap_or(0)
        } else {
            self.0.len() - 1
        };
        self.0[index].values.insert(name.into(), value);
    }
    fn get(&self, name: &str) -> Taint {
        self.0
            .iter()
            .rev()
            .find_map(|s| s.values.get(name))
            .cloned()
            .unwrap_or_default()
    }
    fn assign(&mut self, name: &str, value: Taint) {
        let index = self
            .0
            .iter()
            .rposition(|s| s.values.contains_key(name))
            .unwrap_or(0);
        self.0[index].values.insert(name.into(), value);
    }
    fn join(mut self, other: Self) -> Self {
        for (a, b) in self.0.iter_mut().zip(other.0) {
            for (k, v) in b.values {
                a.values.entry(k).or_default().extend(v);
            }
        }
        self
    }
}
fn children(node: Node<'_>) -> Vec<Node<'_>> {
    let mut c = node.walk();
    node.named_children(&mut c).collect()
}
fn function(node: Node<'_>) -> bool {
    matches!(
        node.kind(),
        "function_declaration"
            | "function_expression"
            | "generator_function"
            | "generator_function_declaration"
            | "arrow_function"
            | "method_definition"
    )
}
struct Report {
    findings: BTreeMap<String, Value>,
    sources: BTreeMap<String, Value>,
    unknown: BTreeSet<String>,
    unsupported: BTreeMap<String, Value>,
    bounded: bool,
    nodes: usize,
    bytes: usize,
}
struct Visitor<'a> {
    snapshot: &'a Snapshot,
    file: &'a SourceFile,
    graph: &'a Graph,
    text: &'a str,
    lines: Vec<usize>,
    policy: &'a Policy,
    deadline: &'a Deadline,
    report: &'a mut Report,
}
impl Visitor<'_> {
    fn tick(&mut self, depth: usize) -> Result<bool> {
        self.deadline.check()?;
        self.report.nodes += 1;
        if self.report.nodes > self.policy.max_nodes || depth > 128 {
            self.report.bounded = true;
            return Ok(false);
        }
        Ok(true)
    }
    fn raw(&self, n: Node<'_>) -> &str {
        &self.text[n.start_byte()..n.end_byte()]
    }
    fn anchor(&self, n: Node<'_>) -> Value {
        let start = n.start_byte();
        let end = n.end_byte();
        let line = |p| self.lines.partition_point(|s| *s <= p);
        let entity = self
            .graph
            .entities
            .iter()
            .filter(|e| e.path == self.file.path && e.start_byte <= start && e.end_byte >= end)
            .min_by_key(|e| e.end_byte - e.start_byte)
            .map(|e| &e.id);
        json!({"snapshotId":self.snapshot.id,"path":self.file.path,"contentHash":self.file.hash,"startByte":start,"endByte":end,"startLine":line(start),"endLine":line(end.saturating_sub(1).max(start)),"anchor":"exact_source_bytes","entityId":entity})
    }
    fn note(&mut self, n: Node<'_>, reason: &str) {
        if self.report.unsupported.len() >= 100 {
            self.report.bounded = true;
            return;
        }
        let key = identity(&(&self.file.path, n.start_byte(), reason));
        let mut v = self.anchor(n);
        v["construct"] = json!(n.kind());
        v["reason"] = json!(reason);
        self.report.unsupported.entry(key).or_insert(v);
    }
    fn insert(&mut self, key: String, value: Value, source: bool) -> Result<bool> {
        let map = if source {
            &mut self.report.sources
        } else {
            &mut self.report.findings
        };
        let old = map
            .get(&key)
            .map(serde_json::to_vec)
            .transpose()?
            .map_or(0, |b| b.len());
        let new = serde_json::to_vec(&value)?.len();
        if (!map.contains_key(&key) && map.len() >= self.policy.max_records)
            || self.report.bytes.saturating_sub(old) + new > 4_000_000
        {
            self.report.bounded = true;
            return Ok(false);
        }
        self.report.bytes = self.report.bytes.saturating_sub(old) + new;
        map.insert(key, value);
        Ok(true)
    }
    fn selector(&self, n: Node<'_>, depth: usize) -> Option<String> {
        if depth > 128 {
            return None;
        }
        match n.kind() {
            "identifier" | "property_identifier" => Some(self.raw(n).into()),
            "member_expression" => Some(format!(
                "{}.{}",
                self.selector(n.child_by_field_name("object")?, depth + 1)?,
                self.raw(n.child_by_field_name("property")?)
            )),
            "subscript_expression" => {
                let base = self.selector(n.child_by_field_name("object")?, depth + 1)?;
                let index = n.child_by_field_name("index")?;
                if index.kind() == "string" {
                    let value = self.raw(index);
                    Some(format!(
                        "{}.{}",
                        base,
                        &value[1..value.len().saturating_sub(1)]
                    ))
                } else {
                    Some(format!("{base}[*]"))
                }
            }
            _ => None,
        }
    }
    fn bind(
        &mut self,
        n: Node<'_>,
        values: &Taint,
        env: &mut Env,
        var: bool,
        depth: usize,
    ) -> Result<()> {
        if !self.tick(depth)? {
            return Ok(());
        }
        if matches!(
            n.kind(),
            "identifier" | "shorthand_property_identifier_pattern"
        ) {
            env.declare(self.raw(n), values.clone(), var);
        } else {
            if matches!(
                n.kind(),
                "object_pattern" | "array_pattern" | "rest_pattern"
            ) {
                self.note(
                    n,
                    "destructuring and rest bindings conservatively share input taint",
                );
            }
            for c in children(n) {
                if !matches!(
                    c.kind(),
                    "type_annotation" | "type_identifier" | "property_identifier"
                ) {
                    self.bind(c, values, env, var, depth + 1)?;
                }
            }
        }
        Ok(())
    }
    fn predeclare(&mut self, n: Node<'_>, env: &mut Env, vars: bool, depth: usize) -> Result<()> {
        if !self.tick(depth)? {
            return Ok(());
        }
        for c in children(n) {
            if function(c) {
                if !vars && let Some(name) = c.child_by_field_name("name") {
                    env.declare(self.raw(name), Taint::new(), false);
                }
                continue;
            }
            let declaration = matches!(c.kind(), "lexical_declaration" | "variable_declaration");
            let is_var = c.kind() == "variable_declaration";
            if declaration && is_var == vars {
                for d in children(c) {
                    if d.kind() == "variable_declarator"
                        && let Some(name) = d.child_by_field_name("name")
                    {
                        self.bind(name, &Taint::new(), env, is_var, depth + 1)?;
                    }
                }
            }
            if vars {
                self.predeclare(c, env, true, depth + 1)?;
            }
        }
        Ok(())
    }
    fn analyze_function(&mut self, n: Node<'_>, env: &Env, depth: usize) -> Result<()> {
        let Some(body) = n.child_by_field_name("body") else {
            return Ok(());
        };
        let mut local = env.clone();
        local.push(true);
        if let Some(params) = n
            .child_by_field_name("parameters")
            .or_else(|| n.child_by_field_name("parameter"))
        {
            self.bind(params, &Taint::new(), &mut local, false, depth + 1)?;
        }
        self.predeclare(body, &mut local, true, depth + 1)?;
        self.flow(body, &mut local, depth + 1)?;
        Ok(())
    }
    fn eval(&mut self, n: Node<'_>, env: &mut Env, depth: usize) -> Result<Taint> {
        if !self.tick(depth)? {
            return Ok(Taint::new());
        }
        if function(n) {
            self.analyze_function(n, env, depth + 1)?;
            return Ok(Taint::new());
        }
        if let Some(selector) = self.selector(n, 0)
            && let Some(pattern) = self.policy.sources.iter().find(|p| {
                selector == **p
                    || selector.starts_with(&format!("{p}."))
                    || selector.starts_with(&format!("{p}["))
            })
        {
            let id = identity(&(
                &self.snapshot.id,
                &self.file.path,
                n.start_byte(),
                n.end_byte(),
            ));
            let mut item = self.anchor(n);
            item["id"] = json!(id);
            item["selector"] = json!(pattern);
            if self.insert(id.clone(), item, true)? {
                return Ok(BTreeSet::from([id]));
            }
            return Ok(Taint::new());
        }
        match n.kind() {
            "identifier" | "shorthand_property_identifier" => Ok(env.get(self.raw(n))),
            "variable_declarator" => {
                let value = if let Some(v) = n.child_by_field_name("value") {
                    self.eval(v, env, depth + 1)?
                } else {
                    Taint::new()
                };
                if let Some(name) = n.child_by_field_name("name") {
                    self.bind(name, &value, env, false, depth + 1)?;
                }
                Ok(value)
            }
            "assignment_expression" | "augmented_assignment_expression" => {
                let mut values = if let Some(v) = n.child_by_field_name("right") {
                    self.eval(v, env, depth + 1)?
                } else {
                    Taint::new()
                };
                if let Some(left) = n.child_by_field_name("left") {
                    if left.kind() == "identifier" {
                        if n.kind() == "augmented_assignment_expression" {
                            values.extend(env.get(self.raw(left)));
                        }
                        env.assign(self.raw(left), values.clone());
                    } else {
                        self.note(n, "heap/property writes and aliases are not modeled");
                    }
                }
                Ok(values)
            }
            "call_expression" | "new_expression" => {
                let callee = n
                    .child_by_field_name("function")
                    .or_else(|| n.child_by_field_name("constructor"));
                let name = callee.and_then(|c| self.selector(c, 0)).unwrap_or_else(|| {
                    callee.map_or("<dynamic>".into(), |c| self.raw(c).to_string())
                });
                let mut args = Vec::new();
                if let Some(arguments) = n.child_by_field_name("arguments") {
                    for a in children(arguments) {
                        args.push(self.eval(a, env, depth + 1)?);
                    }
                }
                if let Some(indices) = self.policy.sinks.get(&name) {
                    let values: Taint = indices
                        .iter()
                        .flat_map(|i| args.get(*i).into_iter().flatten().cloned())
                        .collect();
                    if !values.is_empty() {
                        let key = identity(&(&self.file.path, n.start_byte(), &name));
                        let mut item = self.anchor(n);
                        let mut combined = values;
                        if let Some(old) = self.report.findings.get(&key)
                            && let Some(ids) = old["sourceIds"].as_array()
                        {
                            combined
                                .extend(ids.iter().filter_map(Value::as_str).map(str::to_string));
                        }
                        item["id"] = json!(key);
                        item["rule"] = json!(format!("taint:{name}"));
                        item["sink"] = json!(name);
                        item["sourceIds"] = json!(combined);
                        item["resolution"] = json!("heuristic_dataflow");
                        item["claim"] = json!("candidate_not_proven_vulnerability");
                        self.insert(key, item, false)?;
                    }
                }
                if self.policy.sanitizers.contains(&name) {
                    return Ok(Taint::new());
                }
                if !self.policy.sinks.contains_key(&name) {
                    self.report
                        .unknown
                        .insert(identity(&(&self.file.path, n.start_byte())));
                }
                let mut values: Taint = args.into_iter().flatten().collect();
                if let Some(c) = callee
                    && matches!(c.kind(), "member_expression" | "subscript_expression")
                {
                    values.extend(self.eval(c, env, depth + 1)?);
                }
                Ok(values)
            }
            "ternary_expression" => {
                if let Some(c) = n.child_by_field_name("condition") {
                    self.eval(c, env, depth + 1)?;
                }
                let mut a = env.clone();
                let mut b = env.clone();
                let mut values = Taint::new();
                if let Some(c) = n.child_by_field_name("consequence") {
                    values.extend(self.eval(c, &mut a, depth + 1)?);
                }
                if let Some(c) = n.child_by_field_name("alternative") {
                    values.extend(self.eval(c, &mut b, depth + 1)?);
                }
                *env = a.join(b);
                Ok(values)
            }
            "binary_expression" => {
                let mut values = Taint::new();
                if let Some(left) = n.child_by_field_name("left") {
                    values.extend(self.eval(left, env, depth + 1)?);
                }
                let before = env.clone();
                if let Some(right) = n.child_by_field_name("right") {
                    values.extend(self.eval(right, env, depth + 1)?);
                }
                if let Some(operator) = n.child_by_field_name("operator")
                    && matches!(self.raw(operator), "&&" | "||" | "??")
                {
                    *env = before.join(env.clone());
                }
                Ok(values)
            }
            "string" | "number" | "true" | "false" | "null" | "undefined" | "type_annotation"
            | "type_identifier" => Ok(Taint::new()),
            _ => {
                if matches!(
                    n.kind(),
                    "await_expression"
                        | "yield_expression"
                        | "jsx_element"
                        | "jsx_self_closing_element"
                        | "subscript_expression"
                        | "spread_element"
                        | "ERROR"
                ) {
                    self.note(
                        n,
                        "unsupported dynamic/runtime construct; child taint is conservative",
                    );
                }
                let mut values = Taint::new();
                for c in children(n) {
                    values.extend(self.eval(c, env, depth + 1)?);
                }
                Ok(values)
            }
        }
    }
    fn flow(&mut self, n: Node<'_>, env: &mut Env, depth: usize) -> Result<()> {
        if !self.tick(depth)? {
            return Ok(());
        }
        if function(n) {
            return self.analyze_function(n, env, depth + 1);
        }
        match n.kind() {
            "program" | "statement_block" => {
                let block = n.kind() == "statement_block";
                if block {
                    env.push(false);
                }
                self.predeclare(n, env, false, depth + 1)?;
                for c in children(n) {
                    self.flow(c, env, depth + 1)?;
                }
                if block {
                    env.0.pop();
                }
            }
            "lexical_declaration" | "variable_declaration" => {
                for d in children(n) {
                    if d.kind() == "variable_declarator" {
                        if n.kind() == "variable_declaration"
                            && d.child_by_field_name("value").is_none()
                        {
                            // Hoisting already created the binding. A bare var redeclaration is not an assignment.
                            continue;
                        }
                        let values = if let Some(value) = d.child_by_field_name("value") {
                            self.eval(value, env, depth + 1)?
                        } else {
                            Taint::new()
                        };
                        if let Some(name) = d.child_by_field_name("name") {
                            self.bind(
                                name,
                                &values,
                                env,
                                n.kind() == "variable_declaration",
                                depth + 1,
                            )?;
                        }
                    }
                }
            }
            "if_statement" => {
                if let Some(c) = n.child_by_field_name("condition") {
                    self.eval(c, env, depth + 1)?;
                }
                let mut a = env.clone();
                let mut b = env.clone();
                if let Some(c) = n.child_by_field_name("consequence") {
                    self.flow(c, &mut a, depth + 1)?;
                }
                if let Some(c) = n.child_by_field_name("alternative") {
                    self.flow(c, &mut b, depth + 1)?;
                }
                *env = a.join(b);
            }
            "else_clause" | "finally_clause" => {
                for c in children(n) {
                    self.flow(c, env, depth + 1)?;
                }
            }
            "export_statement" => {
                for child in children(n) {
                    self.flow(child, env, depth + 1)?;
                }
            }
            "while_statement" | "do_statement" | "for_statement" | "for_in_statement" => {
                env.push(false);
                if let Some(init) = n.child_by_field_name("initializer") {
                    self.flow(init, env, depth + 1)?;
                }
                if n.kind() == "for_in_statement" {
                    self.note(
                        n,
                        "for-in/of iteration values and ordering are conservative",
                    );
                    if let Some(right) = n.child_by_field_name("right") {
                        let values = self.eval(right, env, depth + 1)?;
                        if let Some(left) = n.child_by_field_name("left") {
                            self.bind(left, &values, env, false, depth + 1)?;
                        }
                    }
                }
                let mut stable = false;
                for _ in 0..self.policy.max_iterations {
                    if self.report.bounded && self.report.nodes > self.policy.max_nodes {
                        break;
                    }
                    let before = env.clone();
                    let mut after = env.clone();
                    if let Some(c) = n.child_by_field_name("condition") {
                        self.eval(c, &mut after, depth + 1)?;
                    }
                    if let Some(body) = n.child_by_field_name("body") {
                        self.flow(body, &mut after, depth + 1)?;
                    }
                    if let Some(step) = n.child_by_field_name("increment") {
                        self.eval(step, &mut after, depth + 1)?;
                    }
                    *env = before.clone().join(after);
                    if *env == before {
                        stable = true;
                        break;
                    }
                }
                if !stable {
                    self.report.bounded = true;
                }
                env.0.pop();
            }
            "try_statement" => {
                self.note(n, "exception paths are conservatively joined");
                let before = env.clone();
                let mut a = before.clone();
                let mut b = before;
                if let Some(body) = n.child_by_field_name("body") {
                    self.flow(body, &mut a, depth + 1)?;
                }
                if let Some(handler) = n.child_by_field_name("handler") {
                    b.push(false);
                    if let Some(parameter) = handler.child_by_field_name("parameter") {
                        self.bind(parameter, &Taint::new(), &mut b, false, depth + 1)?;
                    }
                    if let Some(body) = handler.child_by_field_name("body") {
                        self.flow(body, &mut b, depth + 1)?;
                    }
                    b.0.pop();
                }
                *env = a.join(b);
                if let Some(finalizer) = n.child_by_field_name("finalizer") {
                    self.flow(finalizer, env, depth + 1)?;
                }
            }
            "return_statement" | "throw_statement" | "break_statement" | "continue_statement"
            | "switch_statement" | "class_declaration" => {
                self.note(n, "abrupt/control/class semantics are not fully modeled");
                self.eval(n, env, depth + 1)?;
            }
            _ => {
                self.eval(n, env, depth + 1)?;
            }
        }
        Ok(())
    }
}
pub fn analyze(
    store: &Store,
    snapshot: &Snapshot,
    graph: &Graph,
    policy: Policy,
    deadline: &Deadline,
) -> Result<Value> {
    deadline.check()?;
    store.set_deadline(deadline)?;
    crate::source_encoding::require_legacy(store, snapshot, "dataflow raw-source spans")?;
    ensure!(
        snapshot.workspace_id == store.workspace_id
            && identity(&store.snapshot(&snapshot.id)?) == identity(snapshot),
        "Snapshot scope differs from immutable store"
    );
    ensure!(
        graph.snapshot_id == snapshot.id
            && identity(&store.graph(&snapshot.id)?) == identity(graph),
        "Graph scope differs from immutable store"
    );
    ensure!(
        (1..=64).contains(&policy.max_iterations)
            && (1..=1_000_000).contains(&policy.max_nodes)
            && (1..=10_000).contains(&policy.max_records),
        "Invalid dataflow bounds"
    );
    ensure!(
        policy.sources.len() <= 100 && policy.sinks.len() <= 100 && policy.sanitizers.len() <= 100,
        "Policy selector budget exceeded"
    );
    ensure!(
        policy
            .sources
            .iter()
            .chain(policy.sinks.keys())
            .chain(policy.sanitizers.iter())
            .all(|p| !p.is_empty() && p.len() <= 200 && !p.chars().any(char::is_control)),
        "Invalid policy selector"
    );
    ensure!(
        policy
            .sinks
            .values()
            .all(|v| v.len() <= 50 && v.iter().all(|i| *i <= 50)),
        "Invalid sink argument indices"
    );
    let mut report = Report {
        findings: BTreeMap::new(),
        sources: BTreeMap::new(),
        unknown: BTreeSet::new(),
        unsupported: BTreeMap::new(),
        bounded: false,
        nodes: 0,
        bytes: 0,
    };
    let mut unsupported_languages = BTreeSet::new();
    let mut analyzed = 0;
    for file in &snapshot.files {
        deadline.check()?;
        if !matches!(
            file.language.as_str(),
            "typescript" | "tsx" | "javascript" | "jsx"
        ) {
            unsupported_languages.insert(file.language.clone());
            continue;
        }
        let text = String::from_utf8(store.read_blob(&file.hash)?)?;
        if text.len() > 16_000_000 {
            report.bounded = true;
            continue;
        }
        let mut parser = Parser::new();
        parser.set_language(
            &crate::analysis::grammar(&file.language)
                .ok_or_else(|| anyhow::anyhow!("Dataflow grammar unavailable"))?,
        )?;
        let mut cancelled = |_: &tree_sitter::ParseState| deadline.check().is_err();
        let tree = parser
            .parse_with_options(
                &mut |offset, _| &text.as_bytes()[offset.min(text.len())..],
                None,
                Some(ParseOptions::new().progress_callback(&mut cancelled)),
            )
            .ok_or_else(|| anyhow::anyhow!("Dataflow parser cancelled or unavailable"))?;
        let mut visitor = Visitor {
            snapshot,
            file,
            graph,
            text: &text,
            lines: line_starts(&text),
            policy: &policy,
            deadline,
            report: &mut report,
        };
        if tree.root_node().has_error() {
            visitor.note(
                tree.root_node(),
                "parse errors prevent complete structural flow",
            );
        }
        let mut env = Env::new();
        visitor.predeclare(tree.root_node(), &mut env, true, 0)?;
        visitor.flow(tree.root_node(), &mut env, 0)?;
        analyzed += 1;
    }
    let mut findings: Vec<_> = report.findings.into_values().collect();
    findings.sort_by_key(|v| {
        (
            v["path"].as_str().unwrap_or("").to_string(),
            v["startByte"].as_u64().unwrap_or(0),
        )
    });
    let incomplete =
        report.bounded || !report.unsupported.is_empty() || !unsupported_languages.is_empty();
    let mut value = json!({"snapshotId":snapshot.id,"workspaceId":snapshot.workspace_id,"graphHash":identity(graph),"provider":"yoke-structural-intraprocedural-dataflow-v1","findings":findings,"sources":report.sources.into_values().collect::<Vec<_>>(),"unknownCalls":report.unknown.len(),"boundedOut":report.bounded,"visitedNodes":report.nodes,"analyzedFiles":analyzed,"unsupported":report.unsupported.into_values().collect::<Vec<_>>(),"unsupportedLanguages":unsupported_languages,"status":if incomplete{"incomplete"}else{"analyzed"},"exhaustive":false,"semantic":false,"assumptions":policy,"limitations":["Structural lexical scopes are not compiler binding. Source/sink/sanitizer selectors are explicit host assumptions; local aliases and imported semantics are not resolved.","Intraprocedural candidates are not proven vulnerabilities. Heap aliases, call effects, closure timing, exceptional/abrupt control, reflection and generated/runtime behavior are not fully modeled. Unknown calls conservatively propagate input taint.","Zero findings is not a security certificate. No analyzed source was executed."]});
    deadline.check()?;
    let bytes = serde_json::to_vec(&value)?;
    ensure!(bytes.len() <= 8_000_000, "Dataflow output budget exceeded");
    value["artifactId"] = json!(store.artifact(&bytes)?);
    Ok(value)
}
