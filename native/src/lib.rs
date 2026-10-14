pub mod analysis;
pub mod capture;
pub mod changes;
pub mod container;
pub mod contracts;
pub mod credentials;
pub mod dataflow;
pub mod editor;
pub mod editor_http;
pub mod export;
pub mod federation;
pub mod federation_http;
pub mod host_credentials;
pub mod json;
pub mod knowledge;
pub mod ledger;
pub mod lsp;
pub mod maintenance;
pub mod media;
pub mod model;
pub mod ollama;
pub mod paths;
pub mod platform_credentials;
pub mod process;
pub mod projects;
pub mod protocol;
pub mod providers;
pub mod query;
pub mod retention;
pub mod runtime;
pub mod store;
pub mod test_map;
mod validation;
pub mod verify;

use anyhow::{Result, bail, ensure};
use capture::Deadline;
use model::{Graph, Snapshot, identity};
use rayon::prelude::*;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::{
    path::{Path, PathBuf},
    time::Instant,
};
use store::Store;
pub use validation::validate;

pub struct Engine {
    pub root: PathBuf,
    pub store: Store,
    pub allow_writes: bool,
    pub servers: BTreeMap<String, lsp::Server>,
}
impl Engine {
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        Self::open_with_timeout(root, std::time::Duration::from_secs(10))
    }
    pub fn open_with_timeout(root: impl AsRef<Path>, timeout: std::time::Duration) -> Result<Self> {
        let root = root.as_ref().canonicalize()?;
        ensure!(root.is_dir(), "Workspace root must be a directory");
        let store = Store::open_with_timeout(&root, timeout)?;
        Ok(Self {
            root,
            store,
            allow_writes: false,
            servers: BTreeMap::new(),
        })
    }
    pub fn index(&mut self, deadline: &Deadline) -> Result<(Snapshot, Graph, usize, usize)> {
        self.store.set_deadline(deadline)?;
        let snapshot = capture::capture(&self.root, &self.store, deadline)?;
        self.analyze_snapshot(snapshot, deadline)
    }
    pub fn analyze_snapshot(
        &mut self,
        snapshot: Snapshot,
        deadline: &Deadline,
    ) -> Result<(Snapshot, Graph, usize, usize)> {
        self.store.set_deadline(deadline)?;
        ensure!(
            snapshot.workspace_id == self.store.workspace_id,
            "Snapshot workspace scope mismatch"
        );
        if let Ok(graph) = self.store.graph(&snapshot.id) {
            self.store.set_latest(&snapshot.id)?;
            deadline.check()?;
            return Ok((snapshot.clone(), graph, 0, snapshot.files.len()));
        }
        let mut ready = Vec::new();
        let mut pending = Vec::new();
        for file in &snapshot.files {
            deadline.check()?;
            let key = identity(&(model::ANALYZER, &file.language, &file.hash, &file.path));
            if let Some(extraction) = self.store.cache(&key)? {
                ready.push(extraction);
            } else {
                pending.push((
                    key,
                    file.clone(),
                    String::from_utf8(self.store.read_blob(&file.hash)?)?,
                ));
            }
        }
        let reused = ready.len();
        let parsed = pending.len();
        let results: Vec<_> = pending
            .par_iter()
            .map(|(key, file, text)| {
                analysis::extract(file, text, deadline).map(|extraction| (key.clone(), extraction))
            })
            .collect();
        let results = results.into_iter().collect::<Result<Vec<_>>>()?;
        self.store.put_caches(&results, deadline)?;
        for (_, extraction) in results {
            ready.push(extraction);
        }
        ready.sort_by(|a, b| a.entities[0].path.cmp(&b.entities[0].path));
        let graph = analysis::assemble(&snapshot, ready, deadline)?;
        self.store.publish(&snapshot, &graph)?;
        deadline.check()?;
        Ok((snapshot, graph, parsed, reused))
    }
    pub fn select(&mut self, args: &Value, deadline: &Deadline) -> Result<(Snapshot, Graph)> {
        if let Some(id) = args["snapshotId"].as_str() {
            return Ok((self.store.snapshot(id)?, self.store.graph(id)?));
        }
        let (snapshot, graph, _, _) = self.index(deadline)?;
        Ok((snapshot, graph))
    }
    pub fn invoke(&mut self, tool: &str, args: Value) -> Result<Value> {
        let started = Instant::now();
        let deadline = Deadline::new(
            args["timeoutMs"]
                .as_u64()
                .filter(|n| (10..=600000).contains(n))
                .unwrap_or(120000),
        );
        let byte_max = args["maxBytes"]
            .as_u64()
            .filter(|n| (1024..=4_000_000).contains(n))
            .unwrap_or(32000) as usize;
        let max = args["tokenBudget"]
            .as_u64()
            .filter(|n| (1..=1000000).contains(n))
            .map_or(byte_max, |n| byte_max.min((n * 3) as usize));
        ensure!(
            max >= 512,
            "Requested budget cannot fit a response envelope"
        );
        ensure!(
            tool != "ci_change_apply" || args["mode"] != "workspace" || max >= 1024,
            "Workspace apply needs at least 1024 response bytes to preserve recovery information"
        );
        let mut sid = args["snapshotId"].as_str().map(str::to_string);
        let mut local_only = true;
        let mut language_server_attempts = 0;
        let result = (|| -> Result<Value> {
            self.store.set_deadline(&deadline)?;
            validate(tool, &args)?;
            if tool == "ci_read" && args.get("artifactId").is_some() {
                // A CAS ID can name generic artifacts or bytes used by many
                // snapshots. Caller-provided source scope is not evidence of
                // this object's membership in that snapshot.
                sid = None;
                let id = args["artifactId"].as_str().unwrap();
                let bytes = self.store.read_artifact(id)?;
                if args["encoding"] == "base64" {
                    use base64::Engine as _;
                    let start = args["cursor"].as_str().unwrap_or("0").parse::<usize>()?;
                    ensure!(start <= bytes.len(), "Invalid binary artifact cursor");
                    let end = start.saturating_add(max / 5).min(bytes.len());
                    return Ok(
                        json!({"chunk":base64::prelude::BASE64_STANDARD.encode(&bytes[start..end]),"encoding":"base64","cursor":start.to_string(),"nextCursor":if end<bytes.len(){Some(end.to_string())}else{None},"totalBytes":bytes.len(),"artifactId":id}),
                    );
                }
                let text = String::from_utf8(bytes)?;
                let start = args["cursor"].as_str().unwrap_or("0").parse::<usize>()?;
                ensure!(
                    start <= text.len() && text.is_char_boundary(start),
                    "Invalid artifact cursor"
                );
                let mut end = (start + max / 5).min(text.len());
                while !text.is_char_boundary(end) {
                    end -= 1;
                }
                return Ok(
                    json!({"chunk":&text[start..end],"cursor":start.to_string(),"nextCursor":if end<text.len(){Some(end.to_string())}else{None},"totalBytes":text.len(),"artifactId":id}),
                );
            }
            if tool == "ci_status" && args["mode"].as_str().unwrap_or("status") == "status" {
                sid = self.store.latest()?;
                return Ok(
                    json!({"version":env!("CARGO_PKG_VERSION"),"runtime":"rust","workspaceId":self.store.workspace_id,"latestSnapshotId":sid,"capabilities":{"structuralLanguages":analysis::LANGUAGES,"sourceSearch":true,"regexSearch":true,"directedGraph":true,"immutableSnapshots":true,"semanticRename":"host-selected-lsp-required","registeredSemanticLanguages":self.servers.keys().collect::<Vec<_>>(),"structuralDataflow":true,"hostWorkflows":["configured_compiler","test_map_collection","model_providers_and_ledger","python_debugger_repl","docker_verification","signed_tls_source_exchange","reviewed_history_retention"],"modelCalls":false,"upstreamApplicationsRequired":false,"workspaceWrites":self.allow_writes},"limitations":["Structural candidates and model inferences are not exhaustive compiler or runtime proofs.","Optional execution, models, credentials, network service and deletion require separate explicit host authority.","All-language-server certification and proprietary hosted features are outside the measured local workflow replacement."]}),
                );
            }
            if tool == "ci_change_apply" {
                let receipt =
                    changes::apply(&self.store, &self.root, &args, self.allow_writes, &deadline)?;
                sid = Some(receipt.snapshot_id.clone());
                return Ok(json!({"receipt":receipt}));
            }
            if tool == "ci_verify" {
                ensure!(
                    args.get("profiles").is_none(),
                    "Executed profiles are host-only CLI operations"
                );
                let id = args["planId"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("planId required"))?;
                let plan: changes::Plan = self.store.get_record("plan", id)?;
                sid = Some(plan.result_snapshot_id.clone());
                return Ok(json!({"verification":changes::verify_syntax(&self.store,&plan)?}));
            }
            if tool == "ci_status" && args["mode"] == "index" {
                let (snapshot, graph, parsed, reused) = self.index(&deadline)?;
                sid = Some(snapshot.id.clone());
                return Ok(
                    json!({"snapshot":snapshot,"coverage":graph.coverage,"diagnostics":graph.diagnostics,"entities":graph.entities.len(),"observations":graph.observations.len(),"parsedFiles":parsed,"reusedFiles":reused}),
                );
            }
            let (snapshot, graph) = self.select(&args, &deadline)?;
            sid = Some(snapshot.id.clone());
            match tool {
                "ci_status" if args["mode"] == "dataflow" => dataflow::analyze(
                    &self.store,
                    &snapshot,
                    &graph,
                    dataflow::Policy::default(),
                    &deadline,
                ),
                "ci_status" => Ok(
                    json!({"analytics":query::analytics(&graph,&deadline)?,"coverage":graph.coverage}),
                ),
                "ci_find" if args.get("languageServer").is_some() => {
                    let request = &args["languageServer"];
                    let path = request["path"].as_str().unwrap();
                    let operation = request["operation"].as_str().unwrap();
                    let offset = request["offset"].as_u64().unwrap() as usize;
                    let file = snapshot
                        .files
                        .iter()
                        .find(|file| file.path == path)
                        .ok_or_else(|| {
                            anyhow::anyhow!(
                                "Language server source path is absent from this snapshot"
                            )
                        })?;
                    ensure!(
                        file.language == capture::language(path),
                        "Captured source language mismatch"
                    );
                    let server = self.semantic_server(&file.language).ok_or_else(|| {
                        anyhow::anyhow!(
                            "No host-authorized semantic server registered for this language"
                        )
                    })?;
                    ensure!(server.trust, "Language server trust is required");
                    let (text, hash) = query::source(&self.store, &snapshot, path)?;
                    lsp::byte_to_position(&text, offset)?;
                    local_only = false;
                    language_server_attempts += 1;
                    let mut response = lsp::run_at(
                        self,
                        &server,
                        &snapshot,
                        &graph,
                        lsp::SemanticRequest {
                            path,
                            offset,
                            operation,
                            new_name: None,
                        },
                        &deadline,
                    )?;
                    response["operation"] = json!(operation);
                    response["path"] = json!(path);
                    response["offset"] = json!(offset);
                    response["contentHash"] = json!(hash);
                    response["semantic"] = json!("language-server-result");
                    Ok(response)
                }
                "ci_find" if args["kind"] == "semantic" => {
                    let model = args["model"].as_str().ok_or_else(|| {
                        anyhow::anyhow!("An explicitly indexed vector model is required")
                    })?;
                    let limit = args["limit"].as_u64().unwrap_or(20) as usize;
                    if let Some(vector) = args["vector"].as_array() {
                        let vector = vector
                            .iter()
                            .map(|number| {
                                number.as_f64().ok_or_else(|| {
                                    anyhow::anyhow!("Query vector must contain finite numbers")
                                })
                            })
                            .collect::<Result<Vec<_>>>()?;
                        knowledge::search_vectors(
                            &self.store,
                            &snapshot,
                            &graph,
                            model,
                            &vector,
                            limit,
                            &deadline,
                        )
                    } else {
                        knowledge::search_local_vectors(
                            &self.store,
                            &snapshot,
                            &graph,
                            model,
                            args["query"].as_str().unwrap_or(""),
                            limit,
                            &deadline,
                        )
                    }
                }
                "ci_find" => query::find(&self.store, &snapshot, &graph, &args, &deadline),
                "ci_read" => query::read(&self.store, &snapshot, &graph, &args),
                "ci_trace" => query::trace(&graph, &args, &deadline),
                "ci_context" => {
                    let mut data =
                        query::context(&self.store, &snapshot, &graph, &args, &deadline)?;
                    if args["includeMemory"] == true {
                        data["memory"] = knowledge::recall(
                            &self.store,
                            &snapshot,
                            args["query"].as_str().unwrap_or(""),
                            10,
                            &deadline,
                        )?;
                    }
                    Ok(data)
                }
                "ci_change_prepare" => {
                    if let Some(rename) = args.get("rename") {
                        ensure!(
                            args.get("edits").is_none()
                                && [
                                    "replaceSymbol",
                                    "insertBefore",
                                    "insertAfter",
                                    "deleteSymbol"
                                ]
                                .iter()
                                .all(|name| args.get(name).is_none()),
                            "Semantic rename cannot be mixed with other operations"
                        );
                        let entity = graph
                            .entities
                            .iter()
                            .find(|entity| entity.id == rename["entityId"])
                            .ok_or_else(|| {
                                anyhow::anyhow!("Rename entity is not in this snapshot")
                            })?;
                        let offset = entity.name_start_byte.ok_or_else(|| {
                            anyhow::anyhow!("Entity has no exact symbol-name anchor")
                        })?;
                        let server = self.semantic_server(&entity.language).ok_or_else(|| {
                            anyhow::anyhow!(
                                "No host-authorized semantic server registered for this language"
                            )
                        })?;
                        ensure!(server.trust, "Language server trust is required");
                        let name = rename["newName"]
                            .as_str()
                            .ok_or_else(|| anyhow::anyhow!("Rename name required"))?;
                        local_only = false;
                        language_server_attempts += 1;
                        return lsp::run_at(
                            self,
                            &server,
                            &snapshot,
                            &graph,
                            lsp::SemanticRequest {
                                path: &entity.path,
                                offset,
                                operation: "rename",
                                new_name: Some(name),
                            },
                            &deadline,
                        );
                    }
                    let plan =
                        changes::prepare(&mut self.store, &snapshot, &graph, &args, &deadline)?;
                    Ok(json!({"plan":plan}))
                }
                _ => bail!("Unknown tool"),
            }
        })();
        let mut envelope = json!({"schemaVersion":1,"requestId":identity(&(tool,started.elapsed().as_nanos(),changes::now())),"snapshotId":sid,"status":"ok","data":{},"warnings":[],"metrics":{"durationMs":started.elapsed().as_millis(),"serializedBytes":0,"estimatedTokens":0,"tokenCount":"estimated","tokenMethod":"utf8_bytes_div_3","modelCalls":0,"localOnly":local_only,"languageServerAttempts":language_server_attempts,"externalUsage":if language_server_attempts>0{"unknown"}else{"none"}}});
        match result {
            Ok(data) => envelope["data"] = data,
            Err(error) => {
                envelope["status"] = json!("error");
                envelope["error"] = if let Some(recovery) =
                    error.downcast_ref::<changes::RecoveryRequired>()
                {
                    json!({"code":"RECOVERY_REQUIRED","message":bounded_error(&error.to_string()),"journalId":recovery.journal_id,"emergencyPersisted":recovery.emergency_persisted})
                } else {
                    json!({"code":"REQUEST_FAILED","message":bounded_error(&error.to_string())})
                };
            }
        }
        update_metrics(&mut envelope)?;
        if serde_json::to_vec(&envelope)?.len() > max {
            let artifact = self.store.artifact(&serde_json::to_vec(&envelope)?)?;
            envelope["status"] = json!("partial");
            envelope["data"] = json!({});
            envelope["artifactId"] = json!(artifact);
            envelope["warnings"] = json!([
                "Complete response stored as an immutable artifact; use ci_read artifactId with its cursor."
            ]);
            update_metrics(&mut envelope)?;
        }
        ensure!(
            serde_json::to_vec(&envelope)?.len() <= max,
            "Response envelope exceeds hard budget"
        );
        self.store.clear_deadline();
        Ok(envelope)
    }
    pub fn approve(&self, id: &str, allow_tests: bool) -> Result<changes::Plan> {
        self.store.clear_deadline();
        changes::approve(&self.store, id, allow_tests)
    }
    pub fn register_server(&mut self, server: lsp::Server) -> Result<()> {
        ensure!(
            server.trust,
            "Registering a semantic server requires host trust"
        );
        self.servers.insert(server.language.clone(), server);
        Ok(())
    }
    fn semantic_server(&self, language: &str) -> Option<lsp::Server> {
        self.servers
            .get(language)
            .or_else(|| match language {
                "tsx" => self.servers.get("typescript"),
                "jsx" => self.servers.get("javascript"),
                _ => None,
            })
            .cloned()
    }
}
fn update_metrics(value: &mut Value) -> Result<()> {
    for _ in 0..5 {
        let size = serde_json::to_vec(value)?.len();
        if value["metrics"]["serializedBytes"] == size {
            break;
        }
        value["metrics"]["serializedBytes"] = json!(size);
        value["metrics"]["estimatedTokens"] = json!(size.div_ceil(3));
    }
    Ok(())
}
fn bounded_error(text: &str) -> String {
    let mut result = String::new();
    let mut encoded = 0;
    for c in text.chars().filter(|c| !c.is_control()) {
        let length = if matches!(c, '"' | '\\') {
            2
        } else {
            c.len_utf8()
        };
        if encoded + length > 250 {
            break;
        }
        result.push(c);
        encoded += length;
    }
    result
}
