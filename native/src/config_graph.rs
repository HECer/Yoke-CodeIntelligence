//! Configuration names and references, never environment secret values or authority.
use crate::{
    capture::Deadline,
    manifest_graph::{Builder, DerivedGraph},
    model::{Graph, Snapshot, SourceFile},
    store::Store,
};
use anyhow::{Result, ensure};
use serde_json::{Value, json};
static ENV_REFERENCE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
    regex::Regex::new(r"\$\{([A-Za-z_][A-Za-z_0-9]*)\}")
        .expect("constant environment-reference pattern")
});
fn key_component(key: &str) -> String {
    key.replace('~', "~0")
        .replace('.', "~1")
        .replace('[', "~2")
        .replace(']', "~3")
}
pub(crate) fn bounded_yaml(text: &str, deadline: &Deadline) -> Result<Vec<yaml_rust2::Yaml>> {
    use yaml_rust2::parser::{Event, Parser};
    #[derive(Clone, Copy)]
    struct Cost {
        nodes: usize,
        scalar_bytes: usize,
    }
    fn charge_bytes(total: &mut usize, bytes: usize) -> Result<()> {
        *total = total
            .checked_add(bytes)
            .ok_or_else(|| anyhow::anyhow!("YAML expanded scalar byte overflow"))?;
        ensure!(
            *total <= 8 * 1024 * 1024,
            "YAML expanded scalar byte capacity exceeded"
        );
        Ok(())
    }
    fn add(stack: &mut [(usize, Cost)], cost: Cost, total: &mut Cost) -> Result<()> {
        let sum = if let Some((_, sum)) = stack.last_mut() {
            sum
        } else {
            total
        };
        sum.nodes = sum
            .nodes
            .checked_add(cost.nodes)
            .ok_or_else(|| anyhow::anyhow!("YAML expansion overflow"))?;
        sum.scalar_bytes = sum
            .scalar_bytes
            .checked_add(cost.scalar_bytes)
            .ok_or_else(|| anyhow::anyhow!("YAML expanded scalar byte overflow"))?;
        ensure!(sum.nodes <= 20000, "YAML expanded node capacity exceeded");
        Ok(())
    }
    let mut parser = Parser::new_from_str(text);
    let mut stack = Vec::new();
    let mut anchors = std::collections::BTreeMap::new();
    let mut total = Cost {
        nodes: 0,
        scalar_bytes: 0,
    };
    // Count decoded UTF-8 bytes across the entire stream before the loader can
    // clone aliases. Anchors retain an additional copy, including containers.
    // Charging all scalar spellings is conservative for numeric/bool values.
    let mut scalar_bytes = 0;
    loop {
        deadline.check()?;
        let (event, _) = parser.next_token()?;
        match event {
            Event::SequenceStart(anchor, _) | Event::MappingStart(anchor, _) => {
                ensure!(stack.len() < 64, "YAML nesting capacity exceeded");
                stack.push((
                    anchor,
                    Cost {
                        nodes: 1,
                        scalar_bytes: 0,
                    },
                ));
            }
            Event::SequenceEnd | Event::MappingEnd => {
                let (anchor, cost) = stack
                    .pop()
                    .ok_or_else(|| anyhow::anyhow!("Unbalanced YAML collection"))?;
                if anchor != 0 {
                    charge_bytes(&mut scalar_bytes, cost.scalar_bytes)?;
                    anchors.insert(anchor, cost);
                }
                add(&mut stack, cost, &mut total)?;
            }
            Event::Scalar(value, _, anchor, _) => {
                let cost = Cost {
                    nodes: 1,
                    scalar_bytes: value.len(),
                };
                charge_bytes(&mut scalar_bytes, cost.scalar_bytes)?;
                if anchor != 0 {
                    charge_bytes(&mut scalar_bytes, cost.scalar_bytes)?;
                    anchors.insert(anchor, cost);
                }
                add(&mut stack, cost, &mut total)?;
            }
            Event::Alias(anchor) => {
                let cost = *anchors
                    .get(&anchor)
                    .ok_or_else(|| anyhow::anyhow!("Recursive or missing YAML anchor"))?;
                charge_bytes(&mut scalar_bytes, cost.scalar_bytes)?;
                add(&mut stack, cost, &mut total)?;
            }
            Event::DocumentStart => {
                anchors.clear();
            }
            Event::StreamEnd => break,
            _ => (),
        }
    }
    deadline.check()?;
    let docs = yaml_rust2::YamlLoader::load_from_str(text)?;
    ensure!(docs.len() <= 100, "YAML document capacity exceeded");
    deadline.check()?;
    Ok(docs)
}
fn yaml(value: &yaml_rust2::Yaml, depth: usize) -> Result<Value> {
    ensure!(depth <= 64, "YAML depth capacity exceeded");
    Ok(match value {
        yaml_rust2::Yaml::Hash(h) => {
            let mut out = serde_json::Map::new();
            for (k, v) in h {
                let key = k
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Complex YAML keys unsupported"))?;
                out.insert(key.into(), yaml(v, depth + 1)?);
            }
            Value::Object(out)
        }
        yaml_rust2::Yaml::Array(a) => Value::Array(
            a.iter()
                .map(|v| yaml(v, depth + 1))
                .collect::<Result<Vec<_>>>()?,
        ),
        yaml_rust2::Yaml::String(s) => json!(s),
        _ => Value::Null,
    })
}
#[allow(clippy::too_many_arguments)] // Carry both human key paths and structural JSON pointers through recursion.
fn walk(
    builder: &mut Builder<'_>,
    file: &SourceFile,
    text: &str,
    value: &Value,
    path: &str,
    pointer: &str,
    parent: Option<&str>,
    depth: usize,
) -> Result<()> {
    builder.deadline.check()?;
    ensure!(depth <= 64, "Configuration depth capacity exceeded");
    match value {
        Value::Object(map) => {
            for (key, value) in map {
                let full = if path.is_empty() {
                    key_component(key)
                } else {
                    format!("{path}.{}", key_component(key))
                };
                let env = path.ends_with(".env") || path == "env";
                let key_pointer =
                    format!("{pointer}/{}", key.replace('~', "~0").replace('/', "~1"));
                let id = builder.node(
                    file,
                    text,
                    if env {
                        "environment_name"
                    } else {
                        "configuration_key"
                    },
                    if env { key } else { &full },
                    "configuration",
                )?;
                if !env
                    && let Some(node) = builder
                        .output
                        .nodes
                        .last_mut()
                        .filter(|node| node["id"] == id)
                {
                    node["jsonPointer"] = json!(key_pointer);
                }
                let anchor = builder.declaration(file, text, key)?;
                if let Some(parent) = parent {
                    builder.edge(
                        parent,
                        Some(&id),
                        key,
                        "contains_key",
                        "declared_key",
                        &anchor,
                        json!({}),
                    )?;
                }
                if !env {
                    if let Some(reference) = value.as_str() {
                        if key == "$ref" || key == "ref" || key == "reference" {
                            builder.edge(
                                &id,
                                None,
                                reference,
                                "references_configuration",
                                "authored_reference",
                                &anchor,
                                json!({}),
                            )?;
                        }
                        for capture in ENV_REFERENCE.captures_iter(reference) {
                            builder.edge(
                                &id,
                                None,
                                &capture[1],
                                "references_environment",
                                "authored_reference",
                                &anchor,
                                json!({}),
                            )?;
                        }
                    }
                    walk(
                        builder,
                        file,
                        text,
                        value,
                        &full,
                        &key_pointer,
                        Some(&id),
                        depth + 1,
                    )?;
                }
            }
        }
        Value::Array(array) => {
            for (i, v) in array.iter().enumerate() {
                walk(
                    builder,
                    file,
                    text,
                    v,
                    &format!("{path}[{i}]"),
                    &format!("{pointer}/{i}"),
                    parent,
                    depth + 1,
                )?;
            }
        }
        _ => (),
    }
    Ok(())
}
fn mcp(builder: &mut Builder<'_>, file: &SourceFile, text: &str, value: &Value) -> Result<()> {
    if let Some(documents) = value.as_array() {
        for document in documents {
            mcp(builder, file, text, document)?;
        }
        return Ok(());
    }
    for section in ["mcpServers", "servers"] {
        if let Some(servers) = value[section].as_object() {
            for (name, config) in servers {
                let server = builder.node(file, text, "mcp_server", name, "mcp")?;
                let anchor = builder.declaration(file, text, name)?;
                if let Some(command) = config["command"].as_str()
                    && command.len() <= 200
                    && command
                        .chars()
                        .all(|c| c.is_alphanumeric() || "._-/\\:".contains(c))
                    && !command.contains("://")
                {
                    builder.edge(
                        &server,
                        None,
                        command,
                        "declares_command",
                        "configuration_only_not_authority",
                        &anchor,
                        json!({}),
                    )?;
                }
                if let Some(env) = config["env"].as_object() {
                    for name in env.keys() {
                        let node = builder.node(file, text, "environment_name", name, "mcp")?;
                        builder.edge(
                            &server,
                            Some(&node),
                            name,
                            "uses_environment_name",
                            "configuration_only_not_authority",
                            &anchor,
                            json!({}),
                        )?;
                    }
                }
                let launcher = config["command"].as_str().unwrap_or("");
                if ["npx", "uvx", "bunx"].contains(&launcher)
                    && let Some(args) = config["args"].as_array()
                    && let Some(package) = launcher_package(args)
                {
                    let package = if launcher == "uvx" {
                        package.split(['=', '<', '>', '[']).next().unwrap_or("")
                    } else {
                        package
                            .rsplit_once('@')
                            .filter(|(head, _)| !head.is_empty())
                            .map_or(package, |(head, _)| head)
                    };
                    if !package.is_empty()
                        && package.len() <= 200
                        && package
                            .chars()
                            .all(|c| c.is_ascii_alphanumeric() || "@/._-".contains(c))
                    {
                        builder.edge(
                            &server,
                            None,
                            package,
                            "declares_package",
                            "configuration_only_not_authority",
                            &anchor,
                            json!({"ecosystem":if launcher=="uvx"{"pypi"}else{"npm"}}),
                        )?;
                    }
                }
            }
        }
    }
    Ok(())
}
fn launcher_package(args: &[Value]) -> Option<&str> {
    let mut at = 0;
    while at < args.len() {
        let arg = args[at].as_str()?;
        if arg == "--" {
            return args.get(at + 1)?.as_str();
        }
        if [
            "-y",
            "--yes",
            "--no",
            "--quiet",
            "-q",
            "--offline",
            "--no-cache",
        ]
        .contains(&arg)
        {
            at += 1;
            continue;
        }
        if [
            "--userconfig",
            "--registry",
            "--cache",
            "--prefix",
            "--package",
            "-p",
            "--index-url",
            "--extra-index-url",
            "--python",
            "--from",
            "--directory",
        ]
        .contains(&arg)
        {
            args.get(at + 1)?.as_str()?;
            at += 2;
            continue;
        }
        if arg.starts_with('-') {
            if arg.contains('=') {
                at += 1;
                continue;
            }
            return None;
        }
        return Some(arg);
    }
    None
}
fn descriptors(
    builder: &mut Builder<'_>,
    file: &SourceFile,
    text: &str,
    extension: &str,
) -> Result<()> {
    let root = builder.node(
        file,
        text,
        "configuration_document",
        &file.path,
        "descriptor",
    )?;
    let component = regex::Regex::new(
        r"^\s*(?:object|inherited|inline)\s+([A-Za-z_][A-Za-z_0-9]*)\s*:\s*([A-Za-z_][A-Za-z_0-9.]*)",
    )?;
    let solution = regex::Regex::new(r#"^\s*Project\([^)]*\)\s*=\s*"[^"]*",\s*"([^"]+)""#)?;
    let asset = regex::Regex::new(r"'([^'\r\n]+\.(?:dmi|dmm|dmf|dm|png|wav|ogg|mp3))'")?;
    let mut offset = 0;
    let mut owner = root.clone();
    for line in text.split_inclusive('\n') {
        builder.deadline.check()?;
        let anchor = builder.anchor(file, text, offset, offset + line.len(), "descriptor_line")?;
        if extension == "sln" {
            if let Some(c) = solution.captures(line) {
                let path = c[1].replace('\\', "/");
                builder.edge(
                    &root,
                    None,
                    &path,
                    "references_project",
                    "authored_path",
                    &anchor,
                    json!({}),
                )?;
            }
        } else if ["dfm", "lfm"].contains(&extension) {
            if let Some(c) = component.captures(line) {
                owner = builder.node(file, text, "form_component", &c[1], "pascal_form")?;
                builder.edge(
                    &root,
                    Some(&owner),
                    &c[1],
                    "contains_component",
                    "declared_component",
                    &anchor,
                    json!({"componentType":&c[2]}),
                )?;
            } else if let Some((key, value)) = line.split_once('=') {
                let key = key.trim();
                let value = value.trim();
                if key.starts_with("On") && value.chars().all(|c| c.is_alphanumeric() || c == '_') {
                    builder.edge(
                        &owner,
                        None,
                        value,
                        "references_handler",
                        "name_candidate",
                        &anchor,
                        json!({"event":key}),
                    )?;
                }
            }
        } else {
            for c in asset.captures_iter(line) {
                builder.edge(
                    &root,
                    None,
                    &c[1],
                    "references_asset",
                    "authored_path",
                    &anchor,
                    json!({}),
                )?;
            }
        }
        offset += line.len();
    }
    Ok(())
}
pub fn extract(
    store: &Store,
    snapshot: &Snapshot,
    graph: &Graph,
    deadline: &Deadline,
) -> Result<DerivedGraph> {
    prepare(store, snapshot, graph, deadline)?.publish(deadline)
}
/// Derive exact immutable output bytes without writing CAS or database records.
pub fn prepare<'a>(
    store: &'a Store,
    snapshot: &'a Snapshot,
    graph: &Graph,
    deadline: &'a Deadline,
) -> Result<crate::manifest_graph::PreparedDerivedGraph<'a>> {
    let mut builder = Builder::new(store, snapshot, graph, "yoke-config-graph-1", deadline)?;
    for file in &snapshot.files {
        deadline.check()?;
        let extension = file.path.rsplit('.').next().unwrap_or("");
        if ![
            "json", "yaml", "yml", "toml", "xml", "csproj", "fsproj", "vbproj", "slnx", "xaml",
            "sln", "dfm", "lfm", "dmm", "dmf", "dme",
        ]
        .contains(&extension)
        {
            continue;
        }
        let (text, _) = crate::query::source(store, snapshot, &file.path)?;
        if ["sln", "dfm", "lfm", "dmm", "dmf", "dme"].contains(&extension) {
            descriptors(&mut builder, file, &text, extension)?;
            continue;
        }
        if ["xml", "csproj", "fsproj", "vbproj", "slnx", "xaml"].contains(&extension) {
            use quick_xml::events::Event;
            let mut reader = quick_xml::Reader::from_str(&text);
            let mut stack: Vec<String> = Vec::new();
            let mut key_paths: Vec<String> = Vec::new();
            loop {
                deadline.check()?;
                let event = reader.read_event()?;
                let empty = matches!(&event, Event::Empty(_));
                match event {
                    Event::Start(e) | Event::Empty(e) => {
                        let name = e.local_name().as_ref().to_owned();
                        let key_path = key_paths
                            .last()
                            .map_or_else(|| name.clone(), |parent| format!("{parent}.{name}"));
                        let node =
                            builder.node(file, &text, "configuration_key", &key_path, "xml")?;
                        let anchor = builder.declaration(file, &text, &name)?;
                        if let Some(parent) = stack.last() {
                            builder.edge(
                                parent,
                                Some(&node),
                                &name,
                                "contains_key",
                                "declared_key",
                                &anchor,
                                json!({}),
                            )?;
                        }
                        for attr in e.attributes() {
                            let attr = attr?;
                            let key = attr.key.local_name().as_ref().to_owned();
                            if key == "ref" {
                                builder.edge(
                                    &node,
                                    None,
                                    attr.value.as_ref(),
                                    "references_configuration",
                                    "authored_reference",
                                    &anchor,
                                    json!({}),
                                )?;
                            }
                            if key == "Include" || key == "Source" {
                                let relation = match name.as_str() {
                                    "ProjectReference" => Some("references_project"),
                                    "Compile" => Some("includes_source"),
                                    "PackageReference" => Some("references_package"),
                                    "ResourceDictionary" => Some("references_asset"),
                                    _ => None,
                                };
                                if let Some(relation) = relation {
                                    builder.edge(
                                        &node,
                                        None,
                                        attr.value.as_ref(),
                                        relation,
                                        "authored_declaration",
                                        &anchor,
                                        json!({}),
                                    )?;
                                }
                            }
                        }
                        if !empty {
                            ensure!(stack.len() < 64, "XML depth capacity exceeded");
                            stack.push(node);
                            key_paths.push(key_path);
                        }
                    }
                    Event::End(_) => {
                        stack.pop();
                        key_paths.pop();
                    }
                    Event::DocType(_) => anyhow::bail!("XML DTD refused"),
                    Event::Eof => break,
                    _ => (),
                }
            }
            continue;
        }
        let value = match extension {
            "json" => crate::json::parse(text.as_bytes(), 4_000_000)?,
            "toml" => serde_json::to_value(toml::from_str::<toml::Value>(&text)?)?,
            _ => {
                ensure!(
                    text.lines()
                        .all(|line| line.len() - line.trim_start().len() <= 256),
                    "YAML indentation capacity exceeded"
                );
                let docs = bounded_yaml(&text, deadline)?;
                ensure!(docs.len() <= 100, "YAML document capacity exceeded");
                Value::Array(
                    docs.iter()
                        .map(|v| yaml(v, 0))
                        .collect::<Result<Vec<_>>>()?,
                )
            }
        };
        walk(&mut builder, file, &text, &value, "", "", None, 0)?;
        mcp(&mut builder, file, &text, &value)?;
    }
    let keys: std::collections::BTreeMap<_, _> = builder
        .output
        .nodes
        .iter()
        .filter(|n| n["kind"] == "configuration_key" && n["jsonPointer"].is_string())
        .map(|n| {
            (
                (
                    n["path"].as_str().unwrap().to_owned(),
                    n["jsonPointer"].as_str().unwrap().to_owned(),
                ),
                n["id"].clone(),
            )
        })
        .collect();
    let paths: std::collections::BTreeMap<_, _> = builder
        .output
        .nodes
        .iter()
        .map(|n| {
            (
                n["id"].as_str().unwrap().to_owned(),
                n["path"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    for edge in &mut builder.output.observations {
        deadline.check()?;
        if edge["relation"] == "references_configuration"
            && let Some(pointer) = edge["targetLabel"]
                .as_str()
                .and_then(|s| s.strip_prefix('#'))
        {
            let key = pointer.to_owned();
            if let Some(id) = keys.get(&(paths[edge["source"].as_str().unwrap()].clone(), key)) {
                edge["target"] = id.clone();
                edge["resolution"] = json!("authored_local_pointer");
            }
        }
    }
    builder.finish()
}
