//! Syntax-local lexical and receiver candidates; no compiler or runtime proof.
use crate::{
    capture::Deadline,
    model::{Entity, Extraction, RawEdge, SourceFile},
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap, HashSet};
use tree_sitter::Node;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolutionData {
    #[serde(default)]
    pub language: String,
    pub scopes: Vec<Scope>,
    pub bindings: Vec<Binding>,
    pub uses: Vec<Use>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub callable_arities: Vec<CallableArity>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallableArity {
    pub entity_key: String,
    pub required: usize,
    pub maximum: Option<usize>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Scope {
    pub start: usize,
    pub end: usize,
    pub parent: Option<usize>,
    pub function: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Binding {
    pub scope: usize,
    pub name: String,
    pub defined_at: usize,
    pub target: BindingTarget,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "camelCase")]
pub enum BindingTarget {
    Entity(String),
    Types(Vec<String>),
    Alias(String),
    Import {
        module: String,
        symbol: Option<String>,
    },
    Unknown,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Use {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub argument_count: Option<usize>,
    pub start: usize,
    pub end: usize,
    pub scope: usize,
    pub relation: String,
    pub label: String,
    pub name: String,
    pub receiver: Option<String>,
    pub type_reference: bool,
    pub source_key: String,
}

fn text<'a>(node: Node<'_>, source: &'a str) -> &'a str {
    &source[node.start_byte()..node.end_byte()]
}
fn function_scope(kind: &str, language: &str) -> bool {
    matches!(
        kind,
        "function_item"
            | "function_definition"
            | "function_declaration"
            | "function_expression"
            | "arrow_function"
            | "lambda"
            | "lambda_expression"
            | "method_definition"
            | "method_declaration"
            | "method_signature"
            | "abstract_method_signature"
            | "function_signature_item"
            | "method"
            | "constructor_declaration"
    ) || crate::languages::procedure_scope(kind, language)
}
fn scope_node(kind: &str, language: &str) -> bool {
    function_scope(kind, language)
        || crate::languages::container_scope(kind, language)
        || matches!(
            kind,
            "class_declaration"
                | "class_definition"
                | "class_specifier"
                | "struct_specifier"
                | "struct_item"
                | "impl_item"
                | "interface_declaration"
                | "trait_item"
                | "namespace_definition"
                | "mod_item"
        )
        || (language != "python"
            && matches!(
                kind,
                "statement_block"
                    | "block"
                    | "compound_statement"
                    | "class_body"
                    | "declaration_list"
                    | "field_declaration_list"
            ))
}
fn function_scope_at(
    epoch: crate::analysis::AnalyzerEpoch,
    node: Node<'_>,
    language: &str,
    source: &str,
) -> bool {
    function_scope(node.kind(), language)
        || epoch == crate::analysis::AnalyzerEpoch::V9
            && (language == "verilog" && node.kind() == "task_declaration"
                || language == "commonlisp"
                    && crate::languages::lisp_local_function(node, source).is_some())
}
fn scope_node_at(
    epoch: crate::analysis::AnalyzerEpoch,
    node: Node<'_>,
    language: &str,
    source: &str,
) -> bool {
    scope_node(node.kind(), language)
        || function_scope_at(epoch, node, language, source)
        || epoch == crate::analysis::AnalyzerEpoch::V9
            && language == "commonlisp"
            && crate::languages::lisp_function_form(node, source)
}
fn callable_name(epoch: crate::analysis::AnalyzerEpoch, language: &str, name: &str) -> String {
    if epoch == crate::analysis::AnalyzerEpoch::V9 {
        if language == "commonlisp" {
            // Escaped reader spelling cannot be folded without a readtable.
            return format!(
                "function:{}",
                if name.contains(['|', '\\']) {
                    name.into()
                } else {
                    name.to_ascii_lowercase()
                }
            );
        }
        if language == "powershell" {
            return name.to_ascii_lowercase();
        }
    }
    name.into()
}
fn simple_name(node: Node<'_>, source: &str) -> Option<String> {
    if matches!(
        node.kind(),
        "identifier"
            | "type_identifier"
            | "field_identifier"
            | "property_identifier"
            | "self"
            | "this"
    ) {
        return Some(text(node, source).into());
    }
    node.child_by_field_name("declarator")
        .and_then(|n| simple_name(n, source))
}
fn type_names(node: Node<'_>, source: &str) -> Vec<String> {
    // Only explicit syntax names are retained, never evaluated type expressions.
    let mut out = Vec::new();
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        if matches!(
            n.kind(),
            "array_type"
                | "slice_type"
                | "tuple_type"
                | "function_type"
                | "constructor_type"
                | "object_type"
        ) {
            continue;
        }
        if matches!(n.kind(), "generic_type" | "template_type") {
            if let Some(head) = n
                .child_by_field_name("name")
                .or_else(|| n.child_by_field_name("type"))
                .or_else(|| n.named_child(0))
            {
                stack.push(head);
            }
            continue;
        }
        if matches!(
            n.kind(),
            "type_identifier"
                | "identifier"
                | "scoped_type_identifier"
                | "qualified_identifier"
                | "nested_type_identifier"
        ) {
            let value = text(n, source).trim();
            if !value.is_empty() && value.len() <= 1000 {
                out.push(value.into());
            }
            continue;
        }
        let mut cursor = n.walk();
        stack.extend(n.named_children(&mut cursor));
    }
    out.sort();
    out.dedup();
    out.truncate(32);
    out
}
fn value_binding(node: Option<Node<'_>>, source: &str) -> BindingTarget {
    let Some(node) = node else {
        return BindingTarget::Unknown;
    };
    if matches!(node.kind(), "new_expression" | "object_creation_expression")
        && let Some(t) = node
            .child_by_field_name("constructor")
            .or_else(|| node.child_by_field_name("type"))
    {
        return BindingTarget::Types(vec![text(t, source).into()]);
    }
    if matches!(node.kind(), "call" | "call_expression")
        && let Some(target) = node.child_by_field_name("function")
    {
        let name = text(target, source);
        if name.chars().next().is_some_and(char::is_uppercase) {
            return BindingTarget::Types(vec![name.into()]);
        }
    }
    if node.kind() == "struct_expression"
        && let Some(t) = node.child_by_field_name("name")
    {
        return BindingTarget::Types(vec![text(t, source).into()]);
    }
    if matches!(node.kind(), "identifier" | "scoped_identifier") {
        return BindingTarget::Alias(text(node, source).into());
    }
    BindingTarget::Unknown
}
fn description(node: Node<'_>, source: &str) -> (String, Option<String>) {
    if matches!(
        node.kind(),
        "member_expression" | "attribute" | "field_expression" | "member_access_expression"
    ) {
        let receiver = node
            .child_by_field_name("object")
            .or_else(|| node.child_by_field_name("value"))
            .or_else(|| node.child_by_field_name("argument"))
            .or_else(|| node.child_by_field_name("expression"));
        let name = node
            .child_by_field_name("property")
            .or_else(|| node.child_by_field_name("attribute"))
            .or_else(|| node.child_by_field_name("field"))
            .or_else(|| node.child_by_field_name("name"));
        if let (Some(receiver), Some(name)) = (receiver, name) {
            return (
                text(name, source).into(),
                Some(text(receiver, source).into()),
            );
        }
    }
    if matches!(node.kind(), "scoped_identifier" | "qualified_identifier")
        && let (Some(path), Some(name)) = (
            node.child_by_field_name("path")
                .or_else(|| node.child_by_field_name("scope")),
            node.child_by_field_name("name"),
        )
    {
        return (text(name, source).into(), Some(text(path, source).into()));
    }
    (text(node, source).into(), None)
}
fn declaration_name(node: Node<'_>) -> Option<Node<'_>> {
    node.child_by_field_name("name")
        .or_else(|| node.child_by_field_name("pattern"))
        .or_else(|| node.child_by_field_name("left"))
        .or_else(|| node.child_by_field_name("declarator"))
}
fn pattern_names(node: Node<'_>, source: &str) -> Vec<(usize, String)> {
    let mut names = Vec::new();
    let mut stack = vec![node];
    while let Some(node) = stack.pop() {
        if matches!(
            node.kind(),
            "identifier" | "shorthand_property_identifier_pattern"
        ) {
            names.push((node.start_byte(), text(node, source).into()));
            continue;
        }
        if let Some(parent) = node.parent()
            && matches!(parent.kind(), "pair_pattern" | "assignment_pattern")
            && parent
                .child_by_field_name("key")
                .or_else(|| parent.child_by_field_name("right"))
                == Some(node)
        {
            continue;
        }
        let mut cursor = node.walk();
        stack.extend(node.named_children(&mut cursor));
    }
    names
}
fn push_binding(
    data: &mut ResolutionData,
    scope: usize,
    name: String,
    at: usize,
    target: BindingTarget,
) {
    if !name.is_empty() && name.len() <= 1000 {
        data.bindings.push(Binding {
            scope,
            name,
            defined_at: at,
            target,
        });
    }
}
fn rust_import(
    node: Node<'_>,
    prefix: &str,
    source: &str,
    scope: usize,
    data: &mut ResolutionData,
) {
    let qualified = |value: &str| {
        if prefix.is_empty() {
            value.to_string()
        } else {
            format!("{prefix}::{value}")
        }
    };
    match node.kind() {
        "scoped_use_list" => {
            let path = node
                .child_by_field_name("path")
                .map(|n| qualified(text(n, source)))
                .unwrap_or_else(|| prefix.into());
            if let Some(list) = node.child_by_field_name("list") {
                rust_import(list, &path, source, scope, data);
            }
        }
        "use_list" => {
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                rust_import(child, prefix, source, scope, data);
            }
        }
        "use_as_clause" | "scoped_identifier" | "identifier" | "self" => {
            let path = node
                .child_by_field_name("path")
                .filter(|_| node.kind() == "use_as_clause")
                .unwrap_or(node);
            let full = qualified(text(path, source));
            let alias = node
                .child_by_field_name("alias")
                .map(|n| text(n, source))
                .unwrap_or_else(|| full.rsplit("::").next().unwrap_or(""));
            if let Some((module, symbol)) = full.rsplit_once("::") {
                push_binding(
                    data,
                    scope,
                    alias.into(),
                    node.start_byte(),
                    BindingTarget::Import {
                        module: module.into(),
                        symbol: Some(symbol.into()),
                    },
                );
            } else {
                push_binding(
                    data,
                    scope,
                    alias.into(),
                    node.start_byte(),
                    BindingTarget::Import {
                        module: full.clone(),
                        symbol: None,
                    },
                );
            }
        }
        _ => {}
    }
}

/// Add bounded scoped metadata and source-anchored non-call references to an extraction.
pub fn enrich(
    root: Node<'_>,
    file: &SourceFile,
    source: &str,
    output: &mut Extraction,
    deadline: &Deadline,
) -> Result<()> {
    enrich_at(
        crate::analysis::AnalyzerEpoch::active()?,
        root,
        file,
        source,
        output,
        deadline,
    )
}
pub fn enrich_at(
    epoch: crate::analysis::AnalyzerEpoch,
    root: Node<'_>,
    file: &SourceFile,
    source: &str,
    output: &mut Extraction,
    deadline: &Deadline,
) -> Result<()> {
    let mut data = ResolutionData {
        language: file.language.clone(),
        ..ResolutionData::default()
    };
    data.scopes.push(Scope {
        start: 0,
        end: source.len(),
        parent: None,
        function: false,
    });
    let mut nodes = Vec::new();
    let mut node_scopes = HashMap::new();
    let entity_ranges: HashMap<_, _> = output
        .entities
        .iter()
        .enumerate()
        .filter(|(_, e)| {
            e.kind != "file" && e.producer == epoch.producer() && e.kind != "rationale"
        })
        .map(|(i, e)| ((e.start_byte, e.end_byte), i))
        .collect();
    let entity_starts: HashMap<_, _> = output
        .entities
        .iter()
        .enumerate()
        .filter(|(_, e)| e.producer == epoch.producer() && e.kind != "rationale")
        .map(|(i, e)| (e.start_byte, i))
        .collect();
    let mut node_owners = HashMap::new();
    // TSNode::parent searches from the root. Repeating it across wide sibling
    // lists (including comments) is quadratic; traversal already knows parents.
    let mut node_parents = HashMap::new();
    let mut stack = vec![(root, 0usize, 0usize)];
    while let Some((node, parent, previous_owner)) = stack.pop() {
        if nodes.len().is_multiple_of(512) {
            deadline.check()?;
        }
        ensure!(
            nodes.len() < 1_000_000 && data.scopes.len() < 100_000,
            "Scoped AST budget exceeded"
        );
        let scope = if node != root && scope_node_at(epoch, node, &file.language, source) {
            let index = data.scopes.len();
            data.scopes.push(Scope {
                start: node.start_byte(),
                end: node.end_byte(),
                parent: Some(
                    if epoch == crate::analysis::AnalyzerEpoch::V9
                        && file.language == "commonlisp"
                        && crate::languages::lisp_local_function(node, source)
                            .is_some_and(|form| crate::languages::lisp_flet(form, source))
                    {
                        data.scopes[parent].parent.unwrap_or(0)
                    } else {
                        parent
                    },
                ),
                function: function_scope_at(epoch, node, &file.language, source),
            });
            index
        } else {
            parent
        };
        let owner = entity_ranges
            .get(&(node.start_byte(), node.end_byte()))
            .copied()
            .unwrap_or(previous_owner);
        node_owners.insert(node.id(), owner);
        node_scopes.insert(node.id(), scope);
        nodes.push(node);
        let mut cursor = node.walk();
        let children: Vec<_> = node.named_children(&mut cursor).collect();
        for child in children.into_iter().rev() {
            node_parents.insert(child.id(), node);
            stack.push((child, scope, owner));
        }
    }
    let by_start: HashMap<_, _> = nodes
        .iter()
        .filter(|node| crate::analysis::symbol(epoch, **node, source, &file.language).is_some())
        .map(|node| (node.start_byte(), *node))
        .collect();
    let mut declarations = HashSet::new();
    for entity in &mut output.entities {
        deadline.check()?;
        if let Some(start) = entity.name_start_byte {
            declarations.insert(start);
        }
        if entity.kind == "file" || entity.producer != epoch.producer() {
            continue;
        }
        let Some(node) = by_start.get(&entity.start_byte).copied() else {
            continue;
        };
        if epoch == crate::analysis::AnalyzerEpoch::V9
            && file.language == "commonlisp"
            && let Some((required, maximum)) = crate::languages::lisp_arity(node, source)
        {
            data.callable_arities.push(CallableArity {
                entity_key: entity.key.clone(),
                required,
                maximum,
            });
        }
        // The signature is an exact source header. Long headers stay visibly bounded.
        let (signature, truncated) =
            crate::analysis::declarations::signature(node, source, entity.start_byte);
        entity.signature = signature;
        if truncated {
            output.diagnostics.push(crate::model::Diagnostic {
                path: file.path.clone(),
                start_byte: entity.start_byte,
                message: "Source signature header omitted after 4096 UTF-8 bytes".into(),
                severity: "warning".into(),
                producer: epoch.producer().into(),
            });
        }
        let scope = node_scopes[&node.id()];
        let binding_scope = if epoch == crate::analysis::AnalyzerEpoch::V9
            && file.language == "commonlisp"
            && let Some(form) = crate::languages::lisp_local_function(node, source)
        {
            node_scopes[&form.id()]
        } else if scope_node_at(epoch, node, &file.language, source) {
            data.scopes[scope].parent.unwrap_or(0)
        } else {
            scope
        };
        if entity.kind != "implementation" {
            push_binding(
                &mut data,
                binding_scope,
                callable_name(epoch, &file.language, &entity.name),
                0,
                BindingTarget::Entity(entity.key.clone()),
            );
        }
        if matches!(
            entity.kind.as_str(),
            "class" | "struct" | "interface" | "implementation"
        ) {
            for name in ["self", "this", "cls"] {
                push_binding(
                    &mut data,
                    scope,
                    name.into(),
                    0,
                    BindingTarget::Types(vec![entity.name.clone()]),
                );
            }
        }
    }
    // Python and C-family method declarations have the same AST kind as free functions.
    let class_keys: HashSet<_> = output
        .entities
        .iter()
        .filter(|e| {
            matches!(
                e.kind.as_str(),
                "class" | "struct" | "interface" | "implementation"
            )
        })
        .map(|e| e.key.clone())
        .collect();
    for entity in &mut output.entities {
        if entity.kind == "function"
            && entity
                .parent_key
                .as_ref()
                .is_some_and(|key| class_keys.contains(key))
        {
            entity.kind = "method".into();
        }
    }
    for node in &nodes {
        deadline.check()?;
        let scope = node_scopes[&node.id()];
        if file.language == "vbnet" && node.kind() == "dim_statement" {
            let mut cursor = node.walk();
            for name in node.children_by_field_name("name", &mut cursor) {
                declarations.insert(name.start_byte());
                push_binding(
                    &mut data,
                    scope,
                    text(name, source).into(),
                    node.start_byte(),
                    BindingTarget::Unknown,
                );
            }
        }
        if matches!(
            node.kind(),
            "variable_declarator"
                | "let_declaration"
                | "required_parameter"
                | "optional_parameter"
                | "typed_parameter"
                | "typed_default_parameter"
                | "default_parameter"
                | "parameter"
                | "parameter_declaration"
                | "formal_parameter"
                | "assignment"
                | "public_field_definition"
                | "field_declaration"
        ) {
            let name = declaration_name(*node).or_else(|| {
                (node.kind() == "typed_parameter")
                    .then(|| node.named_child(0))
                    .flatten()
            });
            if let Some(name_node) = name {
                if matches!(
                    name_node.kind(),
                    "object_pattern"
                        | "array_pattern"
                        | "pattern_list"
                        | "tuple_pattern"
                        | "list_pattern"
                        | "tuple"
                ) {
                    for (at, name) in pattern_names(name_node, source) {
                        declarations.insert(at);
                        push_binding(
                            &mut data,
                            scope,
                            name,
                            node.start_byte(),
                            BindingTarget::Unknown,
                        );
                    }
                }
                let member = matches!(
                    name_node.kind(),
                    "attribute" | "member_expression" | "field_expression"
                )
                .then(|| description(name_node, source));
                let member_name = member
                    .as_ref()
                    .filter(|(_, receiver)| {
                        receiver
                            .as_deref()
                            .is_some_and(|r| matches!(r, "self" | "this" | "cls"))
                    })
                    .map(|(name, _)| format!("this.{name}"));
                if let Some(mut name) = simple_name(name_node, source).or(member_name.clone()) {
                    declarations.insert(name_node.start_byte());
                    let target = if let Some(entity) = entity_starts
                        .get(&node.start_byte())
                        .map(|i| &output.entities[*i])
                        .filter(|e| e.name == name)
                    {
                        BindingTarget::Entity(entity.key.clone())
                    } else if let Some(ty) = node.child_by_field_name("type") {
                        let names = type_names(ty, source);
                        if names.is_empty() {
                            BindingTarget::Unknown
                        } else {
                            BindingTarget::Types(names)
                        }
                    } else {
                        value_binding(
                            node.child_by_field_name("value")
                                .or_else(|| node.child_by_field_name("right")),
                            source,
                        )
                    };
                    let mut bind_scope = scope;
                    if member_name.is_some() {
                        let mut parent = Some(&output.entities[node_owners[&node.id()]]);
                        while let Some(entity) = parent {
                            if matches!(entity.kind.as_str(), "class" | "struct" | "implementation")
                            {
                                bind_scope = data
                                    .scopes
                                    .iter()
                                    .position(|s| {
                                        s.start == entity.start_byte && s.end == entity.end_byte
                                    })
                                    .unwrap_or(bind_scope);
                                break;
                            }
                            parent = entity
                                .parent_key
                                .as_ref()
                                .and_then(|key| output.entities.iter().find(|e| &e.key == key));
                        }
                    }
                    if node.kind() == "public_field_definition"
                        || (node.kind() == "field_declaration"
                            && !matches!(target, BindingTarget::Entity(_)))
                    {
                        name = format!("this.{name}");
                    }
                    if node.parent().is_some_and(|parent| {
                        parent.kind() == "variable_declaration"
                            && text(parent, source).trim_start().starts_with("var ")
                    }) {
                        while bind_scope != 0 && !data.scopes[bind_scope].function {
                            bind_scope = data.scopes[bind_scope].parent.unwrap_or(0);
                        }
                    }
                    push_binding(&mut data, bind_scope, name, node.start_byte(), target);
                }
            }
        }
        // C and C++ declarations put the type on the statement, not the declarator.
        if node.kind() == "declaration"
            && let (Some(ty), Some(declarator)) = (
                node.child_by_field_name("type"),
                node.child_by_field_name("declarator"),
            )
            && let Some(name) = simple_name(declarator, source)
            && !output.entities.iter().any(|e| {
                e.name == name && node.start_byte() <= e.start_byte && e.end_byte <= node.end_byte()
            })
        {
            declarations.insert(declarator.start_byte());
            let types = type_names(ty, source);
            push_binding(
                &mut data,
                scope,
                name,
                node.start_byte(),
                if types.is_empty() {
                    BindingTarget::Unknown
                } else {
                    BindingTarget::Types(types)
                },
            );
        }
        if node.kind() == "parameters" || node.kind() == "formal_parameters" {
            let mut cursor = node.walk();
            for parameter in node
                .named_children(&mut cursor)
                .filter(|n| n.kind() == "identifier")
            {
                let name = text(parameter, source);
                declarations.insert(parameter.start_byte());
                if name != "self" && name != "cls" {
                    push_binding(&mut data, scope, name.into(), 0, BindingTarget::Unknown);
                }
            }
        }
        if matches!(node.kind(), "import_statement" | "import_from_statement") {
            if matches!(
                file.language.as_str(),
                "typescript" | "tsx" | "javascript" | "jsx"
            ) {
                let Some(module) = node
                    .child_by_field_name("source")
                    .map(|n| text(n, source).trim_matches(['\'', '"']).to_string())
                else {
                    continue;
                };
                let mut cursor = node.walk();
                let mut imports: Vec<_> = node.named_children(&mut cursor).collect();
                while let Some(import) = imports.pop() {
                    if import.kind() == "import_specifier" {
                        if let Some(symbol) = import.child_by_field_name("name") {
                            let alias = import.child_by_field_name("alias").unwrap_or(symbol);
                            push_binding(
                                &mut data,
                                scope,
                                text(alias, source).into(),
                                node.start_byte(),
                                BindingTarget::Import {
                                    module: module.clone(),
                                    symbol: Some(text(symbol, source).into()),
                                },
                            );
                        }
                    } else if import.kind() == "namespace_import" {
                        if let Some(alias) = import.named_child(0) {
                            push_binding(
                                &mut data,
                                scope,
                                text(alias, source).into(),
                                node.start_byte(),
                                BindingTarget::Import {
                                    module: module.clone(),
                                    symbol: None,
                                },
                            );
                        }
                    } else if import.kind() == "identifier"
                        && import.parent().is_some_and(|n| n.kind() == "import_clause")
                    {
                        push_binding(
                            &mut data,
                            scope,
                            text(import, source).into(),
                            node.start_byte(),
                            BindingTarget::Import {
                                module: module.clone(),
                                symbol: Some("default".into()),
                            },
                        );
                    } else {
                        let mut cursor = import.walk();
                        imports.extend(import.named_children(&mut cursor));
                    }
                }
            } else if file.language == "python" {
                let from = node.child_by_field_name("module_name");
                let module = from.map(|n| text(n, source));
                let mut cursor = node.walk();
                for import in node.named_children(&mut cursor).filter(|n| {
                    Some(*n) != from && matches!(n.kind(), "aliased_import" | "dotted_name")
                }) {
                    let symbol = import.child_by_field_name("name").unwrap_or(import);
                    let value = text(symbol, source);
                    let alias = import
                        .child_by_field_name("alias")
                        .map(|n| text(n, source))
                        .unwrap_or_else(|| {
                            if module.is_some() {
                                value
                            } else {
                                value.split('.').next().unwrap_or(value)
                            }
                        });
                    push_binding(
                        &mut data,
                        scope,
                        alias.into(),
                        node.start_byte(),
                        BindingTarget::Import {
                            module: module.unwrap_or(value).into(),
                            symbol: module.map(|_| value.into()),
                        },
                    );
                }
            }
        }
        if node.kind() == "use_declaration"
            && file.language == "rust"
            && let Some(argument) = node.child_by_field_name("argument")
        {
            rust_import(argument, "", source, scope, &mut data);
        }
        if node.kind() == "export_specifier"
            && matches!(
                file.language.as_str(),
                "typescript" | "tsx" | "javascript" | "jsx"
            )
            && let Some(name) = node.child_by_field_name("name")
        {
            let alias = node.child_by_field_name("alias").unwrap_or(name);
            let mut parent = node.parent();
            let mut statement = None;
            while let Some(node) = parent {
                if node.kind() == "export_statement" {
                    statement = Some(node);
                    break;
                }
                parent = node.parent();
            }
            let target = statement
                .and_then(|s| s.child_by_field_name("source"))
                .map(|module| BindingTarget::Import {
                    module: text(module, source).trim_matches(['\'', '"']).into(),
                    symbol: Some(text(name, source).trim_matches(['\'', '"']).into()),
                })
                .unwrap_or_else(|| BindingTarget::Alias(text(name, source).into()));
            push_binding(
                &mut data,
                scope,
                text(alias, source).trim_matches(['\'', '"']).into(),
                node.start_byte(),
                target,
            );
        }
    }
    // Assignment invalidates syntax-local receiver inference at and after that point.
    // A conditional assignment is conservatively visible outside its nested branch.
    for node in &nodes {
        if matches!(
            node.kind(),
            "assignment_expression" | "augmented_assignment_expression"
        ) && let Some(left) = node.child_by_field_name("left")
            && let Some(name) = simple_name(left, source)
        {
            let mut scope = node_scopes[&node.id()];
            for _ in 0..128 {
                if data
                    .bindings
                    .iter()
                    .any(|b| b.scope == scope && b.name == name)
                {
                    break;
                }
                let Some(parent) = data.scopes[scope].parent else {
                    break;
                };
                scope = parent;
            }
            push_binding(
                &mut data,
                scope,
                name,
                node.start_byte(),
                BindingTarget::Unknown,
            );
        }
    }
    let line_starts: Vec<_> = std::iter::once(0)
        .chain(
            source
                .bytes()
                .enumerate()
                .filter_map(|(i, b)| (b == b'\n').then_some(i + 1)),
        )
        .collect();
    let mut added = Vec::new();
    let mut call_targets = Vec::new();
    for (index, node) in nodes.iter().enumerate() {
        if index.is_multiple_of(512) {
            deadline.check()?;
        }
        let target = crate::analysis::call_target(epoch, *node, source, &file.language);
        if let Some(target) = target {
            let (name, mut receiver) = description(target, source);
            if node.kind() == "method_invocation" && file.language == "java" {
                receiver = node
                    .child_by_field_name("object")
                    .map(|node| text(node, source).to_string());
            }
            if node.kind() == "message_expression" && file.language == "objc" {
                receiver = node
                    .child_by_field_name("receiver")
                    .map(|node| text(node, source).to_owned());
            }
            let label = text(target, source).to_string();
            data.uses.push(Use {
                argument_count: if epoch == crate::analysis::AnalyzerEpoch::V9
                    && file.language == "commonlisp"
                {
                    let mut cursor = node.walk();
                    let children: Vec<_> = node
                        .named_children(&mut cursor)
                        .filter(|child| child.kind() != "comment")
                        .collect();
                    (!children.iter().any(|child| {
                        matches!(
                            child.kind(),
                            "read_cond_lit" | "splicing_read_cond_lit" | "dis_expr"
                        )
                    }))
                    .then_some(children.len().saturating_sub(1))
                } else {
                    None
                },
                start: node.start_byte(),
                end: node.end_byte(),
                scope: node_scopes[&node.id()],
                relation: "calls".into(),
                label,
                name: callable_name(epoch, &file.language, &name),
                receiver,
                type_reference: false,
                source_key: output.entities[node_owners[&node.id()]].key.clone(),
            });
            call_targets.push(target.id());
        }
    }
    let call_targets: HashSet<_> = call_targets.into_iter().collect();
    for (index, node) in nodes.iter().enumerate() {
        if index.is_multiple_of(512) {
            deadline.check()?;
        }
        ensure!(
            data.bindings.len() <= 100_000 && data.uses.len() < 200_000,
            "Scoped binding/reference budget exceeded"
        );
        let scope = node_scopes[&node.id()];
        let mut inherited_relation = None;
        if let Some(parent) = node_parents.get(&node.id()).copied() {
            inherited_relation = match parent.kind() {
                "extends_clause" | "extends_type_clause" | "superclass" | "base_class_clause" => {
                    Some("extends")
                }
                "implements_clause" | "super_interfaces" => Some("implements"),
                _ => None,
            };
            if file.language == "python"
                && parent.kind() == "argument_list"
                && parent
                    .parent()
                    .is_some_and(|n| n.kind() == "class_definition")
            {
                inherited_relation = Some("extends");
            }
        }
        let reference_identifier = (epoch == crate::analysis::AnalyzerEpoch::V9
            && file.language == "powershell"
            && node.kind() == "variable")
            || crate::languages::explicit_function_reference(*node, &file.language, source)
            || matches!(
                node.kind(),
                "identifier"
                    | "type_identifier"
                    | "scoped_type_identifier"
                    | "member_expression"
                    | "attribute"
                    | "field_expression"
                    | "qualified_identifier"
                    | "scoped_identifier"
            );
        if inherited_relation.is_none() && !reference_identifier {
            continue;
        }
        if declarations.contains(&node.start_byte()) || call_targets.contains(&node.id()) {
            continue;
        }
        // Identifier children of one member/path expression are represented once by the whole expression.
        if node.parent().is_some_and(|parent| {
            (matches!(parent.kind(), "keyword_argument" | "labeled_statement")
                && parent
                    .child_by_field_name("name")
                    .or_else(|| parent.child_by_field_name("label"))
                    == Some(*node))
                || matches!(
                    parent.kind(),
                    "member_expression"
                        | "attribute"
                        | "field_expression"
                        | "qualified_identifier"
                        | "scoped_identifier"
                        | "scoped_type_identifier"
                )
        }) {
            continue;
        }
        let mut ancestor = node.parent();
        let mut skip = false;
        while let Some(parent) = ancestor {
            if call_targets.contains(&parent.id()) {
                skip = true;
                break;
            }
            if matches!(
                parent.kind(),
                "import_statement"
                    | "import_from_statement"
                    | "use_declaration"
                    | "string"
                    | "comment"
                    | "attribute_item"
            ) {
                skip = true;
                break;
            }
            if parent.kind() == "string_literal"
                && !(epoch == crate::analysis::AnalyzerEpoch::V9 && file.language == "powershell")
            {
                skip = true;
                break;
            }
            if function_scope(parent.kind(), &file.language)
                || matches!(
                    parent.kind(),
                    "statement_block" | "block" | "compound_statement"
                )
            {
                break;
            }
            ancestor = parent.parent();
        }
        if skip {
            continue;
        }
        let label = text(*node, source).to_string();
        if label.is_empty() || label.len() > 1000 {
            continue;
        }
        let relation = inherited_relation.unwrap_or("references");
        let (mut name, receiver) = description(*node, source);
        if epoch == crate::analysis::AnalyzerEpoch::V9
            && file.language == "commonlisp"
            && crate::languages::explicit_function_reference(*node, &file.language, source)
        {
            name = callable_name(epoch, &file.language, &name);
        }
        if epoch == crate::analysis::AnalyzerEpoch::V9
            && file.language == "powershell"
            && node.kind() == "variable"
        {
            let variable = label
                .trim_start_matches('$')
                .trim_start_matches('{')
                .trim_end_matches('}');
            if variable
                .get(..9)
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("function:"))
            {
                name = variable[9..].to_ascii_lowercase();
            } else {
                // A value variable, including an interpolated one, cannot name a function.
                name = format!("value:{}", variable.to_ascii_lowercase());
            }
        }
        let parent_entity = &output.entities[node_owners[&node.id()]];
        added.push(RawEdge {
            source_key: parent_entity.key.clone(),
            target_label: label.clone(),
            relation: relation.into(),
            start_byte: node.start_byte(),
            end_byte: node.end_byte(),
            start_line: line_starts.partition_point(|start| *start <= node.start_byte()),
            end_line: line_starts.partition_point(|start| *start < node.end_byte()),
        });
        data.uses.push(Use {
            argument_count: None,
            start: node.start_byte(),
            end: node.end_byte(),
            scope,
            relation: relation.into(),
            label,
            name,
            receiver,
            type_reference: inherited_relation.is_some() || node.kind().contains("type_identifier"),
            source_key: parent_entity.key.clone(),
        });
    }
    output.edges.extend(added);
    output.resolution = data;
    Ok(())
}

/// One lookup result can contain several possible dispatch targets. None means legacy metadata.
type MemberCache = HashMap<(String, String), Option<Vec<String>>>;
pub struct Resolver<'a> {
    arities: HashMap<String, (usize, Option<usize>)>,
    entities: &'a [Entity],
    data: BTreeMap<String, ResolutionData>,
    keys: HashMap<String, &'a Entity>,
    names: HashMap<(String, String), Vec<&'a Entity>>,
    global: HashMap<(String, String), Vec<&'a Entity>>,
    methods: HashMap<(String, String, String), Vec<&'a Entity>>,
    bindings: HashMap<(String, usize, String), Vec<Binding>>,
    uses: HashMap<(String, usize, usize, String, String), Use>,
    languages: HashMap<String, String>,
    by_id: HashMap<String, &'a Entity>,
    qualified: HashMap<(String, String), Vec<&'a Entity>>,
    bases: HashMap<String, Vec<Use>>,
    member_cache: RefCell<MemberCache>,
    deadline: &'a Deadline,
    work: Cell<usize>,
    exhausted: Cell<bool>,
}
impl<'a> Resolver<'a> {
    pub fn new(
        entities: &'a [Entity],
        records: Vec<(String, ResolutionData)>,
        deadline: &'a Deadline,
    ) -> Result<Self> {
        let data: BTreeMap<_, _> = records.into_iter().collect();
        let mut keys = HashMap::new();
        let mut names: HashMap<_, Vec<_>> = HashMap::new();
        let mut global: HashMap<_, Vec<_>> = HashMap::new();
        let mut methods: HashMap<_, Vec<_>> = HashMap::new();
        let mut languages = HashMap::new();
        let mut by_id = HashMap::new();
        let mut qualified: HashMap<_, Vec<_>> = HashMap::new();
        for (index, entity) in entities.iter().enumerate() {
            if index.is_multiple_of(256) {
                deadline.check()?;
            }
            by_id.insert(entity.id.clone(), entity);
            let name = entity
                .qualified_name
                .strip_prefix(&format!("{}/", entity.path))
                .unwrap_or(&entity.qualified_name);
            qualified
                .entry((entity.language.clone(), name.to_string()))
                .or_default()
                .push(entity);
            languages
                .entry(entity.path.clone())
                .or_insert_with(|| entity.language.clone());
            keys.insert(entity.key.clone(), entity);
            names
                .entry((entity.path.clone(), entity.name.clone()))
                .or_default()
                .push(entity);
            global
                .entry((entity.language.clone(), entity.name.clone()))
                .or_default()
                .push(entity);
            if entity.producer == "yoke-tree-sitter-9"
                && matches!(entity.language.as_str(), "commonlisp" | "powershell")
            {
                let lookup = callable_name(
                    crate::analysis::AnalyzerEpoch::V9,
                    &entity.language,
                    &entity.name,
                );
                if lookup != entity.name {
                    global
                        .entry((entity.language.clone(), lookup))
                        .or_default()
                        .push(entity);
                }
            }
        }
        for entity in entities {
            if matches!(entity.kind.as_str(), "function" | "method")
                && let Some(parent) = entity
                    .parent_key
                    .as_ref()
                    .and_then(|key| keys.get(key))
                    .filter(|e| {
                        matches!(
                            e.kind.as_str(),
                            "class" | "struct" | "interface" | "implementation"
                        )
                    })
            {
                methods
                    .entry((
                        entity.path.clone(),
                        parent.qualified_name.clone(),
                        entity.name.clone(),
                    ))
                    .or_default()
                    .push(entity);
            }
        }
        let mut bindings: HashMap<_, Vec<_>> = HashMap::new();
        let mut uses = HashMap::new();
        let mut bases: HashMap<String, Vec<Use>> = HashMap::new();
        let mut arities = HashMap::new();
        for (path, record) in &data {
            for arity in &record.callable_arities {
                arities.insert(arity.entity_key.clone(), (arity.required, arity.maximum));
            }
            for (index, binding) in record.bindings.iter().enumerate() {
                if index.is_multiple_of(256) {
                    deadline.check()?;
                }
                bindings
                    .entry((path.clone(), binding.scope, binding.name.clone()))
                    .or_default()
                    .push(binding.clone());
            }
            for (index, usage) in record.uses.iter().enumerate() {
                if index.is_multiple_of(256) {
                    deadline.check()?;
                }
                if matches!(usage.relation.as_str(), "extends" | "implements") {
                    bases
                        .entry(usage.source_key.clone())
                        .or_default()
                        .push(usage.clone());
                }
                uses.insert(
                    (
                        path.clone(),
                        usage.start,
                        usage.end,
                        usage.relation.clone(),
                        usage.label.clone(),
                    ),
                    usage.clone(),
                );
            }
        }
        for values in bindings.values_mut() {
            values.sort_by_key(|b| b.defined_at);
        }
        for (path, record) in &data {
            if !record.language.is_empty() {
                languages.insert(path.clone(), record.language.clone());
            }
        }
        Ok(Self {
            arities,
            entities,
            data,
            keys,
            names,
            global,
            methods,
            bindings,
            uses,
            languages,
            by_id,
            qualified,
            bases,
            member_cache: RefCell::new(HashMap::new()),
            deadline,
            work: Cell::new(0),
            exhausted: Cell::new(false),
        })
    }
    fn tick(&self) -> bool {
        let work = self.work.get() + 1;
        self.work.set(work);
        if work > 8192 || (work.is_multiple_of(64) && self.deadline.check().is_err()) {
            self.exhausted.set(true);
            false
        } else {
            true
        }
    }
    fn binding(&self, path: &str, mut scope: usize, name: &str, at: usize) -> Option<&Binding> {
        for _ in 0..128 {
            if let Some(values) = self.bindings.get(&(path.into(), scope, name.into())) {
                if let Some(binding) = values.iter().rev().find(|b| b.defined_at <= at) {
                    return Some(binding);
                }
                if self
                    .languages
                    .get(path)
                    .is_some_and(|language| language == "rust")
                {
                    if let Some(binding) = values
                        .iter()
                        .find(|b| matches!(b.target, BindingTarget::Entity(_)))
                    {
                        return Some(binding);
                    }
                    // Rust locals begin at their declaration; an earlier call
                    // can still refer to a binding in the enclosing scope.
                } else {
                    return values.first();
                }
            }
            scope = self.data.get(path)?.scopes.get(scope)?.parent?;
        }
        None
    }
    fn module_paths(&self, from: &str, module: &str, language: &str) -> Vec<String> {
        let mut paths = Vec::new();
        let mut bases = Vec::new();
        let mut components: Vec<_> = from.split('/').collect();
        components.pop();
        let module = module.trim_matches(['\'', '"']);
        if language == "rust" {
            let mut parts = module.split("::").peekable();
            if parts.peek() == Some(&"crate") {
                parts.next();
                components = if from.starts_with("src/") {
                    vec!["src"]
                } else {
                    Vec::new()
                };
            }
            while parts
                .peek()
                .is_some_and(|part| *part == "self" || *part == "super")
            {
                if parts.next() == Some("super") && components.pop().is_none() {
                    return paths;
                }
            }
            components.extend(parts);
            bases.push(components.join("/"));
        } else if language == "python" {
            let leading = module.chars().take_while(|c| *c == '.').count();
            for _ in 1..leading {
                if components.pop().is_none() {
                    return paths;
                }
            }
            let value = module.trim_start_matches('.').replace('.', "/");
            bases.push(if components.is_empty() {
                value.clone()
            } else {
                format!("{}/{value}", components.join("/"))
            });
            if leading == 0 {
                bases.push(value);
            }
        } else {
            if module.starts_with('.') {
                for part in module.split('/') {
                    match part {
                        "" | "." => {}
                        ".." => {
                            if components.pop().is_none() {
                                return paths;
                            }
                        }
                        _ => components.push(part),
                    }
                }
                bases.push(components.join("/"));
            } else {
                bases.push(module.into());
            }
        }
        for base in bases {
            for suffix in [
                "",
                ".ts",
                ".tsx",
                ".js",
                ".jsx",
                ".mjs",
                ".cjs",
                ".py",
                ".pyi",
                ".rs",
                ".go",
                ".java",
                ".cpp",
                ".h",
                "/index.ts",
                "/index.js",
                "/__init__.py",
                "/mod.rs",
            ] {
                let candidate = format!("{base}{suffix}");
                let compatible = self.languages.get(&candidate).is_some_and(|candidate| {
                    candidate == language
                        || (matches!(language, "typescript" | "tsx" | "javascript" | "jsx")
                            && matches!(
                                candidate.as_str(),
                                "typescript" | "tsx" | "javascript" | "jsx"
                            ))
                        || (matches!(language, "c" | "cpp")
                            && matches!(candidate.as_str(), "c" | "cpp"))
                });
                if self.data.contains_key(&candidate) && compatible {
                    paths.push(candidate);
                }
            }
            // A TypeScript import of a emitted .js module may refer to captured .ts source.
            if let Some(stem) = base.strip_suffix(".js") {
                for suffix in [".ts", ".tsx"] {
                    let candidate = format!("{stem}{suffix}");
                    if self.data.contains_key(&candidate)
                        && matches!(language, "typescript" | "tsx" | "javascript" | "jsx")
                    {
                        paths.push(candidate);
                    }
                }
            }
        }
        paths.sort();
        paths.dedup();
        paths
    }
    fn file_symbols(&self, path: &str, name: &str, depth: usize) -> Vec<String> {
        if depth > 16 || !self.tick() {
            return Vec::new();
        }
        let direct: Vec<_> = self
            .names
            .get(&(path.into(), name.into()))
            .into_iter()
            .flatten()
            .filter(|e| e.kind != "file" && self.top_level(e))
            .map(|e| e.id.clone())
            .collect();
        if !direct.is_empty() {
            return direct;
        }
        if name == "default" {
            let symbols: Vec<_> = self
                .entities
                .iter()
                .filter(|e| {
                    e.path == path
                        && self.top_level(e)
                        && matches!(e.kind.as_str(), "class" | "function")
                })
                .collect();
            if symbols.len() == 1 {
                return vec![symbols[0].id.clone()];
            }
        }
        let Some(binding) = self.binding(path, 0, name, usize::MAX) else {
            return Vec::new();
        };
        self.resolve_binding_target(path, 0, usize::MAX, binding, false, depth + 1)
    }
    fn top_level(&self, entity: &Entity) -> bool {
        entity
            .parent_key
            .as_ref()
            .and_then(|key| self.keys.get(key))
            .is_none_or(|parent| parent.kind == "file" || parent.kind == "module")
    }
    fn resolve_binding_target(
        &self,
        path: &str,
        scope: usize,
        at: usize,
        binding: &Binding,
        types: bool,
        depth: usize,
    ) -> Vec<String> {
        if depth > 16 || !self.tick() {
            return Vec::new();
        }
        match &binding.target {
            BindingTarget::Entity(key) => self
                .keys
                .get(key)
                .map(|e| vec![e.id.clone()])
                .unwrap_or_default(),
            BindingTarget::Import {
                module,
                symbol: Some(symbol),
            } => {
                let language = self.languages.get(path).map_or("", String::as_str);
                let mut found = self
                    .module_paths(path, module, language)
                    .iter()
                    .flat_map(|path| self.file_symbols(path, symbol, depth + 1))
                    .collect::<Vec<_>>();
                if language == "rust" && !module.split("::").any(|part| part == "super") {
                    let prefix = module
                        .strip_prefix("crate::")
                        .or_else(|| module.strip_prefix("self::"))
                        .unwrap_or(module)
                        .replace("::", "/");
                    found.extend(
                        self.qualified
                            .get(&(language.into(), format!("{prefix}/{symbol}")))
                            .into_iter()
                            .flatten()
                            .filter(|e| {
                                matches!(
                                    e.kind.as_str(),
                                    "function" | "class" | "struct" | "type" | "constant"
                                )
                            })
                            .map(|e| e.id.clone()),
                    );
                }
                found
            }
            BindingTarget::Alias(name) => self.bare(path, scope, at, name, types, depth + 1),
            BindingTarget::Types(names) if types => names
                .iter()
                .flat_map(|name| self.bare(path, scope, at, name, true, depth + 1))
                .collect(),
            _ => Vec::new(),
        }
    }
    fn bare(
        &self,
        path: &str,
        scope: usize,
        at: usize,
        name: &str,
        types: bool,
        depth: usize,
    ) -> Vec<String> {
        if depth > 16 || !self.tick() {
            return Vec::new();
        }
        if let Some(binding) = self.binding(path, scope, name, at) {
            return self.resolve_binding_target(path, scope, at, binding, types, depth + 1);
        }
        let language = self.languages.get(path).map_or("", String::as_str);
        let normalized = name.replace("::", "/").replace('.', "/");
        if normalized.contains('/') {
            let candidates = self
                .qualified
                .get(&(language.into(), normalized))
                .into_iter()
                .flatten()
                .filter(|e| {
                    if types {
                        matches!(e.kind.as_str(), "class" | "struct" | "interface" | "type")
                    } else {
                        matches!(
                            e.kind.as_str(),
                            "function" | "method" | "constant" | "class" | "struct"
                        )
                    }
                })
                .map(|e| e.id.clone())
                .collect::<Vec<_>>();
            if !candidates.is_empty() {
                return candidates;
            }
        }
        let candidates: Vec<_> = self
            .global
            .get(&(language.into(), name.into()))
            .into_iter()
            .flatten()
            .filter(|e| {
                self.top_level(e)
                    && if types {
                        matches!(e.kind.as_str(), "class" | "struct" | "interface" | "type")
                    } else {
                        matches!(
                            e.kind.as_str(),
                            "function" | "constant" | "class" | "struct"
                        )
                    }
            })
            .collect();
        if candidates.len() == 1 {
            vec![candidates[0].id.clone()]
        } else {
            Vec::new()
        }
    }
    fn member(&self, path: &str, usage: &Use, receiver: &str, depth: usize) -> Vec<String> {
        if depth > 16 || !self.tick() {
            return Vec::new();
        }
        let normalized = receiver
            .strip_prefix("self.")
            .map_or_else(|| receiver.to_string(), |suffix| format!("this.{suffix}"))
            .replace("->", ".");
        if let Some(binding) = self.binding(path, usage.scope, &normalized, usage.start) {
            if let BindingTarget::Import {
                module,
                symbol: None,
            } = &binding.target
            {
                let language = self.languages.get(path).map_or("", String::as_str);
                return self
                    .module_paths(path, module, language)
                    .iter()
                    .flat_map(|p| self.file_symbols(p, &usage.name, depth + 1))
                    .collect();
            }
            let types = self.resolve_binding_target(
                path,
                usage.scope,
                usage.start,
                binding,
                true,
                depth + 1,
            );
            return types
                .into_iter()
                .flat_map(|id| self.type_member(&id, &usage.name, depth + 1))
                .collect();
        }
        // Explicit class or module-qualified syntax without a local binding.
        let classes = self.bare(path, usage.scope, usage.start, receiver, true, depth + 1);
        if !classes.is_empty() {
            return classes
                .into_iter()
                .flat_map(|id| self.type_member(&id, &usage.name, depth + 1))
                .collect();
        }
        if receiver.contains("::") {
            let language = self.languages.get(path).map_or("", String::as_str);
            return self
                .module_paths(path, receiver, language)
                .iter()
                .flat_map(|p| self.file_symbols(p, &usage.name, depth + 1))
                .collect();
        }
        Vec::new()
    }
    fn type_member(&self, id: &str, name: &str, depth: usize) -> Vec<String> {
        if depth > 64 || !self.tick() {
            return Vec::new();
        }
        let cache_key = (id.to_string(), name.to_string());
        if let Some(value) = self.member_cache.borrow().get(&cache_key) {
            return value.clone().unwrap_or_default();
        }
        self.member_cache
            .borrow_mut()
            .insert(cache_key.clone(), None);
        let Some(class) = self.by_id.get(id).copied() else {
            return Vec::new();
        };
        let direct = self
            .methods
            .get(&(
                class.path.clone(),
                class.qualified_name.clone(),
                name.into(),
            ))
            .map(|items| items.iter().map(|e| e.id.clone()).collect::<Vec<_>>())
            .unwrap_or_default();
        if !direct.is_empty() {
            self.member_cache
                .borrow_mut()
                .insert(cache_key, Some(direct.clone()));
            return direct;
        }
        let mut found = Vec::new();
        for usage in self.bases.get(&class.key).into_iter().flatten() {
            // TypeScript's implements clause checks a contract; it does not
            // provide inherited implementations or default methods.
            if matches!(class.language.as_str(), "typescript" | "tsx")
                && usage.relation == "implements"
            {
                continue;
            }
            for base in self.bare(&class.path, usage.scope, usage.start, &usage.name, true, 0) {
                found.extend(self.type_member(&base, name, depth + 1));
            }
        }
        found.sort();
        found.dedup();
        self.member_cache
            .borrow_mut()
            .insert(cache_key, Some(found.clone()));
        found
    }
    pub fn resolve(&self, path: &str, raw: &RawEdge) -> Result<Option<Vec<String>>> {
        self.deadline.check()?;
        self.work.set(0);
        self.exhausted.set(false);
        self.member_cache.borrow_mut().clear();
        let Some(usage) = self.uses.get(&(
            path.into(),
            raw.start_byte,
            raw.end_byte,
            raw.relation.clone(),
            raw.target_label.clone(),
        )) else {
            return Ok(None);
        };
        let mut targets = if let Some(receiver) = &usage.receiver {
            self.member(path, usage, receiver, 0)
        } else {
            self.bare(
                path,
                usage.scope,
                usage.start,
                &usage.name,
                usage.type_reference,
                0,
            )
        };
        if let Some(count) = usage.argument_count {
            targets.retain(|id| {
                self.by_id
                    .get(id)
                    .and_then(|entity| self.arities.get(&entity.key))
                    .is_none_or(|(required, maximum)| {
                        count >= *required && maximum.is_none_or(|limit| count <= limit)
                    })
            });
        }
        targets.sort();
        targets.dedup();
        self.deadline.check()?;
        ensure!(
            !self.exhausted.get(),
            "Scoped resolution work budget exceeded"
        );
        Ok(Some(targets))
    }
}
