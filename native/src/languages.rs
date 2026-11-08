//! Host-independent grammar registry. Grammar presence is structural support, not compiler binding.
use anyhow::{Result, ensure};
use tree_sitter::{Language, Node};
pub const LANGUAGES: &[&str] = &[
    "rust",
    "typescript",
    "tsx",
    "javascript",
    "jsx",
    "python",
    "go",
    "c",
    "cpp",
    "java",
    "csharp",
    "ruby",
    "php",
    "swift",
    "scala",
    "lua",
    "bash",
    "elixir",
    "solidity",
    "ocaml",
    "ocaml-interface",
    "zig",
    "nix",
    "r",
    "kotlin",
    "dart",
    "clojure",
    "powershell",
    "julia",
    "objc",
    "fortran",
    "erlang",
    "commonlisp",
    "pascal",
    "vbnet",
    "verilog",
    "hcl",
    "apex",
];

pub fn grammar(language: &str) -> Option<Language> {
    Some(match language {
        "rust" => tree_sitter_rust::LANGUAGE.into(),
        "typescript" => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        "tsx" => tree_sitter_typescript::LANGUAGE_TSX.into(),
        "javascript" | "jsx" => tree_sitter_javascript::LANGUAGE.into(),
        "python" => tree_sitter_python::LANGUAGE.into(),
        "go" => tree_sitter_go::LANGUAGE.into(),
        "c" => tree_sitter_c::LANGUAGE.into(),
        "cpp" => tree_sitter_cpp::LANGUAGE.into(),
        "java" => tree_sitter_java::LANGUAGE.into(),
        "csharp" => tree_sitter_c_sharp::LANGUAGE.into(),
        "ruby" => tree_sitter_ruby::LANGUAGE.into(),
        "php" => tree_sitter_php::LANGUAGE_PHP.into(),
        "php-only" => tree_sitter_php::LANGUAGE_PHP_ONLY.into(),
        "swift" => tree_sitter_swift::LANGUAGE.into(),
        "scala" => tree_sitter_scala::LANGUAGE.into(),
        "lua" => tree_sitter_lua::LANGUAGE.into(),
        "bash" => tree_sitter_bash::LANGUAGE.into(),
        "elixir" => tree_sitter_elixir::LANGUAGE.into(),
        "solidity" => tree_sitter_solidity::LANGUAGE.into(),
        "ocaml" => tree_sitter_ocaml::LANGUAGE_OCAML.into(),
        "ocaml-interface" => tree_sitter_ocaml::LANGUAGE_OCAML_INTERFACE.into(),
        "zig" => tree_sitter_zig::LANGUAGE.into(),
        "nix" => tree_sitter_nix::LANGUAGE.into(),
        "r" => tree_sitter_r::LANGUAGE.into(),
        "kotlin" => tree_sitter_kotlin_ng::LANGUAGE.into(),
        "dart" => tree_sitter_dart::LANGUAGE.into(),
        "clojure" => tree_sitter_clojure_orchard::LANGUAGE.into(),
        "powershell" => tree_sitter_powershell::LANGUAGE.into(),
        "julia" => tree_sitter_julia::LANGUAGE.into(),
        "objc" => tree_sitter_objc::LANGUAGE.into(),
        "fortran" => tree_sitter_fortran::LANGUAGE.into(),
        "erlang" => tree_sitter_erlang::LANGUAGE.into(),
        "commonlisp" => tree_sitter_commonlisp::LANGUAGE_COMMONLISP.into(),
        "pascal" => tree_sitter_pascal::LANGUAGE.into(),
        "vbnet" => tree_sitter_vb_dotnet::LANGUAGE.into(),
        "verilog" => tree_sitter_verilog::LANGUAGE.into(),
        "hcl" => tree_sitter_hcl::LANGUAGE.into(),
        "apex" => tree_sitter_sfapex::apex::LANGUAGE.into(),
        _ => return None,
    })
}

