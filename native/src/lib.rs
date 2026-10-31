pub mod agent_repl;
pub mod analysis;
mod arxiv_rate;
pub mod arxiv_source;
pub mod capture;
pub mod changes;
pub mod compatibility;
mod compiler_hierarchy;
pub mod compiler_inline;
pub mod compiler_move;
pub mod config_graph;
pub mod container;
pub mod context_options;
pub mod contracts;
pub mod credentials;
pub mod dataflow;
pub mod debug_adapter;
pub mod decision_records;
pub mod document_graph;
pub mod editor;
pub mod editor_http;
pub mod explorer_http;
pub mod export;
pub mod falkor_publish;
pub mod falkor_wire;
pub mod federation;
pub mod federation_archive;
pub mod federation_http;
pub mod file_api;
pub mod freshness;
pub mod git_ignore_policy;
pub mod git_workflows;
pub mod github;
pub mod github_app;
pub mod github_app_auth;
pub mod google_workspace;
pub mod graph_collections;
pub mod graph_exports;
pub mod graph_extensions;
pub mod graph_view;
pub mod graph_view_archive;
pub mod graphify_adapter;
pub mod host_config;
pub mod host_credentials;
pub mod host_install;
#[cfg(test)]
#[path = "engine_invoke_deadline_tests.rs"]
mod invoke_deadline_tests;
pub mod json;
pub mod knowledge;
pub mod languages;
pub mod ledger;
pub mod lsp;
pub mod maintenance;
pub mod manifest_graph;
pub mod markdown_graph;
pub mod mcp_http;
pub mod meaning_aliases;
pub mod meaning_build;
pub mod meaning_graph;
pub mod meaning_groups;
pub mod meaning_identity;
pub mod meaning_join;
pub mod meaning_projection;
pub mod meaning_view;
pub mod media;
pub mod media_asr;
pub mod mixed_view;
pub mod model;
pub mod model_bedrock;
pub mod model_http;
mod model_protocol;
pub mod neo4j_publish;
pub mod notes;
mod office;
pub mod ollama;
pub mod paths;
pub mod platform_credentials;
pub mod postgres_catalog;
pub mod postgres_catalog_client;
pub mod process;
pub mod project_activation;
pub mod project_session;
pub mod projects;
pub mod protocol;
pub mod provider_batches;
pub mod providers;
pub mod query;
pub mod query_features;
mod refactor_contract;
pub mod replacement;
pub mod repository_discovery;
pub mod resolution;
pub mod retention;
pub mod retrieval;
mod retrieval_capacity;
#[cfg(test)]
#[path = "retrieval_input_tests.rs"]
mod retrieval_input_tests;
mod retrieval_saved_graph;
mod retrieval_scopes;
pub mod runtime;
pub mod semantic_enrichment;
pub mod semantic_refactor;
pub mod semantic_workspace;
pub mod server_autoselection;
pub mod server_catalog;
pub mod server_selection;
pub mod server_settings;
pub mod session_config;
pub mod session_dashboard;
pub mod session_prompt;
pub mod session_steering;
pub mod skill_takeaways;
pub mod source_acquisition;
pub mod source_acquisition_http;
pub mod source_encoding;
pub mod source_exclusions;
pub mod source_line_endings;
pub mod store;
pub mod symbol_info_budget;
pub mod symbol_workspace;
pub mod test_map;
pub mod tool_budgets;
pub mod tool_usage;
pub mod trail_adapter;
mod validation;
pub mod verify;
pub mod view_comparison;

use anyhow::{Context, Result, bail, ensure};
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
    pub(crate) git_repository_host: Option<git_workflows::repository_host::Host>,
    pub(crate) backend_servers: BTreeMap<String, lsp::ConfiguredServer>,
    pub(crate) selected_backends: Option<Vec<String>>,
    pub(crate) automatic_backend_selection: bool,
    pub(crate) backend_registry_sealed: bool,
    pub(crate) symbol_workspace_folders: Option<Vec<String>>,
    pub(crate) additional_workspace_folders: Option<Vec<String>>,
    pub root: PathBuf,
    pub store: Store,
    pub allow_writes: bool,
    pub servers: BTreeMap<String, lsp::Server>,
    pub capture_options: capture::CaptureOptions,
    /// Host-selected policy for new saved-source captures. Historical reads
    /// recover their policy from the immutable capture proof instead.
    pub capture_epoch: analysis::AnalyzerEpoch,
    pub server_configurations: BTreeMap<String, lsp::HostConfiguration>,
}
enum ConstructionEncoding {
    CaptureProof,
    LegacyOverlay,
}

