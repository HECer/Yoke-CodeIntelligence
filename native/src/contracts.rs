//! Independent source-bound contract extraction.
//!
//! Declarations and literal syntax are facts about source. Name-based relationships
//! remain candidates, including uniquely named matches within a project scope.
use crate::{
    capture::Deadline,
    model::{Diagnostic, Entity, Extraction, RawEdge, Snapshot, SourceFile},
};
use anyhow::{Result, bail, ensure};
use std::collections::{BTreeMap, BTreeSet};
use tree_sitter::Node;

const PRODUCER: &str = "yoke-contracts-1";
const MAX_CONTRACTS: usize = 20_000;
fn line(text: &str, byte: usize) -> usize {
    text.as_bytes()[..byte.min(text.len())]
        .iter()
        .filter(|b| **b == b'\n')
        .count()
        + 1
}
#[allow(clippy::too_many_arguments)]
fn add(
    file: &SourceFile,
    text: &str,
    output: &mut Extraction,
    parent: &Entity,
    kind: &str,
    name: &str,
    domain: &str,
    start: usize,
    end: usize,
    name_range: Option<(usize, usize)>,
) -> Result<Entity> {
    ensure!(
        output.entities.len() < MAX_CONTRACTS,
        "Contract entity budget exceeded"
    );
    ensure!(
        start <= end
            && end <= text.len()
            && text.is_char_boundary(start)
            && text.is_char_boundary(end),
        "Invalid contract source range"
    );
    let item = Entity {
        id: String::new(),
        key: format!("contract:{}:{kind}:{start}:{name}", file.path),
        snapshot_id: String::new(),
        path: file.path.clone(),
        name: name.into(),
        qualified_name: domain.into(),
        kind: kind.into(),
        language: file.language.clone(),
        signature: if kind == "configuration_key" {
            "Environment variable name only; no value stored.".into()
        } else {
            text[start..end].chars().take(800).collect()
        },
        start_byte: start,
        end_byte: end,
        start_line: line(text, start),
        end_line: line(text, end.saturating_sub(1)),
        name_start_byte: name_range.map(|r| r.0),
        name_end_byte: name_range.map(|r| r.1),
        parent_key: Some(parent.key.clone()),
        producer: PRODUCER.into(),
    };
    output.edges.push(edge(
        parent,
        &item.key,
        if kind == "schema_field" {
            "has_field"
        } else {
            "declares_contract"
        },
    ));
    // The declaration's evidence is the declaration rather than its enclosing file.
    if let Some(raw) = output.edges.last_mut() {
        raw.start_byte = start;
        raw.end_byte = end;
        raw.start_line = item.start_line;
        raw.end_line = item.end_line;
    }
    output.entities.push(item.clone());
    Ok(item)
}
fn edge(source: &Entity, target: &str, relation: &str) -> RawEdge {
    RawEdge {
        source_key: source.key.clone(),
        target_label: target.into(),
        relation: relation.into(),
        start_byte: source.start_byte,
        end_byte: source.end_byte,
        start_line: source.start_line,
        end_line: source.end_line,
    }
}
fn diagnostic(file: &SourceFile, output: &mut Extraction, byte: usize, message: impl Into<String>) {
    if output.diagnostics.len() < 200 {
        output.diagnostics.push(Diagnostic {
            path: file.path.clone(),
            start_byte: byte,
            message: message.into(),
            severity: "error".into(),
            producer: PRODUCER.into(),
        });
    }
}
fn env_name(value: &str) -> bool {
    let mut chars = value.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}
