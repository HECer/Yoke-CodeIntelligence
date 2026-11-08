//! Independent manifest graphs; repository declarations never authorize execution.
use crate::{
    capture::Deadline,
    model::{Graph, Snapshot, SourceFile, hash, identity},
    store::Store,
};
use anyhow::{Result, ensure};
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
pub const RECORD_KIND: &str = "native_derived_source_graph_v1";
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DerivedGraph {
    pub snapshot_id: String,
    pub workspace_id: String,
    pub graph_hash: String,
    pub producer: String,
    pub nodes: Vec<Value>,
    pub observations: Vec<Value>,
    pub evidence: Vec<Value>,
    pub limitations: Vec<String>,
    pub artifact_id: String,
}
/// A prepared structural graph can only be obtained from checked source derivation.
/// Its exact bytes retain the released empty-`artifactId` serialization.
pub struct PreparedDerivedGraph<'a> {
    store: &'a Store,
    output: DerivedGraph,
    bytes: Vec<u8>,
}
impl PreparedDerivedGraph<'_> {
    pub fn artifact_id(&self) -> &str {
        &self.output.artifact_id
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn byte_len(&self) -> usize {
        self.bytes.len()
    }
    fn registration(&self) -> Value {
        json!({"snapshotId":self.output.snapshot_id,"workspaceId":self.output.workspace_id,"graphHash":self.output.graph_hash,"artifactId":self.output.artifact_id,"producer":self.output.producer})
    }
    pub(crate) fn preflight(
        &self,
        tx: &rusqlite::Transaction<'_>,
        deadline: &Deadline,
    ) -> Result<()> {
        deadline.check()?;
        ensure!(
            std::ptr::eq(&**tx, &self.store.db),
            "Prepared structural graph belongs to a different Store connection"
        );
        self.store.set_deadline(deadline)?;
        let stored: Option<Option<String>> = tx
            .query_row(
                "SELECT CASE WHEN length(CAST(value AS BLOB)) <= 64000 THEN value END FROM records WHERE kind=?1 AND id=?2",
                rusqlite::params![RECORD_KIND, self.output.artifact_id],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(stored) = stored {
            let stored =
                stored.ok_or_else(|| anyhow::anyhow!("Stored derived graph scope differs"))?;
            let stored = crate::json::parse(stored.as_bytes(), 64000)
                .map_err(|_| anyhow::anyhow!("Stored derived graph scope differs"))?;
            ensure!(
                stored == self.registration(),
                "Stored derived graph scope differs"
            );
        }
        deadline.check()
    }
    pub fn publish_into(
        self,
        tx: &rusqlite::Transaction<'_>,
        deadline: &Deadline,
    ) -> Result<DerivedGraph> {
        self.preflight(tx, deadline)?;
        // CAS persistence is outside SQLite rollback. All admission must precede
        // this call; an IO/commit failure can leave an unregistered physical blob.
        ensure!(
            self.store.artifact(&self.bytes)? == self.output.artifact_id,
            "Derived artifact mismatch"
        );
        let record = self.registration();
        tx.execute(
            "INSERT INTO records(kind,id,value) VALUES(?1,?2,?3) ON CONFLICT(kind,id) DO NOTHING",
            rusqlite::params![
                RECORD_KIND,
                self.output.artifact_id,
                serde_json::to_string(&record)?
            ],
        )?;
        ensure!(
            self.store
                .get_record::<Value>(RECORD_KIND, &self.output.artifact_id)?
                == record,
            "Stored derived graph scope differs"
        );
        deadline.check()?;
        Ok(self.output)
    }
    pub(crate) fn publish(self, deadline: &Deadline) -> Result<DerivedGraph> {
        deadline.check()?;
        let store = self.store;
        let tx = rusqlite::Transaction::new_unchecked(
            &store.db,
            rusqlite::TransactionBehavior::Immediate,
        )?;
        let output = self.publish_into(&tx, deadline)?;
        tx.commit()?;
        Ok(output)
    }
}

pub(crate) struct Builder<'a> {
    pub output: DerivedGraph,
    pub store: &'a Store,
    pub snapshot: &'a Snapshot,
    pub deadline: &'a Deadline,
    node_ids: BTreeSet<String>,
    anchor_ids: BTreeSet<String>,
}
impl<'a> Builder<'a> {
    pub fn new(
        store: &'a Store,
        snapshot: &'a Snapshot,
        graph: &Graph,
        producer: &str,
        deadline: &'a Deadline,
    ) -> Result<Self> {
        crate::graph_extensions::validate_scope(store, snapshot, graph, deadline)?;
        Ok(Self{store,snapshot,deadline,node_ids:BTreeSet::new(),anchor_ids:BTreeSet::new(),output:DerivedGraph{snapshot_id:snapshot.id.clone(),workspace_id:snapshot.workspace_id.clone(),graph_hash:identity(graph),producer:producer.into(),nodes:vec![],observations:vec![],evidence:vec![],limitations:vec!["Declarations and name matches are source facts or candidates; they do not authorize execution, network access or establish compiler/runtime behavior.".into()],artifact_id:String::new()}})
    }
    pub fn anchor(
        &mut self,
        file: &SourceFile,
        text: &str,
        start: usize,
        end: usize,
        granularity: &str,
    ) -> Result<String> {
        self.deadline.check()?;
        crate::paths::utf8_range(text, start, end)?;
        let id = identity(&(
            &self.snapshot.id,
            &file.path,
            &file.hash,
            start,
            end,
            granularity,
        ));
        if self.anchor_ids.insert(id.clone()) {
            ensure!(
                self.output.evidence.len() < 100000,
                "Derived anchor capacity exceeded"
            );
            self.output.evidence.push(json!({"id":id,"snapshotId":self.snapshot.id,"path":file.path,"contentHash":file.hash,"startByte":start,"endByte":end,"startLine":text[..start].bytes().filter(|b|*b==b'\n').count()+1,"endLine":text[..end].bytes().filter(|b|*b==b'\n').count()+1,"anchor":granularity}));
        }
        Ok(id)
    }
    pub fn declaration(&mut self, file: &SourceFile, text: &str, label: &str) -> Result<String> {
        let hits: Vec<_> = text.match_indices(label).take(2).collect();
        if hits.len() == 1 && !label.is_empty() {
            self.anchor(
                file,
                text,
                hits[0].0,
                hits[0].0 + label.len(),
                "unique_literal_occurrence",
            )
        } else {
            self.anchor(file, text, 0, text.len(), "file")
        }
    }
    pub fn node(
        &mut self,
        file: &SourceFile,
        text: &str,
        kind: &str,
        name: &str,
        ecosystem: &str,
    ) -> Result<String> {
        self.deadline.check()?;
        ensure!(
            self.output.nodes.len() < 20000 && name.len() <= 4096,
            "Derived node capacity exceeded"
        );
        let id = identity(&(&self.snapshot.id, &file.path, kind, name, ecosystem));
        if self.node_ids.insert(id.clone()) {
            let anchor = self.declaration(file, text, name)?;
            self.output.nodes.push(json!({"id":id,"snapshotId":self.snapshot.id,"path":file.path,"kind":kind,"name":name,"ecosystem":ecosystem,"evidenceIds":[anchor]}));
        }
        Ok(id)
    }
    #[allow(clippy::too_many_arguments)] // Explicit source, target, relation and evidence avoid hidden mutable edge state.
    pub fn edge(
        &mut self,
        source: &str,
        target: Option<&str>,
        label: &str,
        relation: &str,
        resolution: &str,
        anchor: &str,
        metadata: Value,
    ) -> Result<()> {
        self.deadline.check()?;
        ensure!(
            self.output.observations.len() < 50000,
            "Derived observation capacity exceeded"
        );
        let ordinal = self.output.observations.len();
        let mut value = json!({"id":identity(&(&self.snapshot.id,&self.output.producer,ordinal,source,target,label,relation)),"source":source,"target":target,"targetLabel":label,"relation":relation,"resolution":resolution,"evidenceIds":[anchor]});
        if let Some(meta) = metadata.as_object() {
            for (k, v) in meta {
                ensure!(
                    !value.as_object().unwrap().contains_key(k),
                    "Reserved derived observation field"
                );
                value[k] = v.clone();
            }
        }
        self.output.observations.push(value);
        Ok(())
    }
    pub fn finish(mut self) -> Result<PreparedDerivedGraph<'a>> {
        self.deadline.check()?;
        let bytes = serde_json::to_vec(&self.output)?;
        ensure!(
            bytes.len() <= 32_000_000,
            "Derived graph byte capacity exceeded"
        );
        self.output.artifact_id = hash(&bytes);
        self.deadline.check()?;
        Ok(PreparedDerivedGraph {
            store: self.store,
            output: self.output,
            bytes,
        })
    }
}
#[derive(Clone)]
struct Dependency {
    alias: String,
    name: String,
    scope: String,
    path: Option<String>,
    inherited: bool,
}
struct Package {
    file: SourceFile,
    text: String,
    name: String,
    ecosystem: String,
    dependencies: Vec<Dependency>,
    declares_workspace: bool,
}
fn npm(file: &SourceFile, text: &str) -> Result<Package> {
    let doc: Value = crate::json::parse(text.as_bytes(), 4_000_000)?;
    let name = doc["name"].as_str().unwrap_or(&file.path).to_owned();
    let mut dependencies = vec![];
    for (section, scope) in [
        ("dependencies", "runtime"),
        ("devDependencies", "development"),
        ("peerDependencies", "peer"),
        ("optionalDependencies", "optional"),
    ] {
        if let Some(table) = doc[section].as_object() {
            for (alias, v) in table {
                let spec = v
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Dependency specification must be a string"))?;
                let actual = if let Some(rest) = spec.strip_prefix("npm:") {
                    rest.rsplit_once('@').map(|(a, _)| a).unwrap_or(rest)
                } else {
                    alias
                };
                dependencies.push(Dependency {
                    alias: alias.clone(),
                    name: actual.into(),
                    scope: scope.into(),
                    path: spec.strip_prefix("file:").map(str::to_owned),
                    inherited: false,
                });
            }
        }
    }
    Ok(Package {
        file: file.clone(),
        text: text.into(),
        name,
        ecosystem: "npm".into(),
        dependencies,
        declares_workspace: false,
    })
}
fn go(file: &SourceFile, text: &str) -> Result<Package> {
    let mut name = file.path.clone();
    let mut dependencies = vec![];
    let mut require = false;
    for line in text.lines() {
        let line = line.split("//").next().unwrap().trim();
        let words: Vec<_> = line.split_whitespace().collect();
        if words.first() == Some(&"module") && words.len() == 2 {
            name = words[1].trim_matches('"').into();
        } else if words.first() == Some(&"require") {
            if words.get(1) == Some(&"(") {
                require = true;
            } else if words.len() >= 3 {
                dependencies.push(Dependency {
                    alias: words[1].into(),
                    name: words[1].into(),
                    scope: "runtime".into(),
                    path: None,
                    inherited: false,
                });
            }
        } else if line == ")" {
            require = false;
        } else if require && words.len() >= 2 {
            dependencies.push(Dependency {
                alias: words[0].into(),
                name: words[0].into(),
                scope: "runtime".into(),
                path: None,
                inherited: false,
            });
        }
    }
    Ok(Package {
        file: file.clone(),
        text: text.into(),
        name,
        ecosystem: "go".into(),
        dependencies,
        declares_workspace: false,
    })
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
) -> Result<PreparedDerivedGraph<'a>> {
    let mut builder = Builder::new(store, snapshot, graph, "yoke-manifest-graph-1", deadline)?;
    let mut packages = vec![];
    for file in &snapshot.files {
        deadline.check()?;
        let name = file.path.rsplit('/').next().unwrap();
        if ![
            "package.json",
            "apm.json",
            "apm.yml",
            "apm.yaml",
            "Cargo.toml",
            "pyproject.toml",
            "go.mod",
            "pom.xml",
        ]
        .contains(&name)
        {
            continue;
        }
        let (text, _) = crate::query::source(store, snapshot, &file.path)?;
        let package = match name {
            "package.json" | "apm.json" => npm(file, &text)?,
            "apm.yml" | "apm.yaml" => apm(file, &text, deadline)?,
            "go.mod" => go(file, &text)?,
            "Cargo.toml" | "pyproject.toml" => toml_package(file, &text)?,
            "pom.xml" => maven(file, &text)?,
            _ => unreachable!(),
        };
        packages.push(package);
    }
    let mut ids = BTreeMap::new();
    for p in &packages {
        if !p.name.is_empty() {
            ids.insert(
                p.file.path.clone(),
                builder.node(&p.file, &p.text, "package", &p.name, &p.ecosystem)?,
            );
        }
    }
    for p in &packages {
        let Some(source) = ids.get(&p.file.path) else {
            continue;
        };
        for d in &p.dependencies {
            let mut dep = d.clone();
            let mut path_base = p.file.path.clone();
            if dep.inherited {
                let dir = p.file.path.rsplit_once('/').map(|(a, _)| a).unwrap_or("");
                if let Some(workspace) = packages
                    .iter()
                    .filter(|w| {
                        w.ecosystem == "cargo"
                            && w.declares_workspace
                            && (w.file.path == "Cargo.toml"
                                || dir.starts_with(w.file.path.trim_end_matches("Cargo.toml")))
                    })
                    .max_by_key(|w| w.file.path.len())
                    && let Some(inherited) = workspace
                        .dependencies
                        .iter()
                        .find(|x| x.alias == dep.alias && x.scope == "workspace")
                {
                    dep.name = inherited.name.clone();
                    dep.path = inherited.path.clone();
                    path_base = workspace.file.path.clone();
                }
            }
            let target_path = dep
                .path
                .as_ref()
                .and_then(|path| resolve_manifest_path(&path_base, path, &p.ecosystem));
            let mismatch = target_path
                .as_ref()
                .and_then(|path| packages.iter().find(|p| &p.file.path == path))
                .is_some_and(|target| target.name != dep.name);
            let target = if mismatch {
                None
            } else {
                target_path.as_ref().and_then(|path| ids.get(path))
            };
            let anchor = builder.declaration(&p.file, &p.text, &dep.alias)?;
            builder.edge(source,target.map(String::as_str),&dep.name,"depends_on",if mismatch{"manifest_package_mismatch"}else if target.is_some(){"manifest_path"}else{"declared_dependency"},&anchor,json!({"dependencyAlias":dep.alias,"scope":dep.scope,"ecosystem":p.ecosystem,"inherited":dep.inherited}))?;
        }
    }
    builder.finish()
}
fn resolve_manifest_path(manifest: &str, path: &str, ecosystem: &str) -> Option<String> {
    if path.starts_with('/') || path.contains(['\\', ':']) {
        return None;
    }
    let mut parts: Vec<_> = manifest
        .rsplit_once('/')
        .map(|(a, _)| a.split('/').collect())
        .unwrap_or_default();
    for p in path.split('/') {
        match p {
            "" | "." => (),
            ".." => {
                parts.pop()?;
            }
            _ => parts.push(p),
        }
    }
    parts.push(if ecosystem == "cargo" {
        "Cargo.toml"
    } else {
        "package.json"
    });
    Some(parts.join("/"))
}
fn apm(file: &SourceFile, text: &str, deadline: &Deadline) -> Result<Package> {
    let docs = crate::config_graph::bounded_yaml(text, deadline)?;
    ensure!(docs.len() == 1, "APM requires one YAML document");
    let doc = &docs[0];
    let name = doc["name"].as_str().unwrap_or(&file.path).to_owned();
    let mut dependencies = vec![];
    match &doc["dependencies"] {
        yaml_rust2::Yaml::Array(array) => {
            for item in array {
                let name = item
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("APM list dependency must be a name"))?;
                dependencies.push(Dependency {
                    alias: name.into(),
                    name: name.into(),
                    scope: "runtime".into(),
                    path: None,
                    inherited: false,
                });
            }
        }
        yaml_rust2::Yaml::Hash(map) => {
            for key in map.keys() {
                let name = key
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("APM dependency key must be a name"))?;
                dependencies.push(Dependency {
                    alias: name.into(),
                    name: name.into(),
                    scope: "runtime".into(),
                    path: None,
                    inherited: false,
                });
            }
        }
        _ => (),
    }
    Ok(Package {
        file: file.clone(),
        text: text.into(),
        name,
        ecosystem: "apm".into(),
        dependencies,
        declares_workspace: false,
    })
}
fn toml_package(file: &SourceFile, text: &str) -> Result<Package> {
    let doc: toml::Value = toml::from_str(text)?;
    let cargo = file.path.ends_with("Cargo.toml");
    let ecosystem = if cargo { "cargo" } else { "pypi" };
    let name = doc
        .get(if cargo { "package" } else { "project" })
        .and_then(|v| v.get("name"))
        .and_then(toml::Value::as_str)
        .or_else(|| {
            doc.get("tool")
                .and_then(|v| v.get("poetry"))
                .and_then(|v| v.get("name"))
                .and_then(toml::Value::as_str)
        })
        .unwrap_or("")
        .to_owned();
    let mut dependencies = vec![];
    if cargo {
        fn table(out: &mut Vec<Dependency>, value: Option<&toml::Value>, scope: &str) {
            if let Some(table) = value.and_then(toml::Value::as_table) {
                for (alias, spec) in table {
                    out.push(Dependency {
                        alias: alias.clone(),
                        name: spec
                            .get("package")
                            .and_then(toml::Value::as_str)
                            .unwrap_or(alias)
                            .into(),
                        scope: scope.into(),
                        path: spec
                            .get("path")
                            .and_then(toml::Value::as_str)
                            .map(str::to_owned),
                        inherited: spec.get("workspace").and_then(toml::Value::as_bool)
                            == Some(true),
                    });
                }
            }
        }
        for (section, scope) in [
            ("dependencies", "runtime"),
            ("dev-dependencies", "development"),
            ("build-dependencies", "build"),
        ] {
            table(&mut dependencies, doc.get(section), scope);
            if let Some(targets) = doc.get("target").and_then(toml::Value::as_table) {
                for target in targets.values() {
                    table(
                        &mut dependencies,
                        target.get(section),
                        &format!("target_{scope}"),
                    );
                }
            }
        }
        table(
            &mut dependencies,
            doc.get("workspace").and_then(|v| v.get("dependencies")),
            "workspace",
        );
    } else {
        fn pep(out: &mut Vec<Dependency>, values: Option<&toml::Value>, scope: &str) -> Result<()> {
            if let Some(array) = values.and_then(toml::Value::as_array) {
                for v in array {
                    let raw = v
                        .as_str()
                        .ok_or_else(|| anyhow::anyhow!("PEP508 dependency must be text"))?;
                    let n = raw
                        .split(['[', '<', '>', '=', '!', '~', ';', ' ', '@'])
                        .next()
                        .unwrap();
                    ensure!(!n.is_empty(), "Invalid PEP508 dependency");
                    out.push(Dependency {
                        alias: n.into(),
                        name: n.to_ascii_lowercase().replace('_', "-"),
                        scope: scope.into(),
                        path: None,
                        inherited: false,
                    });
                }
            }
            Ok(())
        }
        pep(
            &mut dependencies,
            doc.get("project").and_then(|v| v.get("dependencies")),
            "runtime",
        )?;
        if let Some(groups) = doc
            .get("project")
            .and_then(|v| v.get("optional-dependencies"))
            .and_then(toml::Value::as_table)
        {
            for (group, values) in groups {
                pep(
                    &mut dependencies,
                    Some(values),
                    &format!("optional:{group}"),
                )?;
            }
        }
        if let Some(groups) = doc.get("dependency-groups").and_then(toml::Value::as_table) {
            for (group, values) in groups {
                pep(&mut dependencies, Some(values), &format!("group:{group}"))?;
            }
        }
        if let Some(table) = doc
            .get("tool")
            .and_then(|v| v.get("poetry"))
            .and_then(|v| v.get("dependencies"))
            .and_then(toml::Value::as_table)
        {
            for (alias, spec) in table {
                if alias == "python" {
                    continue;
                }
                dependencies.push(Dependency {
                    alias: alias.clone(),
                    name: alias.to_ascii_lowercase().replace('_', "-"),
                    scope: "runtime".into(),
                    path: spec
                        .get("path")
                        .and_then(toml::Value::as_str)
                        .map(str::to_owned),
                    inherited: false,
                });
            }
        }
    }
    Ok(Package {
        file: file.clone(),
        text: text.into(),
        name,
        ecosystem: ecosystem.into(),
        dependencies,
        declares_workspace: cargo
            && doc
                .get("workspace")
                .and_then(toml::Value::as_table)
                .is_some(),
    })
}
fn maven(file: &SourceFile, text: &str) -> Result<Package> {
    use quick_xml::events::Event;
    let mut reader = quick_xml::Reader::from_str(text);
    let mut stack = Vec::<String>::new();
    let mut root = BTreeMap::new();
    let mut current = BTreeMap::new();
    let mut dependencies = vec![];
    let mut dep_depth = None;
    loop {
        match reader.read_event()? {
            Event::Start(e) => {
                ensure!(stack.len() < 128, "XML nesting capacity exceeded");
                let tag = e.local_name().as_ref().to_owned();
                stack.push(tag.clone());
                if tag == "dependency" {
                    ensure!(dep_depth.is_none(), "Nested dependency declaration");
                    dep_depth = Some(stack.len());
                    current.clear();
                }
            }
            Event::End(_) => {
                if dep_depth == Some(stack.len()) {
                    if let (Some(group), Some(artifact)) =
                        (current.get("groupId"), current.get("artifactId"))
                    {
                        let name = format!("{group}:{artifact}");
                        dependencies.push(Dependency {
                            alias: name.clone(),
                            name,
                            scope: current
                                .get("scope")
                                .cloned()
                                .unwrap_or_else(|| "compile".into()),
                            path: None,
                            inherited: false,
                        });
                    }
                    dep_depth = None;
                }
                stack.pop();
            }
            Event::Text(e) => {
                let value = e
                    .xml_content(quick_xml::XmlVersion::Implicit1_0)
                    .into_owned();
                if let Some(tag) = stack.last() {
                    if dep_depth.is_some() {
                        current.insert(tag.clone(), value);
                    } else if stack.len() == 2 {
                        root.insert(tag.clone(), value);
                    }
                }
            }
            Event::DocType(_) => anyhow::bail!("XML DTD and entity declarations refused"),
            Event::Eof => break,
            _ => (),
        }
    }
    let name = format!(
        "{}:{}",
        root.get("groupId").map(String::as_str).unwrap_or(""),
        root.get("artifactId")
            .map(String::as_str)
            .unwrap_or(&file.path)
    );
    Ok(Package {
        file: file.clone(),
        text: text.into(),
        name,
        ecosystem: "maven".into(),
        dependencies,
        declares_workspace: false,
    })
}