impl Engine {
    /// Bind already selected host data for direct CLI/library approval paths.
    /// This grants no source scope, writes, provider or execution authority.
    pub fn bind_source_encoding(&mut self) {
        self.capture_options.source_encoding = self
            .capture_options
            .source_encoding
            .and_then(source_encoding::Policy::canonical);
        self.store.source_encoding = self.capture_options.source_encoding;
    }
    pub(crate) fn sync_source_exclusions(&mut self, deadline: &Deadline) -> Result<()> {
        self.bind_source_encoding();
        if let Some(host) = &self.git_repository_host {
            host.check(&self.store, deadline)?;
        }
        let physical_ignored_dirs = self
            .capture_options
            .ignored_directory_identity
            .map(|_| {
                self.capture_options
                    .validated()
                    .map(|options| options.include_ignored_dirs)
            })
            .transpose()?;
        self.store.current_git_ignored_read_only = self.capture_options.include_git_ignored;
        self.store.current_physical_ignored_dirs = physical_ignored_dirs;
        self.store.current_repository_scope = self
            .capture_options
            .repository_scope
            .as_ref()
            .map(capture::repository_scope::WritePolicy::from_scope)
            .transpose()?;
        let policy = self
            .capture_options
            .source_exclusions
            .as_ref()
            .map(source_exclusions::Policy::canonical)
            .transpose()?
            .flatten();
        if self.store.source_exclusion_selection == policy {
            return Ok(());
        }
        let compiled = source_exclusions::Compiled::new(policy.as_ref(), &|| deadline.check())?;
        self.store.current_source_exclusions = (!compiled.is_empty()).then_some(compiled);
        self.store.source_exclusion_selection = policy;
        Ok(())
    }
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        Self::open_with_timeout(root, std::time::Duration::from_secs(10))
    }
    pub fn open_with_timeout(root: impl AsRef<Path>, timeout: std::time::Duration) -> Result<Self> {
        Self::open_with_timeout_inner(root.as_ref(), timeout, None)
    }
    /// Host-only Store setup observations, with no change to opening authority.
    pub fn open_with_timeout_observed(
        root: impl AsRef<Path>,
        timeout: std::time::Duration,
        observations: &mut store::OpeningObservations,
    ) -> Result<Self> {
        *observations = store::OpeningObservations::default();
        Self::open_with_timeout_inner(root.as_ref(), timeout, Some(observations))
    }
    fn open_with_timeout_inner(
        root: &Path,
        timeout: std::time::Duration,
        observations: Option<&mut store::OpeningObservations>,
    ) -> Result<Self> {
        let root = root.canonicalize()?;
        ensure!(root.is_dir(), "Workspace root must be a directory");
        let store = match observations {
            Some(observations) => Store::open_with_timeout_observed(&root, timeout, observations)?,
            None => Store::open_with_timeout(&root, timeout)?,
        };
        Ok(Self {
            git_repository_host: None,
            backend_servers: BTreeMap::new(),
            selected_backends: None,
            automatic_backend_selection: false,
            backend_registry_sealed: false,
            symbol_workspace_folders: None,
            additional_workspace_folders: None,
            root,
            store,
            allow_writes: false,
            servers: BTreeMap::new(),
            capture_options: capture::CaptureOptions::default(),
            capture_epoch: analysis::AnalyzerEpoch::active()?,
            server_configurations: BTreeMap::new(),
        })
    }
    pub fn index(&mut self, deadline: &Deadline) -> Result<(Snapshot, Graph, usize, usize)> {
        self.index_with_graph_policy(deadline, false)
    }
    fn index_with_graph_policy(
        &mut self,
        deadline: &Deadline,
        bounded_saved_graph: bool,
    ) -> Result<(Snapshot, Graph, usize, usize)> {
        if let Some(host) = self.git_repository_host.clone() {
            return host.index(self, deadline, bounded_saved_graph);
        }
        self.sync_source_exclusions(deadline)?;
        if bounded_saved_graph {
            retrieval_scopes::check_deadline(deadline)?;
        }
        self.store.set_deadline(deadline)?;
        let snapshot = capture::capture_with_options_at(
            self.capture_epoch,
            &self.root,
            &self.store,
            &self.capture_options,
            deadline,
        )?;
        let result = self.analyze_snapshot_with_graph_policy(
            self.capture_epoch,
            snapshot,
            deadline,
            bounded_saved_graph,
        )?;
        self.store.check_deadline()?;
        self.store.db.execute("INSERT INTO meta(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", rusqlite::params![freshness::BASELINE_KEY, result.0.id])?;
        Ok(result)
    }
    pub(crate) fn index_retrieval_at(
        &mut self,
        deadline: &Deadline,
    ) -> Result<(Snapshot, Graph, usize, usize)> {
        self.index_with_graph_policy(deadline, true)
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
            ensure!(
                identity(&self.store.snapshot(&snapshot.id)?) == identity(&snapshot),
                "Snapshot differs from the stored immutable manifest"
            );
            self.store.set_latest(&snapshot.id)?;
            deadline.check()?;
            return Ok((snapshot.clone(), graph, 0, snapshot.files.len()));
        }
        let epoch =
            capture::analyzer_epoch_for_snapshot_with_deadline(&self.store, &snapshot, deadline)?;
        self.analyze_snapshot_at(epoch, snapshot, deadline)
    }

    /// Owned constructors supply their epoch before their capture receipt exists.
    /// Public reanalysis must infer policy from the exact immutable capture proof.
    pub(crate) fn analyze_snapshot_at(
        &mut self,
        epoch: analysis::AnalyzerEpoch,
        snapshot: Snapshot,
        deadline: &Deadline,
    ) -> Result<(Snapshot, Graph, usize, usize)> {
        self.analyze_snapshot_with_graph_policy(epoch, snapshot, deadline, false)
    }
    /// The editor has checked every buffer and host path before constructing
    /// this overlay. Its immutable receipt is deliberately published afterward.
    pub(crate) fn analyze_legacy_overlay_at(
        &mut self,
        epoch: analysis::AnalyzerEpoch,
        snapshot: Snapshot,
        base: &Snapshot,
        deadline: &Deadline,
    ) -> Result<(Snapshot, Graph, usize, usize)> {
        self.store.set_deadline(deadline)?;
        ensure!(
            snapshot.kind == "overlay"
                && base.kind == "disk"
                && snapshot.workspace_id == base.workspace_id
                && snapshot.omissions == base.omissions
                && snapshot.capture_consistency == "overlay-from-stable-disk-and-immutable-buffers"
                && identity(&self.store.snapshot(&base.id)?) == identity(base),
            "Owned overlay construction requires its exact published disk base"
        );
        source_encoding::require_legacy(&self.store, base, "UTF-8 editor overlay buffers")?;
        self.analyze_snapshot_with_construction_policy(
            epoch,
            snapshot,
            deadline,
            false,
            ConstructionEncoding::LegacyOverlay,
        )
    }
    fn analyze_snapshot_with_graph_policy(
        &mut self,
        epoch: analysis::AnalyzerEpoch,
        snapshot: Snapshot,
        deadline: &Deadline,
        bounded_saved_graph: bool,
    ) -> Result<(Snapshot, Graph, usize, usize)> {
        self.analyze_snapshot_with_construction_policy(
            epoch,
            snapshot,
            deadline,
            bounded_saved_graph,
            ConstructionEncoding::CaptureProof,
        )
    }
    fn analyze_snapshot_with_construction_policy(
        &mut self,
        epoch: analysis::AnalyzerEpoch,
        snapshot: Snapshot,
        deadline: &Deadline,
        bounded_saved_graph: bool,
        construction_encoding: ConstructionEncoding,
    ) -> Result<(Snapshot, Graph, usize, usize)> {
        self.store.set_deadline(deadline)?;
        ensure!(
            snapshot.workspace_id == self.store.workspace_id,
            "Snapshot workspace scope mismatch"
        );
        let saved_graph = if bounded_saved_graph {
            retrieval_saved_graph::read_graph(&self.store, &snapshot.id, deadline)?
        } else {
            self.store.graph(&snapshot.id).ok()
        };
        if let Some(graph) = saved_graph {
            ensure!(
                capture::analyzer_epoch_for_snapshot_at(&self.store, &snapshot, epoch)? == epoch,
                "Existing graph belongs to another extraction epoch"
            );
            self.store.set_latest(&snapshot.id)?;
            deadline.check()?;
            return Ok((snapshot.clone(), graph, 0, snapshot.files.len()));
        }
        let mut ready = Vec::new();
        let mut pending = Vec::new();
        let mut encoded_parsed = 0;
        // Owned construction already supplies its exact acquisition epoch. A
        // historical-reader lookup would require this new manifest to have
        // been published before its graph can be built.
        let encoding = match construction_encoding {
            ConstructionEncoding::CaptureProof => {
                capture::options_for_snapshot_at(&self.store, &snapshot, epoch)?.source_encoding
            }
            ConstructionEncoding::LegacyOverlay => None,
        };
        for file in &snapshot.files {
            deadline.check()?;
            let key = if let Some(policy) = encoding {
                identity(&(epoch.cache_key(file), source_encoding::VERSION, policy))
            } else {
                epoch.cache_key(file)
            };
            if let Some(extraction) = self.store.cache(&key)? {
                ready.push(extraction);
            } else if encoding.is_some() {
                // Release each bounded mapping before reading the next file.
                // The ordinary parallel UTF-8 path and cache identities stay exact.
                let raw =
                    self.store
                        .read_blob_exact(&file.hash, file.size, source_encoding::MAX_RAW)?;
                let view = source_encoding::View::decode(raw, encoding, &|| deadline.check())?;
                let extraction = source_encoding::extract(epoch, file, &view, deadline)?;
                self.store
                    .put_caches(&[(key, extraction.clone())], deadline)?;
                ready.push(extraction);
                encoded_parsed += 1;
            } else {
                pending.push((
                    key,
                    file.clone(),
                    String::from_utf8(
                        self.store
                            .read_blob_exact(&file.hash, file.size, 4_000_000)?,
                    )?,
                ));
            }
        }
        let reused = ready.len() - encoded_parsed;
        let parsed = pending.len() + encoded_parsed;
        let results: Vec<_> = pending
            .par_iter()
            .map(|(key, file, text)| {
                analysis::extract_at(epoch, file, text, deadline)
                    .map(|extraction| (key.clone(), extraction))
            })
            .collect();
        let results = results.into_iter().collect::<Result<Vec<_>>>()?;
        self.store.put_caches(&results, deadline)?;
        for (_, extraction) in results {
            ready.push(extraction);
        }
        ready.sort_by(|a, b| a.entities[0].path.cmp(&b.entities[0].path));
        let mut graph = analysis::assemble_at(epoch, &snapshot, ready, deadline)?;
        source_encoding::qualify_graph(&mut graph, encoding);
        // Reuse the checked construction policy without a second published-
        // manifest lookup through the current reader epoch.
        self.store.publish_encoded(&snapshot, &graph, encoding)?;
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
    pub(crate) fn select_retrieval_inputs_at(
        &mut self,
        args: &Value,
        deadline: &Deadline,
    ) -> Result<(Snapshot, Graph)> {
        retrieval_scopes::check_deadline(deadline)?;
        if let Some(id) = args["snapshotId"].as_str() {
            self.store.set_deadline(deadline)?;
            let snapshot = self.store.snapshot(id)?;
            let graph = retrieval_saved_graph::read_graph(&self.store, id, deadline)?.ok_or(
                retrieval_scopes::RetrievalHardError::SourceIntegrity("saved_graph_missing"),
            )?;
            return Ok((snapshot, graph));
        }
        let (snapshot, graph, _, _) = self.index_retrieval_at(deadline)?;
        Ok((snapshot, graph))
    }
    pub fn invoke(&mut self, tool: &str, args: Value) -> Result<Value> {
        self.invoke_bound(tool, args, None, None)
    }
    /// Trusted Session/facade route: serialized timeout defaults cannot replace
    /// the actual inherited host ceiling or its live cancellation flags.
    pub(crate) fn invoke_at(
        &mut self,
        tool: &str,
        args: Value,
        deadline: &Deadline,
    ) -> Result<Value> {
        self.invoke_bound(tool, args, None, Some(deadline))
    }
    /// Trusted Session route only; a serialized request cannot install this
    /// provenance context or bypass ordinary validation/write admission.
    pub(crate) fn invoke_document_apply(
        &mut self,
        args: Value,
        binding: changes::DocumentBinding,
    ) -> Result<Value> {
        self.invoke_bound("ci_change_apply", args, Some(binding), None)
    }
    fn invoke_bound(
        &mut self,
        tool: &str,
        args: Value,
        document: Option<changes::DocumentBinding>,
        inherited: Option<&Deadline>,
    ) -> Result<Value> {
        let started = Instant::now();
        let deadline = Deadline::new(
            args["timeoutMs"]
                .as_u64()
                .filter(|n| (10..=600000).contains(n))
                .unwrap_or(120000),
        );
        let deadline = match inherited {
            Some(parent) => deadline.intersection(parent)?,
            None => deadline,
        };
        #[cfg(test)]
        invoke_deadline_tests::after_deadline();
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
            self.sync_source_exclusions(&deadline)?;
            if let Some(id) = args["graphViewId"].as_str() {
                // A selected view supplies its own immutable snapshot. Never
                // capture live source or start a provider while resolving it.
                let view = graph_view::load(&self.store, id, &deadline)?;
                ensure!(
                    args["snapshotId"]
                        .as_str()
                        .is_none_or(|snapshot| snapshot == view.snapshot_id),
                    "Selected graph view snapshot mismatch"
                );
                sid = Some(view.snapshot_id.clone());
                return self.query_graph_view(tool, &args, &view, &deadline);
            }
            if tool == "ci_read" && args.get("artifactId").is_some() {
                // A CAS ID can name generic artifacts or bytes used by many
                // snapshots. Caller-provided source scope is not evidence of
                // this object's membership in that snapshot.
                sid = None;
                let id = args["artifactId"].as_str().unwrap();
                let start = args["cursor"].as_str().unwrap_or("0").parse::<usize>()?;
                let page = self.store.read_artifact_page(
                    id,
                    start,
                    max / 5,
                    args["encoding"] != "base64",
                )?;
                let total = page.total_bytes;
                let bytes = page.bytes;
                if args["encoding"] == "base64" {
                    use base64::Engine as _;
                    let end = start + bytes.len();
                    return Ok(
                        json!({"chunk":base64::prelude::BASE64_STANDARD.encode(&bytes),"encoding":"base64","cursor":start.to_string(),"nextCursor":if end<total{Some(end.to_string())}else{None},"totalBytes":total,"artifactId":id}),
                    );
                }
                let text = String::from_utf8(bytes)?;
                let end = start + text.len();
                return Ok(
                    json!({"chunk":text,"cursor":start.to_string(),"nextCursor":if end<total{Some(end.to_string())}else{None},"totalBytes":total,"artifactId":id}),
                );
            }
            if tool == "ci_status" && args["mode"] == "freshness" {
                return freshness::inspect(self, &deadline);
            }
            if tool == "ci_status" && args["mode"].as_str().unwrap_or("status") == "status" {
                sid = self.store.latest()?;
                let registered_languages = if let Some(ids) = &self.selected_backends {
                    ids.iter()
                        .filter_map(|id| server_catalog::get(id))
                        .flat_map(|entry| entry.languages.iter().copied())
                        .collect::<std::collections::BTreeSet<_>>()
                        .into_iter()
                        .map(str::to_owned)
                        .collect::<Vec<_>>()
                } else {
                    self.servers.keys().cloned().collect::<Vec<_>>()
                };

                let mut status = json!({"version":env!("CARGO_PKG_VERSION"),"runtime":"rust","workspaceId":self.store.workspace_id,"latestSnapshotId":sid,"capabilities":{"structuralLanguages":analysis::LANGUAGES,"sourceSearch":true,"regexSearch":true,"directedGraph":true,"immutableSnapshots":true,"semanticRename":"host-selected-lsp-required","registeredSemanticLanguages":registered_languages,"structuralDataflow":true,"hostWorkflows":["configured_compiler","test_map_collection","model_providers_and_ledger","python_debugger_repl","docker_verification","signed_tls_source_exchange","reviewed_history_retention"],"modelCalls":false,"upstreamApplicationsRequired":false,"workspaceWrites":self.allow_writes},"limitations":["Structural candidates and model inferences are not exhaustive compiler or runtime proofs.","Optional execution, models, credentials, network service and deletion require separate explicit host authority.","All-language-server certification and proprietary hosted features are outside the measured local workflow replacement."]});
                if let Some(ids) = &self.selected_backends {
                    status["capabilities"]["selectedLanguageServers"] = json!(ids);
                    status["capabilities"]["backendSelectionPolicy"] =
                        json!(if self.automatic_backend_selection {
                            "creation_time_captured_filename_counts_then_priority"
                        } else {
                            "first_declared_extension_match_then_first_fallback"
                        });
                    status["capabilities"]["backendRuntimeCertification"] = json!(false);
                }
                if let Some(policy) = self.capture_options.source_encoding {
                    status["capabilities"]["sourceEncoding"] = json!({"schema":source_encoding::VERSION,"selected":policy,"supported":["utf8","utf8_sig","utf16_bom_required","utf16_le","utf16_be","latin1","ascii","windows1252","utf32_bom_required","utf32_le","utf32_be"],"otherCodecs":"unsupported","offsets":"original_source_bytes","semanticWorkspaceEdits":false,"compilerPieceRefactors":false,"runtimeTraceMapping":false,"manifestBalancedRetrieval":false,"fullEncodingIdentityCertified":false});
                }
                return Ok(status);
            }
            if tool == "ci_change_apply" {
                let receipt = if let Some(binding) = &document {
                    changes::apply_document(
                        &self.store,
                        &self.root,
                        &args,
                        self.allow_writes,
                        binding,
                        &deadline,
                    )?
                } else {
                    changes::apply(&self.store, &self.root, &args, self.allow_writes, &deadline)?
                };
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
                let plan = changes::load_checked_plan(&self.store, id, &deadline)?;
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
                "ci_change_prepare" if args.get("bulkReplace").is_some() => {
                    query_features::bulk_replace(
                        self,
                        &snapshot,
                        &graph,
                        &args["bulkReplace"],
                        &deadline,
                    )
                }
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
                    capture::validate_language(&self.store, &snapshot, path)?;
                    let server = self.semantic_server(&file.language, path)?.ok_or_else(|| {
                        anyhow::anyhow!(
                            "No host-authorized semantic server registered for this language"
                        )
                    })?;
                    ensure!(server.trust, "Language server trust is required");
                    let view = query::source_view(&self.store, &snapshot, path)?;
                    let hash = file.hash.clone();
                    lsp::byte_to_position(view.text(), view.to_decoded(offset)?)?;
                    local_only = false;
                    language_server_attempts += 1;
                    let mut response = self.run_semantic_at(
                        &server,
                        &snapshot,
                        &graph,
                        lsp::SemanticRequest {
                            path,
                            offset,
                            operation,
                            new_name: None,
                        },
                        &request.get("options").cloned().unwrap_or_else(|| json!({})),
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
                "ci_find"
                    if args["kind"].as_str().unwrap_or("symbol") == "symbol"
                        && self.symbol_workspace_folders.is_some() =>
                {
                    query_features::symbols_find_selected(
                        &self.store,
                        &snapshot,
                        &graph,
                        &args,
                        self.symbol_workspace_folders.as_deref().unwrap(),
                        &deadline,
                    )
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
                        let file = snapshot
                            .files
                            .iter()
                            .find(|file| file.path == entity.path)
                            .ok_or_else(|| {
                                anyhow::anyhow!("Rename source file absent from snapshot")
                            })?;
                        capture::validate_language(&self.store, &snapshot, &entity.path)?;
                        let server = self.semantic_server(&file.language, &entity.path)?.ok_or_else(|| {
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
                        return self.run_semantic_at(
                            &server,
                            &snapshot,
                            &graph,
                            lsp::SemanticRequest {
                                path: &entity.path,
                                offset,
                                operation: "rename",
                                new_name: Some(name),
                            },
                            &json!({}),
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
    fn query_graph_view(
        &self,
        tool: &str,
        args: &Value,
        view: &graph_view::View,
        deadline: &Deadline,
    ) -> Result<Value> {
        let mut options = args.clone();
        options.as_object_mut().unwrap().retain(|key, _| {
            ![
                "graphViewId",
                "snapshotId",
                "maxBytes",
                "tokenBudget",
                "timeoutMs",
                "taskId",
            ]
            .contains(&key.as_str())
        });
        let mut data = match tool {
            "ci_find" => graph_view::find(&self.store, view, &options, deadline)?,
            "ci_trace" => graph_view::trace(&self.store, view, &options, deadline)?,
            "ci_context" => graph_view::context(&self.store, view, &options, deadline)?,
            "ci_read" => self.read_graph_view(view, &options, deadline)?,
            _ => bail!("Selected graph views support read-only query tools"),
        };
        let graph = self.store.graph(&view.snapshot_id)?;
        data["graphViewId"] = json!(view.id);
        data["graphView"] = json!({
            "id":view.id,"artifactId":view.artifact_id,
            "snapshotId":view.snapshot_id,"workspaceId":view.workspace_id,
            "baseGraphHash":view.base_graph_hash,"overlays":view.overlays,
            "coverage":graph.coverage,"complete":view.complete,
            "limitations":view.limitations,
        });
        if let Some(meaning) = &view.meaning {
            data["graphView"]["meaning"] = json!({
                "scope": meaning.scope,
                "selections": meaning.selections,
                "resolution": "inferred_concept",
                "providerAttestation": "unverified",
            });
        }
        deadline.check()?;
        Ok(data)
    }
    fn read_graph_view(
        &self,
        view: &graph_view::View,
        args: &Value,
        deadline: &Deadline,
    ) -> Result<Value> {
        if let Some(data) = graph_view::read_meaning(&self.store, view, args, deadline)? {
            return Ok(data);
        }
        let snapshot = self.store.snapshot(&view.snapshot_id)?;
        let graph = self.store.graph(&view.snapshot_id)?;
        let mut options = args.clone();
        let mut declaration = None;
        if let Some(id) = args["entityId"].as_str() {
            let node = view
                .nodes
                .iter()
                .find(|node| node["id"] == id)
                .ok_or_else(|| anyhow::anyhow!("Unknown selected graph view entity"))?;
            let bindings = node["sourceBindings"]
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("Selected entity has no source bindings"))?;
            ensure!(
                bindings.len() == 1,
                "Selected entity has ambiguous source anchors; read an explicit snapshot path and range"
            );
            let binding = &bindings[0];
            options.as_object_mut().unwrap().remove("entityId");
            options["path"] = binding["path"].clone();
            if ["startByte", "endByte", "startLine", "endLine"]
                .iter()
                .all(|key| args.get(key).is_none())
            {
                options["startByte"] = binding["startByte"].clone();
                options["endByte"] = binding["endByte"].clone();
            }
            declaration = Some(binding.clone());
        }
        deadline.check()?;
        let mut data = query::read(&self.store, &snapshot, &graph, &options)?;
        if let Some(binding) = declaration {
            ensure!(
                data["contentHash"] == binding["contentHash"],
                "Selected entity source hash mismatch"
            );
            data["entityId"] = args["entityId"].clone();
            data["declarationAnchor"] = binding;
        }
        Ok(data)
    }
    pub fn register_server(&mut self, server: lsp::Server) -> Result<()> {
        ensure!(
            server.trust,
            "Registering a semantic server requires host trust"
        );
        self.server_configurations.remove(&server.language);
        self.servers.insert(server.language.clone(), server);
        Ok(())
    }
    /// Dormant host capability. A project policy must separately select its ID.
    pub fn register_backend(&mut self, id: &str, server: lsp::ConfiguredServer) -> Result<()> {
        ensure!(
            !self.backend_registry_sealed,
            "Backend registrations are fixed for an open project session"
        );
        ensure!(
            !self.backend_servers.contains_key(id),
            "Duplicate host backend registration"
        );
        let server = server_selection::checked(id, server)?;
        let mut next = self.backend_servers.clone();
        next.insert(id.to_owned(), server);
        server_selection::bindings(&next)?;
        self.backend_servers = next;
        Ok(())
    }
    pub(crate) fn selected_provider(
        &self,
        path: &str,
    ) -> Result<Option<(String, lsp::ConfiguredServer)>> {
        let Some(id) = self.selected_backend_id(path)? else {
            return Ok(None);
        };
        let server = self
            .backend_servers
            .get(id)
            .context("language_server_not_host_registered")?
            .clone();
        Ok(Some((id.to_owned(), server)))
    }
    pub(crate) fn selected_backend_id(&self, path: &str) -> Result<Option<&str>> {
        let Some(ids) = &self.selected_backends else {
            return Ok(None);
        };
        if self.automatic_backend_selection {
            let selected = ids
                .iter()
                .find(|id| server_autoselection::matches_backend(id, path));
            if let Some(id) = selected {
                ensure!(
                    self.backend_servers.contains_key(id),
                    "language_server_not_host_registered"
                );
            }
            return Ok(selected.map(String::as_str));
        }
        let id = server_selection::choose(ids, path)?;
        ensure!(
            self.backend_servers.contains_key(id),
            "language_server_not_host_registered"
        );
        Ok(Some(id))
    }
    pub(crate) fn semantic_host(
        &self,
        path: &str,
        language: &str,
    ) -> Result<Option<(lsp::Server, Option<lsp::HostConfiguration>)>> {
        if let Some((_, p)) = self.selected_provider(path)? {
            return Ok(Some((p.server, Some(p.configuration))));
        }
        if self.automatic_backend_selection {
            return Ok(None);
        }
        Ok(self
            .servers
            .get(language)
            .cloned()
            .map(|server| (server, self.server_configurations.get(language).cloned())))
    }
    pub fn register_configured_server(&mut self, server: lsp::ConfiguredServer) -> Result<()> {
        let language = server.server.language.clone();
        self.register_server(server.server)?;
        self.server_configurations
            .insert(language, server.configuration);
        Ok(())
    }
    fn run_semantic_at(
        &mut self,
        server: &lsp::Server,
        snapshot: &Snapshot,
        graph: &Graph,
        request: lsp::SemanticRequest<'_>,
        options: &Value,
        deadline: &Deadline,
    ) -> Result<Value> {
        if let Some(configuration) = self.server_configurations.get(&server.language).cloned() {
            lsp::run_at_configured(
                self,
                &lsp::ConfiguredServer {
                    server: server.clone(),
                    configuration,
                },
                snapshot,
                graph,
                request,
                options,
                deadline,
            )
        } else {
            lsp::run_at_with_options(self, server, snapshot, graph, request, options, deadline)
        }
    }
    fn semantic_server(&self, language: &str, path: &str) -> Result<Option<lsp::Server>> {
        if let Some((_, p)) = self.selected_provider(path)? {
            return Ok(Some(p.server));
        }
        if self.automatic_backend_selection {
            return Ok(None);
        }
        Ok(self
            .servers
            .get(language)
            .or_else(|| match language {
                "tsx" => self.servers.get("typescript"),
                "jsx" => self.servers.get("javascript"),
                _ => None,
            })
            .cloned())
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