fn descendant<'a>(node: Node<'a>, kinds: &[&str]) -> Option<Node<'a>> {
    let mut stack = vec![node];
    for _ in 0..256 {
        let node = stack.pop()?;
        if kinds.contains(&node.kind()) {
            return Some(node);
        }
        let mut cursor = node.walk();
        let children: Vec<_> = node.named_children(&mut cursor).collect();
        stack.extend(children.into_iter().rev());
    }
    None
}
pub fn call_target<'a>(node: Node<'a>, language: &str) -> Option<Node<'a>> {
    match (language, node.kind()) {
        ("objc", "message_expression") => node.child_by_field_name("method"),
        ("vbnet", "invocation") => node.child_by_field_name("target"),
        ("powershell", "command") => node.child_by_field_name("command_name"),
        ("julia", "call_expression" | "broadcast_call_expression")
            if node.parent().is_none_or(|p| p.kind() != "signature") =>
        {
            node.named_child(0)
        }
        ("erlang", "call") => node.child_by_field_name("expr"),
        ("fortran", "subroutine_call") => node.child_by_field_name("subroutine"),
        ("pascal", "exprCall") => node.child_by_field_name("entity"),
        ("hcl", "function_call") => descendant(node, &["identifier"]),
        ("verilog", "tf_call") => descendant(node, &["simple_identifier", "escaped_identifier"]),
        _ => None,
    }
}
pub fn procedure_scope(kind: &str, language: &str) -> bool {
    matches!(
        (language, kind),
        (
            "powershell",
            "function_statement" | "class_method_definition"
        ) | ("erlang", "function_clause")
            | ("commonlisp", "defun")
            | ("fortran", "function" | "subroutine")
    )
}
pub fn container_scope(kind: &str, language: &str) -> bool {
    matches!(
        (language, kind),
        ("powershell", "class_statement")
            | ("objc", "class_interface" | "class_implementation")
            | ("fortran", "module")
            | ("julia", "module_definition" | "struct_definition")
            | (
                "vbnet",
                "module_block"
                    | "namespace_block"
                    | "class_block"
                    | "structure_block"
                    | "interface_block"
            )
            | ("verilog", "module_declaration" | "package_declaration")
    )
}
fn lisp_head_is(node: Node<'_>, source: &str, names: &[&str]) -> bool {
    node.kind() == "list_lit"
        && node.named_child(0).is_some_and(|head| {
            head.utf8_text(source.as_bytes()).ok().is_some_and(|name| {
                names
                    .iter()
                    .any(|expected| name.eq_ignore_ascii_case(expected))
            })
        })
}
/// A declaration directly in FLET/LABELS, rather than a call with a list argument.
pub(crate) fn lisp_local_function<'a>(node: Node<'a>, source: &str) -> Option<Node<'a>> {
    if node.kind() != "list_lit"
        || node.named_child(0)?.kind() != "sym_lit"
        || node.named_child(1)?.kind() != "list_lit"
    {
        return None;
    }
    let bindings = node.parent()?;
    let form = bindings.parent()?;
    (form.named_child(1) == Some(bindings) && lisp_head_is(form, source, &["flet", "labels"]))
        .then_some(form)
}
pub(crate) fn lisp_function_form(node: Node<'_>, source: &str) -> bool {
    lisp_head_is(node, source, &["flet", "labels"])
}
pub(crate) fn lisp_flet(node: Node<'_>, source: &str) -> bool {
    lisp_head_is(node, source, &["flet"])
}
/// Known positional lambda lists only. Keyword/destructuring/reader-dependent forms remain unknown.
pub(crate) fn lisp_arity(node: Node<'_>, source: &str) -> Option<(usize, Option<usize>)> {
    let lambda = if node.kind() == "defun" {
        node.named_child(0)?.child_by_field_name("lambda_list")?
    } else if lisp_local_function(node, source).is_some() {
        node.named_child(1)?
    } else {
        return None;
    };
    let mut required = 0;
    let mut optional = 0;
    let mut optional_mode = false;
    let mut rest = false;
    let mut rest_name_pending = false;
    let mut cursor = lambda.walk();
    for parameter in lambda.named_children(&mut cursor) {
        if parameter.kind() == "comment" {
            continue;
        }
        let value = parameter.utf8_text(source.as_bytes()).ok()?;
        if rest_name_pending {
            if parameter.kind() != "sym_lit" || value.starts_with('&') {
                return None;
            }
            rest_name_pending = false;
            continue;
        }
        if value.eq_ignore_ascii_case("&optional") && !optional_mode && !rest {
            optional_mode = true;
        } else if value.eq_ignore_ascii_case("&rest") && !rest {
            rest = true;
            rest_name_pending = true;
        } else if value.starts_with('&') || rest {
            return None;
        } else if parameter.kind() == "sym_lit" || optional_mode && parameter.kind() == "list_lit" {
            if optional_mode {
                optional += 1;
            } else {
                required += 1;
            }
        } else {
            return None;
        }
    }
    if rest_name_pending {
        return None;
    }
    Some((required, (!rest).then_some(required + optional)))
}
pub(crate) fn verilog_task_symbol(node: Node<'_>) -> Option<(&'static str, Node<'_>)> {
    Some(("function", descendant(node, &["task_identifier"])?))
}
fn lisp_declaration_header(node: Node<'_>, source: &str) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    // Class superclass lists and direct slot headers describe declarations.
    if parent.named_child(2) == Some(node) && lisp_head_is(parent, source, &["defclass"])
        || parent.parent().is_some_and(|form| {
            form.named_child(3) == Some(parent) && lisp_head_is(form, source, &["defclass"])
        })
    {
        return true;
    }
    // Direct entries in an explicit binding/declaration list are declarations.
    if parent.parent().is_some_and(|form| {
        form.named_child(1) == Some(parent)
            && lisp_head_is(
                form,
                source,
                &[
                    "let",
                    "let*",
                    "flet",
                    "labels",
                    "macrolet",
                    "symbol-macrolet",
                ],
            )
    }) {
        return true;
    }
    // The second child of a local function declaration is its lambda list.
    if parent.named_child(1) == Some(node)
        && parent
            .parent()
            .and_then(|bindings| bindings.parent().map(|form| (bindings, form)))
            .is_some_and(|(bindings, form)| {
                form.named_child(1) == Some(bindings)
                    && lisp_head_is(form, source, &["flet", "labels", "macrolet"])
            })
    {
        return true;
    }
    parent.named_child(2) == Some(node)
        && lisp_head_is(parent, source, &["defmacro", "deftype", "defgeneric"])
}
pub(crate) fn commonlisp_literal_data(node: Node<'_>, source: &str) -> bool {
    let mut ancestor = node.parent();
    for _ in 0..128 {
        let Some(parent) = ancestor else {
            return false;
        };
        if matches!(parent.kind(), "quoting_lit" | "syn_quoting_lit" | "vec_lit")
            || lisp_head_is(parent, source, &["quote"])
        {
            return true;
        }
        ancestor = parent.parent();
    }
    true
}
pub fn commonlisp_call_target<'a>(node: Node<'a>, source: &str) -> Option<Node<'a>> {
    if node.kind() != "list_lit" || lisp_declaration_header(node, source) {
        return None;
    }
    let head = node
        .named_child(0)
        .filter(|head| head.kind() == "sym_lit")?;
    let special = |head: Node<'_>| {
        head.utf8_text(source.as_bytes()).ok().is_some_and(|value| {
            matches!(
                value.to_ascii_lowercase().as_str(),
                "quote"
                    | "function"
                    | "lambda"
                    | "let"
                    | "let*"
                    | "flet"
                    | "labels"
                    | "if"
                    | "progn"
                    | "block"
                    | "return-from"
                    | "catch"
                    | "throw"
                    | "unwind-protect"
                    | "setq"
                    | "setf"
                    | "defvar"
                    | "defparameter"
                    | "defconstant"
                    | "defclass"
                    | "defstruct"
                    | "defgeneric"
                    | "defmethod"
                    | "defmacro"
                    | "eval-when"
                    | "tagbody"
                    | "go"
                    | "the"
                    | "locally"
                    | "declare"
                    | "macrolet"
                    | "symbol-macrolet"
                    | "multiple-value-bind"
                    | "multiple-value-call"
                    | "load-time-value"
                    | "progv"
            )
        })
    };
    if special(head) {
        return None;
    }
    let mut ancestor = node.parent();
    for _ in 0..128 {
        let Some(parent) = ancestor else {
            return Some(head);
        };
        if matches!(
            parent.kind(),
            "defun_header" | "quoting_lit" | "syn_quoting_lit" | "var_quoting_lit" | "vec_lit"
        ) {
            return None;
        }
        if parent.kind() == "list_lit"
            && parent.named_child(0).is_some_and(|head| {
                head.utf8_text(source.as_bytes())
                    .ok()
                    .is_some_and(|value| value.eq_ignore_ascii_case("quote"))
            })
        {
            return None;
        }
        ancestor = parent.parent();
    }
    None
}
pub fn explicit_function_reference(node: Node<'_>, language: &str, source: &str) -> bool {
    if !matches!(
        (language, node.kind()),
        ("erlang", "atom") | ("commonlisp", "sym_lit")
    ) {
        return false;
    }
    let Some(parent) = node.parent() else {
        return false;
    };
    if language == "erlang" {
        return node.kind() == "atom"
            && parent.kind() == "internal_fun"
            && parent.child_by_field_name("fun") == Some(node);
    }
    let selected = parent.kind() == "var_quoting_lit"
        || parent.kind() == "list_lit"
            && parent.named_child(1) == Some(node)
            && parent.named_child(0).is_some_and(|head| {
                head.utf8_text(source.as_bytes())
                    .ok()
                    .is_some_and(|name| name.eq_ignore_ascii_case("function"))
            });
    if !selected {
        return false;
    }
    let mut ancestor = parent.parent();
    for _ in 0..128 {
        let Some(next) = ancestor else {
            return true;
        };
        if matches!(next.kind(), "quoting_lit" | "syn_quoting_lit" | "vec_lit")
            || next.kind() == "list_lit"
                && next.named_child(0).is_some_and(|head| {
                    head.utf8_text(source.as_bytes())
                        .ok()
                        .is_some_and(|name| name.eq_ignore_ascii_case("quote"))
                })
        {
            return false;
        }
        ancestor = next.parent();
    }
    false
}
pub fn symbol<'a>(node: Node<'a>, language: &str, text: &str) -> Option<(&'static str, Node<'a>)> {
    if language == "objc" && matches!(node.kind(), "method_definition" | "method_declaration") {
        let mut cursor = node.walk();
        return Some((
            "method",
            node.named_children(&mut cursor)
                .find(|child| child.kind() == "identifier")?,
        ));
    }
    if language == "powershell" && node.kind() == "class_method_definition" {
        return Some(("method", descendant(node, &["simple_name"])?));
    }
    if language == "fortran" && node.kind() == "module" {
        return Some((
            "module",
            descendant(descendant(node, &["module_statement"])?, &["name"])?,
        ));
    }
    if language == "verilog" && node.kind() == "function_declaration" {
        return Some(("function", descendant(node, &["function_identifier"])?));
    }
    if language == "commonlisp" && node.kind() == "list_lit" {
        let keyword = node.named_child(0)?.utf8_text(text.as_bytes()).ok()?;
        let kind = match keyword.to_ascii_lowercase().as_str() {
            "defclass" | "defstruct" => "class",
            "defmethod" => "method",
            "defgeneric" | "defmacro" => "function",
            _ => return None,
        };
        return Some((kind, node.named_child(1)?));
    }
    if language == "pascal" && node.kind() == "declType" {
        return Some((
            if node
                .child_by_field_name("type")
                .is_some_and(|value| value.kind() == "declClass")
            {
                "class"
            } else {
                "type"
            },
            node.child_by_field_name("name")?,
        ));
    }
    if language == "r" {
        if node.kind() == "argument"
            && node
                .child_by_field_name("value")
                .is_some_and(|n| n.kind() == "function_definition")
        {
            return Some(("method", node.child_by_field_name("name")?));
        }
        if node.kind() == "binary_operator" {
            if node
                .child_by_field_name("rhs")
                .is_some_and(|n| n.kind() == "function_definition")
                && let Some(lhs) = node
                    .child_by_field_name("lhs")
                    .filter(|n| n.kind() == "call")
            {
                let function = lhs
                    .child_by_field_name("function")?
                    .utf8_text(text.as_bytes())
                    .ok()?;
                if matches!(function, "method" | "S7::method") {
                    let args = lhs.child_by_field_name("arguments")?;
                    let mut cursor = args.walk();
                    let first = args
                        .named_children(&mut cursor)
                        .find(|n| n.kind() == "argument")?;
                    return Some(("method", first.child_by_field_name("value")?));
                }
            }
            if let Some(rhs) = node
                .child_by_field_name("rhs")
                .filter(|n| n.kind() == "call")
            {
                let constructor = rhs
                    .child_by_field_name("function")?
                    .utf8_text(text.as_bytes())
                    .ok()?;
                if matches!(
                    constructor,
                    "R6::R6Class" | "R6Class" | "S7::new_class" | "new_class"
                ) {
                    return Some(("class", node.child_by_field_name("lhs")?));
                }
            }
        }
        if node.kind() == "call" {
            let target = node
                .child_by_field_name("function")?
                .utf8_text(text.as_bytes())
                .ok()?;
            let kind = match target {
                "setClass" | "methods::setClass" => "class",
                "setMethod" | "methods::setMethod" => "method",
                "setGeneric" | "methods::setGeneric" => "function",
                _ => return None,
            };
            let args = node.child_by_field_name("arguments")?;
            let mut cursor = args.walk();
            let argument = args
                .named_children(&mut cursor)
                .find(|n| n.kind() == "argument")?;
            let literal = argument
                .child_by_field_name("value")
                .filter(|n| n.kind() == "string")?;
            let content = literal.child_by_field_name("content")?;
            if content.child_count() == 0 {
                return Some((kind, content));
            }
        }
    }
    let k = node.kind();
    let kind = match (language, k) {
        ("powershell", "function_statement")
        | ("erlang", "function_clause")
        | ("fortran", "function" | "subroutine")
        | ("commonlisp", "defun")
        | ("pascal", "declProc") => "function",
        ("powershell", "class_statement")
        | ("objc", "class_interface" | "class_implementation")
        | ("vbnet", "class_block") => "class",
        ("julia", "struct_definition") | ("vbnet", "structure_block") => "struct",
        ("julia", "module_definition")
        | ("vbnet", "module_block" | "namespace_block")
        | ("verilog", "module_declaration" | "package_declaration") => "module",
        ("vbnet", "interface_block") => "interface",
        ("vbnet", "enum_block") => "enum",
        ("hcl", "block") => "configuration",
        ("julia", "function_definition") => "function",
        _ => return None,
    };
    let name = if language == "hcl" {
        let mut cursor = node.walk();
        let labels: Vec<_> = node
            .named_children(&mut cursor)
            .filter(|n| n.kind() == "string_lit")
            .collect();
        descendant(*labels.last()?, &["template_literal"])?
    } else if let Some(name) = node.child_by_field_name("name") {
        name
    } else {
        let fields = match language {
            "powershell" => &["function_name", "simple_name"][..],
            "julia" => &["identifier"][..],
            "erlang" => &["atom"][..],
            "commonlisp" => &["sym_lit"][..],
            "verilog" => &["simple_identifier", "escaped_identifier"][..],
            _ => &["identifier"][..],
        };
        if language == "fortran" {
            descendant(node, &["subroutine_statement", "function_statement"])?
                .child_by_field_name("name")?
        } else if language == "commonlisp" {
            descendant(node, &["defun_header"])?.child_by_field_name("function_name")?
        } else {
            descendant(node, fields)?
        }
    };
    Some((kind, name))
}