fn literal(node: Node<'_>, text: &str) -> Option<String> {
    if !matches!(
        node.kind(),
        "string" | "string_literal" | "interpreted_string_literal" | "template_string"
    ) {
        return None;
    }
    let raw = node.utf8_text(text.as_bytes()).ok()?;
    if raw.len() > 4098 {
        return None;
    }
    if raw.starts_with('"') {
        return serde_json::from_str::<String>(raw).ok();
    }
    let quote = raw.chars().next()?;
    if !['\'', '`'].contains(&quote) || !raw.ends_with(quote) || raw.len() < 2 || raw.contains("${")
    {
        return None;
    }
    let mut value = String::new();
    let mut chars = raw[1..raw.len() - 1].chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            let next = chars.next()?;
            value.push(match next {
                '\\' => '\\',
                '\'' => '\'',
                '"' => '"',
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                _ => return None,
            });
        } else {
            value.push(ch);
        }
    }
    Some(value)
}
fn callee(node: Node<'_>, text: &str) -> Option<String> {
    let raw = node.utf8_text(text.as_bytes()).ok()?;
    (raw.len() <= 200
        && raw
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_.$:".contains(c)))
    .then(|| raw.into())
}
pub fn extract_code_node(
    file: &SourceFile,
    text: &str,
    node: Node<'_>,
    owner: &Entity,
    output: &mut Extraction,
) -> Result<()> {
    if matches!(
        file.language.as_str(),
        "typescript" | "tsx" | "javascript" | "jsx"
    ) && node.kind() == "member_expression"
        && let Some(raw) = callee(node, text).filter(|s| s.starts_with("process.env."))
    {
        let name = &raw[12..];
        if env_name(name) {
            add(
                file,
                text,
                output,
                owner,
                "configuration_key",
                name,
                &format!("env:{name}"),
                node.start_byte(),
                node.end_byte(),
                None,
            )?;
        }
    }
    if !matches!(node.kind(), "call_expression" | "call") {
        return Ok(());
    }
    let Some(function) = node.child_by_field_name("function") else {
        return Ok(());
    };
    let Some(name) = callee(function, text) else {
        return Ok(());
    };
    let Some(arguments) = node.child_by_field_name("arguments") else {
        return Ok(());
    };
    let value = arguments
        .named_child(0)
        .and_then(|first| literal(first, text));
    let start = node.start_byte();
    let end = node.end_byte();
    if [
        "getenv",
        "os.getenv",
        "os.environ.get",
        "environ.get",
        "std::env::var",
        "std::env::var_os",
    ]
    .contains(&name.as_str())
    {
        if let Some(value) = value.as_deref().filter(|v| env_name(v)) {
            add(
                file,
                text,
                output,
                owner,
                "configuration_key",
                value,
                &format!("env:{value}"),
                start,
                end,
                None,
            )?;
        }
        return Ok(());
    }
    if let Some((receiver, action)) = name.rsplit_once('.') {
        if ["emit", "publish", "subscribe", "on", "once"].contains(&action)
            && let Some(value) = value.as_deref().filter(|v| !v.is_empty())
        {
            add(
                file,
                text,
                output,
                owner,
                if ["emit", "publish"].contains(&action) {
                    "event_publisher"
                } else {
                    "event_subscriber"
                },
                value,
                &format!("event:{value}"),
                start,
                end,
                None,
            )?;
        }
        if let Some((database, model)) = receiver.split_once('.')
            && ["prisma", "db"].contains(&database)
            && env_name(model)
            && [
                "findMany",
                "findUnique",
                "findFirst",
                "create",
                "createMany",
                "update",
                "updateMany",
                "delete",
                "deleteMany",
                "upsert",
                "count",
                "aggregate",
                "groupBy",
            ]
            .contains(&action)
        {
            add(
                file,
                text,
                output,
                owner,
                "orm_operation",
                model,
                &format!("orm:{}", model.to_ascii_lowercase()),
                start,
                end,
                None,
            )?;
        }
        if ["app", "router"].contains(&receiver)
            && ["get", "post", "put", "patch", "delete", "head", "options"].contains(&action)
            && let Some(value) = value.as_deref().filter(|v| v.starts_with('/'))
        {
            let domain = format!("http:{}:{value}", action.to_ascii_uppercase());
            add(
                file,
                text,
                output,
                owner,
                "route",
                &format!("{} {value}", action.to_ascii_uppercase()),
                &domain,
                start,
                end,
                None,
            )?;
        }
    }
    if name == "fetch" || name.starts_with("axios.") {
        let Some(value) = value.filter(|v| !v.is_empty()) else {
            return Ok(());
        };
        let mut method = if name == "fetch" {
            "GET".to_string()
        } else {
            name[6..].to_ascii_uppercase()
        };
        if !["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"].contains(&method.as_str())
        {
            return Ok(());
        }
        if name == "fetch"
            && let Some(opts) = arguments.named_child(1)
        {
            if opts.kind() != "object" {
                return Ok(());
            }
            let mut cursor = opts.walk();
            for prop in opts.named_children(&mut cursor) {
                if prop.kind() != "pair" {
                    return Ok(());
                }
                let Some(key) = prop.child_by_field_name("key") else {
                    continue;
                };
                let key_text = key.utf8_text(text.as_bytes())?.trim_matches(['\'', '"']);
                if key_text == "method" {
                    let Some(value) = prop
                        .child_by_field_name("value")
                        .and_then(|n| literal(n, text))
                    else {
                        return Ok(());
                    };
                    method = value.to_ascii_uppercase();
                }
            }
        }
        add(
            file,
            text,
            output,
            owner,
            "http_client_call",
            &format!("{method} {value}"),
            &format!("http:{method}:{value}"),
            start,
            end,
            None,
        )?;
    }
    Ok(())
}

#[derive(Clone, Debug)]
struct Token {
    value: String,
    start: usize,
    end: usize,
    kind: char,
}
fn tokenize(text: &str, sql: bool, deadline: &Deadline) -> Result<Vec<Token>> {
    let bytes = text.as_bytes();
    let mut at = 0;
    let mut out = Vec::new();
    while at < bytes.len() {
        if out.len().is_multiple_of(512) {
            deadline.check()?;
        }
        ensure!(out.len() < 200_000, "Contract token budget exceeded");
        if bytes[at].is_ascii_whitespace() {
            at += 1;
            continue;
        }
        if bytes[at..].starts_with(b"//")
            || (sql && bytes[at..].starts_with(b"--"))
            || (!sql && bytes[at] == b'#')
        {
            while at < bytes.len() && bytes[at] != b'\n' {
                at += 1;
            }
            continue;
        }
        if bytes[at..].starts_with(b"/*") {
            at += 2;
            let mut depth = 1;
            while at < bytes.len() && depth > 0 {
                if bytes[at..].starts_with(b"/*") {
                    depth += 1;
                    at += 2;
                    ensure!(depth <= 64, "Comment nesting budget exceeded");
                } else if bytes[at..].starts_with(b"*/") {
                    depth -= 1;
                    at += 2;
                } else {
                    at += 1;
                }
            }
            ensure!(depth == 0, "Unterminated contract comment");
            continue;
        }
        let start = at;
        if sql && bytes[at] == b'$' {
            let mut stop = at + 1;
            while stop < bytes.len() && (bytes[stop].is_ascii_alphanumeric() || bytes[stop] == b'_')
            {
                stop += 1;
            }
            if stop < bytes.len() && bytes[stop] == b'$' {
                let delimiter = &text[at..=stop];
                let body = stop + 1;
                let tail = text[body..]
                    .find(delimiter)
                    .ok_or_else(|| anyhow::anyhow!("Unterminated SQL dollar string"))?;
                at = body + tail + delimiter.len();
                out.push(Token {
                    value: String::new(),
                    start,
                    end: at,
                    kind: 's',
                });
                continue;
            }
        }
        let quote = bytes[at];
        if quote == b'\'' || quote == b'"' || quote == b'`' || (sql && quote == b'[') {
            let triple = !sql && bytes[at..].starts_with(b"\"\"\"");
            let closing = if quote == b'[' { b']' } else { quote };
            let width = if triple { 3 } else { 1 };
            at += width;
            let body = at;
            let mut value = String::new();
            let mut closed = false;
            while at < bytes.len() {
                if triple && bytes[at..].starts_with(b"\"\"\"") {
                    value = text[body..at].into();
                    at += 3;
                    closed = true;
                    break;
                }
                if !triple && bytes[at] == closing {
                    if sql && at + 1 < bytes.len() && bytes[at + 1] == closing {
                        value.push(closing as char);
                        at += 2;
                        continue;
                    }
                    at += 1;
                    closed = true;
                    break;
                }
                if !sql && bytes[at] == b'\\' {
                    at += 1;
                    ensure!(at < bytes.len(), "Unterminated escaped string");
                    let ch = text[at..].chars().next().unwrap();
                    value.push(ch);
                    at += ch.len_utf8();
                } else {
                    let ch = text[at..].chars().next().unwrap();
                    value.push(ch);
                    at += ch.len_utf8();
                }
            }
            ensure!(closed, "Unterminated contract string");
            out.push(Token {
                value,
                start,
                end: at,
                kind: if sql && quote != b'\'' { 'q' } else { 's' },
            });
            continue;
        }
        if bytes[at].is_ascii_alphabetic() || bytes[at] == b'_' || bytes[at] >= 128 {
            at += 1;
            while at < bytes.len()
                && (bytes[at].is_ascii_alphanumeric() || bytes[at] == b'_' || bytes[at] >= 128)
            {
                at += 1;
            }
            out.push(Token {
                value: text[start..at].into(),
                start,
                end: at,
                kind: 'i',
            });
        } else {
            at += 1;
            out.push(Token {
                value: (quote as char).to_string(),
                start,
                end: at,
                kind: 'p',
            });
        }
    }
    Ok(out)
}
fn equal(tokens: &[Token], at: usize, word: &str) -> bool {
    tokens
        .get(at)
        .is_some_and(|t| t.kind == 'i' && t.value.eq_ignore_ascii_case(word))
}
fn punct(tokens: &[Token], at: usize, value: &str) -> bool {
    tokens
        .get(at)
        .is_some_and(|t| t.kind == 'p' && t.value == value)
}
fn closing(tokens: &[Token], at: usize) -> Option<usize> {
    let open = tokens.get(at)?.value.as_str();
    let close = match open {
        "{" => "}",
        "(" => ")",
        "[" => "]",
        _ => return None,
    };
    let mut depth = 0;
    for (i, t) in tokens.iter().enumerate().skip(at) {
        if t.kind != 'p' {
            continue;
        }
        if t.value == open {
            depth += 1;
        } else if t.value == close {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
    }
    None
}
fn sql(file: &SourceFile, text: &str, output: &mut Extraction, tokens: &[Token]) -> Result<()> {
    let root = output.entities[0].clone();
    let mut i = 0;
    while i < tokens.len() {
        let (kind, mut name_at) = if equal(tokens, i, "CREATE") && equal(tokens, i + 1, "TABLE") {
            ("migration_table", i + 2)
        } else if ((equal(tokens, i, "ALTER") || equal(tokens, i, "DROP"))
            && equal(tokens, i + 1, "TABLE"))
            || (equal(tokens, i, "INSERT") && equal(tokens, i + 1, "INTO"))
        {
            ("sql_operation", i + 2)
        } else if ["FROM", "JOIN", "UPDATE"]
            .iter()
            .any(|w| equal(tokens, i, w))
        {
            ("sql_operation", i + 1)
        } else {
            i += 1;
            continue;
        };
        if equal(tokens, name_at, "IF") {
            name_at += 1;
            if equal(tokens, name_at, "NOT") {
                name_at += 1;
            }
            if equal(tokens, name_at, "EXISTS") {
                name_at += 1;
            }
        }
        let Some(token) = tokens.get(name_at).filter(|t| matches!(t.kind, 'i' | 'q')) else {
            i += 1;
            continue;
        };
        let mut name = token.value.clone();
        let mut end = token.end;
        if punct(tokens, name_at + 1, ".")
            && let Some(last) = tokens
                .get(name_at + 2)
                .filter(|t| matches!(t.kind, 'i' | 'q'))
        {
            name.push('.');
            name.push_str(&last.value);
            end = last.end;
        }
        add(
            file,
            text,
            output,
            &root,
            kind,
            &name,
            &format!("sql:{}", name.to_lowercase()),
            tokens[i].start,
            end,
            Some((token.start, end)),
        )?;
        i = name_at + 1;
    }
    Ok(())
}
fn schema(
    file: &SourceFile,
    text: &str,
    output: &mut Extraction,
    tokens: &[Token],
    format: &str,
) -> Result<()> {
    let root = output.entities[0].clone();
    let mut i = 0;
    while i < tokens.len() {
        let declaration = match format {
            "prisma" => ["model", "enum", "type"].contains(&tokens[i].value.as_str()),
            "graphql" => ["type", "input", "enum", "interface", "scalar", "union"]
                .contains(&tokens[i].value.as_str()),
            _ => ["message", "service", "enum"].contains(&tokens[i].value.as_str()),
        };
        if tokens[i].kind != 'i' || !declaration {
            i += 1;
            continue;
        }
        let Some(name) = tokens.get(i + 1).filter(|t| t.kind == 'i') else {
            i += 1;
            continue;
        };
        let mut open = i + 2;
        while open < tokens.len()
            && !punct(tokens, open, "{")
            && !tokens.get(open).is_some_and(|t| {
                t.start > tokens[i].start && line(text, t.start) > line(text, tokens[i].start)
            })
        {
            open += 1;
        }
        if !punct(tokens, open, "{") {
            if format == "graphql" && ["scalar", "union"].contains(&tokens[i].value.as_str()) {
                add(
                    file,
                    text,
                    output,
                    &root,
                    "schema_type",
                    &name.value,
                    &format!("graphql:{}", name.value),
                    tokens[i].start,
                    name.end,
                    Some((name.start, name.end)),
                )?;
            } else {
                diagnostic(
                    file,
                    output,
                    tokens[i].start,
                    "Contract declaration is missing a body",
                );
            }
            i += 2;
            continue;
        }
        let Some(close) = closing(tokens, open) else {
            diagnostic(
                file,
                output,
                tokens[i].start,
                "Unterminated contract declaration body",
            );
            i = open + 1;
            continue;
        };
        let kind = if format == "prisma" && tokens[i].value == "model" {
            "orm_model"
        } else {
            "schema_type"
        };
        let mut domain = format!("{format}:{}", name.value);
        if kind == "orm_model" {
            let mapped = (open + 1..close).find_map(|j| {
                (punct(tokens, j, "@")
                    && punct(tokens, j + 1, "@")
                    && equal(tokens, j + 2, "map")
                    && punct(tokens, j + 3, "("))
                .then(|| tokens.get(j + 4))
                .flatten()
                .filter(|t| t.kind == 's')
                .map(|t| t.value.clone())
            });
            domain = format!(
                "sql:{}",
                mapped.as_deref().unwrap_or(&name.value).to_lowercase()
            );
        }
        let end = if format == "protobuf" {
            tokens[open].end
        } else {
            tokens[close].end
        };
        let model = add(
            file,
            text,
            output,
            &root,
            kind,
            &name.value,
            &domain,
            tokens[i].start,
            end,
            Some((name.start, name.end)),
        )?;
        if format == "prisma" && tokens[i].value != "enum" {
            let mut j = open + 1;
            while j < close {
                let field = &tokens[j];
                if field.kind == 'i'
                    && tokens.get(j + 1).is_some_and(|t| t.kind == 'i')
                    && (j == open + 1 || line(text, tokens[j - 1].end) < line(text, field.start))
                {
                    let mut stop = j + 2;
                    while stop < close && line(text, tokens[stop].start) == line(text, field.start)
                    {
                        stop += 1;
                    }
                    add(
                        file,
                        text,
                        output,
                        &model,
                        "schema_field",
                        &format!("{}.{}", name.value, field.value),
                        &format!("prisma-field:{}.{}", name.value, field.value),
                        field.start,
                        tokens[stop - 1].end,
                        Some((field.start, field.end)),
                    )?;
                    j = stop;
                } else {
                    j += 1;
                }
            }
        } else if format == "graphql" && tokens[i].value != "enum" {
            let mut j = open + 1;
            while j < close {
                if tokens[j].kind != 'i' {
                    j += 1;
                    continue;
                }
                let field = &tokens[j];
                let mut colon = j + 1;
                if punct(tokens, colon, "(") {
                    colon = closing(tokens, colon).map_or(close, |n| n + 1);
                }
                if punct(tokens, colon, ":") {
                    let mut stop = colon + 1;
                    if punct(tokens, stop, "[") {
                        stop = closing(tokens, stop).map_or(close, |n| n + 1);
                    } else if tokens.get(stop).is_some_and(|t| t.kind == 'i') {
                        stop += 1;
                    } else {
                        j += 1;
                        continue;
                    }
                    if punct(tokens, stop, "!") {
                        stop += 1;
                    }
                    if stop <= close {
                        add(
                            file,
                            text,
                            output,
                            &model,
                            "schema_field",
                            &format!("{}.{}", name.value, field.value),
                            &format!("graphql-field:{}.{}", name.value, field.value),
                            field.start,
                            tokens[stop - 1].end,
                            Some((field.start, field.end)),
                        )?;
                    }
                    j = stop;
                } else {
                    j += 1;
                }
            }
        }
        i = close + 1;
    }
    Ok(())
}

#[derive(Debug)]
struct JsonNode {
    start: usize,
    end: usize,
    is_object: bool,
    members: Vec<(String, usize, JsonNode)>,
}
fn json_node(text: &str, at: &mut usize, depth: usize, deadline: &Deadline) -> Result<JsonNode> {
    ensure!(depth <= 128, "JSON contract nesting budget exceeded");
    deadline.check()?;
    let bytes = text.as_bytes();
    while bytes.get(*at).is_some_and(u8::is_ascii_whitespace) {
        *at += 1;
    }
    let start = *at;
    let mut members = Vec::new();
    let is_object = bytes.get(*at) == Some(&b'{');
    if is_object {
        *at += 1;
        let mut seen = BTreeSet::new();
        loop {
            while bytes.get(*at).is_some_and(u8::is_ascii_whitespace) {
                *at += 1;
            }
            if bytes.get(*at) == Some(&b'}') {
                *at += 1;
                break;
            }
            let key_start = *at;
            let key = json_string(text, at)?;
            ensure!(
                seen.insert(key.clone()),
                "Duplicate JSON contract key {key}"
            );
            while bytes.get(*at).is_some_and(u8::is_ascii_whitespace) {
                *at += 1;
            }
            ensure!(bytes.get(*at) == Some(&b':'), "Expected JSON colon");
            *at += 1;
            let value = json_node(text, at, depth + 1, deadline)?;
            members.push((key, key_start, value));
            while bytes.get(*at).is_some_and(u8::is_ascii_whitespace) {
                *at += 1;
            }
            if bytes.get(*at) == Some(&b'}') {
                *at += 1;
                break;
            }
            ensure!(bytes.get(*at) == Some(&b','), "Expected JSON comma");
            *at += 1;
            while bytes.get(*at).is_some_and(u8::is_ascii_whitespace) {
                *at += 1;
            }
            ensure!(bytes.get(*at) != Some(&b'}'), "Trailing JSON comma");
        }
    } else if bytes.get(*at) == Some(&b'[') {
        *at += 1;
        loop {
            while bytes.get(*at).is_some_and(u8::is_ascii_whitespace) {
                *at += 1;
            }
            if bytes.get(*at) == Some(&b']') {
                *at += 1;
                break;
            }
            json_node(text, at, depth + 1, deadline)?;
            while bytes.get(*at).is_some_and(u8::is_ascii_whitespace) {
                *at += 1;
            }
            if bytes.get(*at) == Some(&b']') {
                *at += 1;
                break;
            }
            ensure!(bytes.get(*at) == Some(&b','), "Expected JSON array comma");
            *at += 1;
            while bytes.get(*at).is_some_and(u8::is_ascii_whitespace) {
                *at += 1;
            }
            ensure!(bytes.get(*at) != Some(&b']'), "Trailing JSON array comma");
        }
    } else if bytes.get(*at) == Some(&b'"') {
        json_string(text, at)?;
    } else {
        while bytes
            .get(*at)
            .is_some_and(|b| !b.is_ascii_whitespace() && !b",]}".contains(b))
        {
            *at += 1;
        }
        ensure!(*at > start, "Missing JSON value");
        serde_json::from_str::<serde_json::Value>(&text[start..*at])?;
    }
    Ok(JsonNode {
        start,
        end: *at,
        is_object,
        members,
    })
}
fn json_string(text: &str, at: &mut usize) -> Result<String> {
    let bytes = text.as_bytes();
    let start = *at;
    ensure!(bytes.get(*at) == Some(&b'"'), "Expected JSON string");
    *at += 1;
    while *at < bytes.len() {
        if bytes[*at] == b'"' {
            *at += 1;
            return Ok(serde_json::from_str(&text[start..*at])?);
        }
        if bytes[*at] == b'\\' {
            *at += 1;
        }
        *at += 1;
    }
    bail!("Unterminated JSON string")
}
fn json_document(
    file: &SourceFile,
    text: &str,
    output: &mut Extraction,
    deadline: &Deadline,
) -> Result<()> {
    let basename = file.path.rsplit('/').next().unwrap_or("");
    let jsonc = basename == "tsconfig.json"
        || basename == "jsconfig.json"
        || (basename.starts_with("tsconfig.") && basename.ends_with(".json"))
        || (basename.starts_with("jsconfig.") && basename.ends_with(".json"));
    let sanitized;
    let parse_text = if jsonc {
        match sanitize_jsonc(text) {
            Ok(value) => {
                sanitized = value;
                sanitized.as_str()
            }
            Err(error) => {
                diagnostic(file, output, 0, error.to_string());
                return Ok(());
            }
        }
    } else {
        text
    };
    let mut at = 0;
    let parsed = match json_node(parse_text, &mut at, 0, deadline) {
        Ok(node) => node,
        Err(error) => {
            deadline.check()?;
            diagnostic(
                file,
                output,
                at.min(text.len()),
                format!("Invalid JSON contract/config: {error}"),
            );
            return Ok(());
        }
    };
    if !parse_text[at..].trim().is_empty() {
        diagnostic(file, output, at, "Trailing JSON content");
        return Ok(());
    }
    let root = output.entities[0].clone();
    if parsed
        .members
        .iter()
        .any(|(key, _, _)| key == "openapi" || key == "swagger")
    {
        let Some((_, _, paths)) = parsed.members.iter().find(|(k, _, _)| k == "paths") else {
            diagnostic(file, output, 0, "OpenAPI paths object is missing");
            return Ok(());
        };
        if !paths.is_object {
            diagnostic(file, output, paths.start, "OpenAPI paths must be an object");
            return Ok(());
        }
        for (url, _, methods) in &paths.members {
            if !url.starts_with('/') {
                diagnostic(
                    file,
                    output,
                    methods.start,
                    "OpenAPI path must start with /",
                );
                continue;
            }
            if !methods.is_object {
                diagnostic(
                    file,
                    output,
                    methods.start,
                    "OpenAPI path item must be an object",
                );
                continue;
            }
            for (method, start, operation) in &methods.members {
                if ![
                    "get", "post", "put", "patch", "delete", "head", "options", "trace",
                ]
                .contains(&method.as_str())
                {
                    continue;
                }
                if !operation.is_object {
                    diagnostic(
                        file,
                        output,
                        operation.start,
                        "OpenAPI operation must be an object",
                    );
                    continue;
                }
                let method = method.to_ascii_uppercase();
                add(
                    file,
                    text,
                    output,
                    &root,
                    "openapi_endpoint",
                    &format!("{method} {url}"),
                    &format!("http:{method}:{url}"),
                    *start,
                    operation.end,
                    None,
                )?;
            }
        }
    }
    if parsed.members.iter().any(|(key, _, _)| key == "$schema")
        && let Some((_, _, properties)) = parsed.members.iter().find(|(k, _, _)| k == "properties")
    {
        if !properties.is_object {
            diagnostic(
                file,
                output,
                properties.start,
                "JSON Schema properties must be an object",
            );
        }
        for (name, start, value) in &properties.members {
            add(
                file,
                text,
                output,
                &root,
                "schema_field",
                name,
                &format!("json-schema:{name}"),
                *start,
                value.end,
                None,
            )?;
        }
    }
    Ok(())
}

fn sanitize_jsonc(text: &str) -> Result<String> {
    let mut bytes = text.as_bytes().to_vec();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'"' {
            at += 1;
            while at < bytes.len() {
                if bytes[at] == b'"' {
                    at += 1;
                    break;
                }
                if bytes[at] == b'\\' {
                    at += 1;
                }
                at += 1;
            }
        } else if bytes[at..].starts_with(b"//") {
            while at < bytes.len() && bytes[at] != b'\n' {
                bytes[at] = b' ';
                at += 1;
            }
        } else if bytes[at..].starts_with(b"/*") {
            bytes[at] = b' ';
            bytes[at + 1] = b' ';
            at += 2;
            let mut closed = false;
            while at < bytes.len() {
                if bytes[at..].starts_with(b"*/") {
                    bytes[at] = b' ';
                    bytes[at + 1] = b' ';
                    at += 2;
                    closed = true;
                    break;
                }
                if bytes[at] != b'\n' && bytes[at] != b'\r' {
                    bytes[at] = b' ';
                }
                at += 1;
            }
            ensure!(closed, "Unterminated JSONC comment");
        } else {
            at += 1;
        }
    }
    at = 0;
    let mut previous = None;
    while at < bytes.len() {
        if bytes[at] == b'"' {
            at += 1;
            while at < bytes.len() {
                if bytes[at] == b'"' {
                    at += 1;
                    break;
                }
                if bytes[at] == b'\\' {
                    at += 1;
                }
                at += 1;
            }
            previous = Some(b'"');
            continue;
        }
        if bytes[at] == b',' && previous.is_some_and(|p| !b"{[,:".contains(&p)) {
            let mut next = at + 1;
            while bytes.get(next).is_some_and(u8::is_ascii_whitespace) {
                next += 1;
            }
            if bytes.get(next).is_some_and(|b| b"]}".contains(b)) {
                bytes[at] = b' ';
            }
        }
        if !bytes[at].is_ascii_whitespace() {
            previous = Some(bytes[at]);
        }
        at += 1;
    }
    Ok(String::from_utf8(bytes)?)
}
pub fn extract_document(
    file: &SourceFile,
    text: &str,
    output: &mut Extraction,
    deadline: &Deadline,
) -> Result<()> {
    let extension = file
        .path
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    if extension == "json" {
        return json_document(file, text, output, deadline);
    }
    let format = match extension.as_str() {
        "prisma" => "prisma",
        "graphql" | "gql" => "graphql",
        "proto" => "protobuf",
        "sql" => "sql",
        _ => return Ok(()),
    };
    let tokens = match tokenize(text, format == "sql", deadline) {
        Ok(tokens) => tokens,
        Err(error) => {
            deadline.check()?;
            diagnostic(file, output, 0, error.to_string());
            return Ok(());
        }
    };
    if format == "sql" {
        sql(file, text, output, &tokens)
    } else {
        schema(file, text, output, &tokens, format)
    }
}
pub fn candidate_relation(relation: &str) -> bool {
    [
        "possibly_accesses_table",
        "possibly_implemented_by_migration",
        "possibly_consumes_contract",
        "possibly_implements_contract",
        "possible_event_delivery",
    ]
    .contains(&relation)
}
fn scope<'a>(path: &'a str, manifests: &[&'a str]) -> &'a str {
    let manifest = manifests
        .iter()
        .copied()
        .filter(|prefix| !prefix.is_empty() && path.starts_with(&format!("{prefix}/")))
        .max_by_key(|p| p.len());
    let mut parts = path.split('/');
    let first = parts.next().unwrap_or("");
    let service = if ["services", "apps", "packages"].contains(&first)
        && let Some(second) = parts.next()
    {
        &path[..first.len() + 1 + second.len()]
    } else {
        ""
    };
    manifest
        .filter(|m| m.len() >= service.len())
        .unwrap_or(service)
}
pub fn candidate_edges(
    entities: &[Entity],
    snapshot: &Snapshot,
    deadline: &Deadline,
) -> Result<Vec<RawEdge>> {
    let manifests: Vec<_> = snapshot
        .files
        .iter()
        .filter(|f| {
            [
                "package.json",
                "Cargo.toml",
                "pyproject.toml",
                "go.mod",
                "pom.xml",
            ]
            .contains(&f.path.rsplit('/').next().unwrap_or(""))
                || f.path.ends_with(".csproj")
        })
        .map(|f| f.path.rsplit_once('/').map_or("", |p| p.0))
        .collect();
    let mut targets: BTreeMap<(String, String, String), Vec<&Entity>> = BTreeMap::new();
    for target in entities {
        let family = match target.kind.as_str() {
            "orm_model" | "migration_table" => "table",
            "openapi_endpoint" => "http",
            "event_subscriber" => "event",
            _ => continue,
        };
        targets
            .entry((
                scope(&target.path, &manifests).into(),
                family.into(),
                target.qualified_name.clone(),
            ))
            .or_default()
            .push(target);
        if target.kind == "orm_model" {
            targets
                .entry((
                    scope(&target.path, &manifests).into(),
                    "table".into(),
                    format!("orm:{}", target.name.to_ascii_lowercase()),
                ))
                .or_default()
                .push(target);
        }
    }
    let mut out = Vec::new();
    for source in entities {
        deadline.check()?;
        let (family, relation) = match source.kind.as_str() {
            "sql_operation" | "orm_operation" => ("table", "possibly_accesses_table"),
            "orm_model" => ("table", "possibly_implemented_by_migration"),
            "http_client_call" => ("http", "possibly_consumes_contract"),
            "route" => ("http", "possibly_implements_contract"),
            "event_publisher" => ("event", "possible_event_delivery"),
            _ => continue,
        };
        let candidates = targets.get(&(
            scope(&source.path, &manifests).into(),
            family.into(),
            source.qualified_name.clone(),
        ));
        let candidates: Vec<_> = candidates
            .into_iter()
            .flatten()
            .filter(|t| {
                t.key != source.key && (source.kind != "orm_model" || t.kind == "migration_table")
            })
            .collect();
        let label = if candidates.len() == 1 {
            candidates[0].key.clone()
        } else {
            format!("candidate:{}", source.qualified_name)
        };
        out.push(edge(source, &label, relation));
    }
    Ok(out)
}