/// Independent syntax units retain their original immutable source offsets.
#[derive(Debug)]
pub struct EmbeddedUnit {
    pub start: usize,
    pub end: usize,
    pub language: &'static str,
    pub unsupported: Option<String>,
}
fn attributes(text: &str) -> Result<std::collections::BTreeMap<String, String>> {
    let mut result = std::collections::BTreeMap::new();
    let bytes = text.as_bytes();
    let mut at = 0;
    while at < bytes.len() {
        while at < bytes.len() && bytes[at].is_ascii_whitespace() {
            at += 1;
        }
        if at == bytes.len() {
            break;
        }
        let start = at;
        while at < bytes.len()
            && (bytes[at].is_ascii_alphanumeric() || matches!(bytes[at], b'-' | b'_' | b':'))
        {
            at += 1;
        }
        if at == start {
            at += 1;
            continue;
        }
        let name = text[start..at].to_ascii_lowercase();
        while at < bytes.len() && bytes[at].is_ascii_whitespace() {
            at += 1;
        }
        let mut value = String::new();
        if bytes.get(at) == Some(&b'=') {
            at += 1;
            while at < bytes.len() && bytes[at].is_ascii_whitespace() {
                at += 1;
            }
            if let Some(quote @ (b'"' | b'\'')) = bytes.get(at).copied() {
                at += 1;
                let start = at;
                while at < bytes.len() && bytes[at] != quote {
                    at += 1;
                }
                value = text[start..at].into();
                if at < bytes.len() {
                    at += 1;
                }
            } else {
                let start = at;
                while at < bytes.len() && !bytes[at].is_ascii_whitespace() {
                    at += 1;
                }
                value = text[start..at].into();
            }
        }
        ensure!(result.len() < 256, "Embedded attribute capacity exceeded");
        let decoded = quick_xml::escape::unescape(&value)
            .map_or_else(|_| value.clone(), |value| value.into_owned());
        result.entry(name).or_insert(decoded);
    }
    Ok(result)
}
pub fn embedded_units(language: &str, text: &str) -> Result<Option<Vec<EmbeddedUnit>>> {
    if !matches!(
        language,
        "vue" | "svelte" | "astro" | "ejs" | "blade" | "razor"
    ) {
        return Ok(None);
    }
    let mut units = Vec::new();
    let mut add = |start, end, language, unsupported| -> Result<()> {
        ensure!(units.len() < 1000, "Embedded segment budget exceeded");
        units.push(EmbeddedUnit {
            start,
            end,
            language,
            unsupported,
        });
        Ok(())
    };
    if language == "ejs" {
        let mut at = 0;
        while let Some(start) = text[at..].find("<%") {
            let mut start = at + start + 2;
            let Some(close) = text[start..].find("%>") else {
                break;
            };
            let mut end = start + close;
            at = end + 2;
            if matches!(text.as_bytes().get(start), Some(b'#' | b'%')) {
                continue;
            }
            if matches!(text.as_bytes().get(start), Some(b'=' | b'-' | b'_')) {
                start += 1;
            }
            if matches!(
                text.as_bytes().get(end.saturating_sub(1)),
                Some(b'-' | b'_')
            ) {
                end -= 1;
            }
            add(start, end, "javascript", None)?;
        }
    } else if language == "blade" {
        let segments = regex::Regex::new(
            r"(?s)\{\{--.*?--\}\}|@verbatim.*?@endverbatim|@php\b(.*?)@endphp|<\?php\b(.*?)\?>",
        )?;
        for captures in segments.captures_iter(text) {
            if let Some(code) = captures.get(1).or_else(|| captures.get(2)) {
                add(code.start(), code.end(), "php-only", None)?;
            }
        }
    } else if language == "razor" {
        let starts = regex::Regex::new(r"(?s)@\*.*?\*@|<!--.*?-->|@(?:code|functions)\s*\{|@\{")?;
        let mut consumed = 0;
        for start in starts.find_iter(text) {
            if start.start() < consumed
                || start.as_str().starts_with("@*")
                || start.as_str().starts_with("<!--")
            {
                continue;
            }
            let open = start.end() - 1;
            match brace_end(text, open) {
                Ok(close) => {
                    add(open + 1, close, "csharp", None)?;
                    consumed = close + 1;
                }
                Err(_) => {
                    add(
                        open,
                        text.len(),
                        "csharp",
                        Some("Unterminated embedded code block".into()),
                    )?;
                    break;
                }
            }
        }
    } else {
        let scripts = regex::Regex::new(
            r#"(?is)<!--.*?-->|<script\b((?:"[^"]*"|'[^']*'|[^'">])*)>(.*?)</script\s*>"#,
        )?;
        for captures in scripts.captures_iter(text) {
            let Some(code) = captures.get(2) else {
                continue;
            };
            let attrs = attributes(captures.get(1).unwrap().as_str())?;
            if attrs.get("type").is_some_and(|kind| {
                !matches!(
                    kind.to_ascii_lowercase().as_str(),
                    "module" | "text/javascript" | "application/javascript" | "text/typescript"
                )
            }) {
                continue;
            }
            let selected = attrs
                .get("lang")
                .map_or("javascript", String::as_str)
                .to_ascii_lowercase();
            let grammar = match selected.as_str() {
                "js" | "javascript" => "javascript",
                "jsx" => "jsx",
                "ts" | "typescript" => "typescript",
                "tsx" => "tsx",
                _ => {
                    add(
                        code.start(),
                        code.end(),
                        "text",
                        Some(format!("Unsupported embedded language: {selected}")),
                    )?;
                    continue;
                }
            };
            add(code.start(), code.end(), grammar, None)?;
        }
        if language == "astro"
            && text.starts_with("---")
            && let Some(close) = text[3..].find("\n---")
        {
            add(3, 3 + close, "typescript", None)?;
        }
    }
    units.sort_by_key(|unit| unit.start);
    Ok(Some(units))
}
fn brace_end(text: &str, start: usize) -> Result<usize> {
    brace_end_at(text, start, 0)
}
fn brace_end_at(text: &str, start: usize, level: usize) -> Result<usize> {
    ensure!(level < 128, "Template interpolation depth exceeded");
    let bytes = text.as_bytes();
    let mut at = start;
    let mut depth = 0usize;
    while at < bytes.len() {
        match bytes[at] {
            b'/' if bytes.get(at + 1) == Some(&b'/') => {
                while at < bytes.len() && bytes[at] != b'\n' {
                    at += 1;
                }
            }
            b'/' if bytes.get(at + 1) == Some(&b'*') => {
                at += 2;
                while at + 1 < bytes.len() && &bytes[at..at + 2] != b"*/" {
                    at += 1;
                }
                at = (at + 2).min(bytes.len());
            }
            quote @ (b'"' | b'\'') => {
                let quotes = bytes[at..].iter().take_while(|byte| **byte == b'"').count();
                if quote == b'"' && quotes >= 3 {
                    at += quotes;
                    while at < bytes.len() {
                        let run = bytes[at..].iter().take_while(|byte| **byte == b'"').count();
                        if run >= quotes {
                            at += quotes;
                            break;
                        }
                        at += run.max(1);
                    }
                    continue;
                }
                let verbatim = quote == b'"' && at > 0 && bytes[at - 1] == b'@';
                let interpolated = quote == b'"'
                    && (at > 0 && bytes[at - 1] == b'$' || at > 1 && &bytes[at - 2..at] == b"$@");
                at += 1;
                while at < bytes.len() {
                    if interpolated && bytes[at] == b'{' {
                        if bytes.get(at + 1) == Some(&b'{') {
                            at += 2;
                        } else {
                            at = brace_end_at(text, at, level + 1)? + 1;
                        }
                        continue;
                    }
                    if !verbatim && bytes[at] == b'\\' {
                        at = (at + 2).min(bytes.len());
                        continue;
                    }
                    if bytes[at] == quote {
                        if verbatim && bytes.get(at + 1) == Some(&quote) {
                            at += 2;
                            continue;
                        }
                        at += 1;
                        break;
                    }
                    at += 1;
                }
            }
            b'{' => {
                depth += 1;
                ensure!(depth <= 256, "Template brace depth exceeded");
                at += 1;
            }
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Ok(at);
                }
                at += 1;
            }
            _ => at += 1,
        }
    }
    anyhow::bail!("Unterminated template code block")
}

#[derive(Debug)]
pub struct PatternDeclaration<'a> {
    pub name: &'a str,
    pub kind: &'static str,
    pub start: usize,
    pub end: usize,
    pub name_start: usize,
    pub name_end: usize,
}
pub fn pattern_declarations<'a>(
    language: &str,
    text: &'a str,
    deadline: &crate::capture::Deadline,
) -> Result<Vec<PatternDeclaration<'a>>> {
    let pattern = match language {
        "cobol" => {
            r"(?im)^[ \t]*(?:[0-9]{6}[ ]?)?(?:PROGRAM-ID[.]\s+([A-Z0-9_-]+)[.]|([A-Z][A-Z0-9_-]*)\s+SECTION[.])"
        }
        "dreammaker" => {
            r"(?m)^[ \t]*(?:/?[A-Za-z_][A-Za-z0-9_/]*/)?proc/([A-Za-z_][A-Za-z0-9_]*)\s*\([^\n]*\)"
        }
        "robot" => r"(?m)^([^\s*#][^\n\r]*)$",
        _ => return Ok(Vec::new()),
    };
    let regex = regex::Regex::new(pattern)?;
    let mut sections = Vec::new();
    if language == "robot" {
        let mut offset = 0;
        for line in text.split_inclusive('\n') {
            deadline.check()?;
            if line.trim_start().starts_with("***") {
                let header = line.to_ascii_lowercase();
                sections.push((
                    offset,
                    header.contains("keywords")
                        || header.contains("test cases")
                        || header.contains("tasks"),
                ));
                ensure!(sections.len() <= 10_000, "Robot section capacity exceeded");
            }
            offset += line.len();
        }
    }
    let mut section = 0;
    let mut in_definition_section = false;
    let mut declarations = Vec::new();
    for captures in regex.captures_iter(text) {
        deadline.check()?;
        ensure!(
            declarations.len() < 100_000,
            "Structural declaration capacity exceeded"
        );
        let whole = captures.get(0).unwrap();
        let name = captures.get(1).or_else(|| captures.get(2)).unwrap();
        if language == "robot" {
            while sections
                .get(section)
                .is_some_and(|(offset, _)| *offset <= whole.start())
            {
                in_definition_section = sections[section].1;
                section += 1;
            }
            if !in_definition_section {
                continue;
            }
        }
        declarations.push(PatternDeclaration {
            name: name.as_str().trim_end(),
            kind: if language == "cobol" && captures.get(1).is_some() {
                "module"
            } else {
                "function"
            },
            start: whole.start(),
            end: whole.end(),
            name_start: name.start(),
            name_end: name.end() - (name.as_str().len() - name.as_str().trim_end().len()),
        });
    }
    for i in 0..declarations.len() {
        declarations[i].end = declarations
            .get(i + 1)
            .map_or(text.len(), |next| next.start);
    }
    Ok(declarations)
}
