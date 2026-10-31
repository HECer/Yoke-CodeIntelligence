use serde_json::{Value, json};
use std::{collections::BTreeSet, fs};
use tempfile::TempDir;
use yoke::{
    Engine,
    analysis::AnalyzerEpoch,
    capture::Deadline,
    graph_view::{self, BuildOptions, StructuralKind, View},
    project_session::{HostProfile, Session},
};

const SOURCE: &str = "fn α(){ beta(); beta(); }\nfn beta(){ gamma(); }\nfn gamma(){}\n";
const GUIDE: &str = "# Flow\n`main.rs::α`\n`main.rs::α`\n";

struct Fixture {
    session: Session,
    selected: View,
    default: View,
    root: TempDir,
}

fn fixture(epoch: AnalyzerEpoch) -> Fixture {
    fixture_with_source(epoch, SOURCE)
}

fn fixture_with_source(epoch: AnalyzerEpoch, source: &str) -> Fixture {
    let root = TempDir::new().unwrap();
    fs::write(root.path().join("main.rs"), source).unwrap();
    fs::write(root.path().join("guide.md"), GUIDE).unwrap();
    fs::write(
        root.path().join("package.json"),
        r#"{"name":"selected-query-app","dependencies":{"external":"^1"}}"#,
    )
    .unwrap();
    let deadline = Deadline::new(30000);
    let mut engine = Engine::open(root.path()).unwrap();
    engine.capture_epoch = epoch;
    let (snapshot, graph, _, _) = engine.index(&deadline).unwrap();
    let default = graph_view::build(
        &engine.store,
        &snapshot,
        &graph,
        &BuildOptions::default(),
        &deadline,
    )
    .unwrap();
    let selected = graph_view::build(
        &engine.store,
        &snapshot,
        &graph,
        &BuildOptions {
            structural_kinds: vec![StructuralKind::Markdown],
            ..BuildOptions::default()
        },
        &deadline,
    )
    .unwrap();
    assert_ne!(selected.id, default.id);
    assert!(default.nodes.iter().any(|node| node["kind"] == "package"));
    assert!(!selected.nodes.iter().any(|node| node["kind"] == "package"));
    let session = Session::open(
        engine,
        HostProfile::default(),
        Some("graphify-selected-queries"),
        &deadline,
    )
    .unwrap();
    Fixture {
        session,
        selected,
        default,
        root,
    }
}

fn node_id(view: &View, kind: &str, name: &str) -> String {
    view.nodes
        .iter()
        .find(|node| node["kind"] == kind && node["name"] == name)
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn selected_call(f: &mut Fixture, name: &str, mut args: Value) -> Value {
    args["graphViewId"] = json!(f.selected.id);
    let result = yoke::graphify_adapter::invoke(&mut f.session, name, &args, &Deadline::new(30000))
        .unwrap_or_else(|error| {
            panic!("{name} rejected a genuine published selected view: {error:#}")
        });
    assert_eq!(result["viewId"], f.selected.id);
    assert_eq!(result["snapshotId"], f.selected.snapshot_id);
    assert_eq!(result["baseGraphHash"], f.selected.base_graph_hash);
    assert_eq!(result["complete"], false);
    result
}

fn labels(value: &Value) -> BTreeSet<String> {
    value["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|node| node["name"].as_str().unwrap().to_owned())
        .collect()
}

fn counts(session: &Session) -> (i64, i64, i64) {
    let count = |table: &str| {
        session
            .engine
            .store
            .db
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap()
    };
    (count("snapshots"), count("graphs"), count("records"))
}

#[test]
fn selected_get_node_resolves_only_members_of_the_published_nondefault_view() {
    let mut f = fixture(AnalyzerEpoch::V8);
    let document = node_id(&f.selected, "document", "guide.md");
    let result = selected_call(&mut f, "get_node", json!({"node_id":document}));
    assert_eq!(result["found"], true);
    assert_eq!(result["node"]["kind"], "document");
    assert_eq!(result["node"]["path"], "guide.md");
    assert_eq!(result["node"]["origin"], "structural_overlay");
    let excluded = selected_call(&mut f, "get_node", json!({"label":"selected-query-app"}));
    assert_eq!(excluded["found"], false);
    assert_eq!(excluded["candidates"], json!([]));
}

#[test]
fn selected_neighbors_preserve_parallel_directions_and_exact_utf8_evidence() {
    let mut f = fixture(AnalyzerEpoch::V8);
    let result = selected_call(
        &mut f,
        "get_neighbors",
        json!({"label":"α","relation_filter":"calls","token_budget":65536}),
    );
    assert_eq!(labels(&result), BTreeSet::from(["α".into(), "beta".into()]));
    let alpha = node_id(&f.selected, "function", "α");
    let beta = node_id(&f.selected, "function", "beta");
    let edges = result["observations"].as_array().unwrap();
    assert_eq!(edges.len(), 2);
    assert_ne!(edges[0]["id"], edges[1]["id"]);
    for edge in edges {
        assert_eq!(edge["source"], alpha);
        assert_eq!(edge["target"], beta);
        assert_eq!(edge["relation"], "calls");
    }
    let sites = result["evidence"].as_array().unwrap();
    assert_eq!(sites.len(), 2);
    let expected: BTreeSet<_> = SOURCE
        .match_indices("beta()")
        .take(2)
        .map(|(start, text)| (start, start + text.len()))
        .collect();
    let actual: BTreeSet<_> = sites
        .iter()
        .map(|site| {
            assert_eq!(site["path"], "main.rs");
            assert_eq!(site["contentHash"], yoke::model::hash(SOURCE.as_bytes()));
            let start = site["startByte"].as_u64().unwrap() as usize;
            let end = site["endByte"].as_u64().unwrap() as usize;
            assert_eq!(&SOURCE[start..end], "beta()");
            (start, end)
        })
        .collect();
    assert_eq!(actual, expected);

    let document = node_id(&f.selected, "document", "guide.md");
    let result = selected_call(
        &mut f,
        "get_neighbors",
        json!({"node_id":document,"relation_filter":"cites_symbol","token_budget":65536}),
    );
    assert_eq!(result["observations"].as_array().unwrap().len(), 2);
    for edge in result["observations"].as_array().unwrap() {
        assert_eq!(edge["source"], document);
        assert_eq!(edge["target"], alpha);
        assert_eq!(edge["resolution"], "unique_name_candidate");
    }
    let sites = result["evidence"].as_array().unwrap();
    assert_eq!(sites.len(), 2);
    assert_ne!(sites[0]["id"], sites[1]["id"]);
    for site in sites {
        assert_eq!(site["path"], "guide.md");
        assert_eq!(site["contentHash"], yoke::model::hash(GUIDE.as_bytes()));
        let start = site["startByte"].as_u64().unwrap() as usize;
        let end = site["endByte"].as_u64().unwrap() as usize;
        assert_eq!(&GUIDE[start..end], "`main.rs::α`");
    }
}

#[test]
fn selected_query_graph_traverses_the_selected_document_and_induced_parallel_edges() {
    let mut f = fixture(AnalyzerEpoch::V8);
    let document = node_id(&f.selected, "document", "guide.md");
    for mode in ["bfs", "dfs"] {
        let result = selected_call(
            &mut f,
            "query_graph",
            json!({"question":document,"mode":mode,"depth":3,"token_budget":65536}),
        );
        assert_eq!(
            labels(&result),
            BTreeSet::from(["guide.md".into(), "α".into(), "beta".into(), "gamma".into()])
        );
        assert_eq!(result["traversalDirection"], "stored_outgoing");
        let edges = result["observations"].as_array().unwrap();
        assert_eq!(edges.len(), 5);
        assert_eq!(
            edges
                .iter()
                .filter(|edge| edge["relation"] == "calls")
                .count(),
            3
        );
        assert_eq!(
            edges
                .iter()
                .filter(|edge| edge["relation"] == "cites_symbol")
                .count(),
            2
        );
        assert_eq!(result["evidence"].as_array().unwrap().len(), 5);
    }
}

#[test]
fn selected_shortest_path_keeps_stored_direction_and_an_explicit_undirected_option() {
    let mut f = fixture(AnalyzerEpoch::V8);
    let document = node_id(&f.selected, "document", "guide.md");
    let result = selected_call(
        &mut f,
        "shortest_path",
        json!({"source":document,"target":"gamma","max_hops":3}),
    );
    assert_eq!(result["found"], true);
    assert_eq!(result["hops"], 3);
    let nodes = result["nodes"].as_array().unwrap();
    assert_eq!(
        nodes
            .iter()
            .map(|node| node["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["guide.md", "α", "beta", "gamma"]
    );
    let edges = result["observations"].as_array().unwrap();
    assert_eq!(edges.len(), 3);
    assert_eq!(
        result["pathObservationIds"],
        json!(edges.iter().map(|edge| &edge["id"]).collect::<Vec<_>>())
    );
    for (index, edge) in edges.iter().enumerate() {
        assert_eq!(edge["source"], nodes[index]["id"]);
        assert_eq!(edge["target"], nodes[index + 1]["id"]);
    }
    let reverse = selected_call(
        &mut f,
        "shortest_path",
        json!({"source":"gamma","target":document,"max_hops":3}),
    );
    assert_eq!(reverse["found"], false);
    assert_eq!(reverse["nodes"], json!([]));
    let undirected = selected_call(
        &mut f,
        "shortest_path",
        json!({"source":"gamma","target":document,"max_hops":3,"undirected":true}),
    );
    assert_eq!(undirected["found"], true);
    assert_eq!(undirected["hops"], 3);
    assert_eq!(undirected["undirected"], true);
}

fn query_cases() -> [(&'static str, Value); 4] {
    [
        ("get_node", json!({"label":"α"})),
        (
            "get_neighbors",
            json!({"label":"α","relation_filter":"calls","token_budget":65536}),
        ),
        (
            "query_graph",
            json!({"question":"α","depth":2,"token_budget":65536}),
        ),
        ("shortest_path", json!({"source":"α","target":"gamma"})),
    ]
}

#[test]
fn selected_queries_replay_historical_views_after_new_index_and_unsaved_host_changes() {
    for epoch in [AnalyzerEpoch::V8, AnalyzerEpoch::V9] {
        let mut f = fixture(epoch);
        fs::write(f.root.path().join("main.rs"), "fn newer(){}\n").unwrap();
        fs::write(f.root.path().join("guide.md"), "# Newer\n").unwrap();
        let (latest, _, _, _) = f.session.engine.index(&Deadline::new(30000)).unwrap();
        assert_ne!(latest.id, f.selected.snapshot_id);
        fs::write(f.root.path().join("main.rs"), "fn not_indexed(){}\n").unwrap();
        let before = counts(&f.session);
        assert!(f.session.engine.servers.is_empty());
        for (name, args) in query_cases() {
            let result = selected_call(&mut f, name, args);
            assert_eq!(result["snapshotId"], f.selected.snapshot_id);
            if name == "get_node" {
                assert_eq!(result["node"]["name"], "α");
            } else {
                assert!(labels(&result).contains("α"), "{name}: {result}");
                assert!(!labels(&result).contains("newer"));
            }
        }
        assert_eq!(
            counts(&f.session),
            before,
            "historical reads published or captured state"
        );
        assert_eq!(f.session.engine.store.latest().unwrap(), Some(latest.id));
        assert_eq!(
            fs::read_to_string(f.root.path().join("main.rs")).unwrap(),
            "fn not_indexed(){}\n"
        );
    }
}

#[test]
fn unknown_and_malformed_view_ids_refuse_before_first_capture() {
    let root = TempDir::new().unwrap();
    fs::write(root.path().join("main.rs"), SOURCE).unwrap();
    let mut session = Session::open(
        Engine::open(root.path()).unwrap(),
        HostProfile::default(),
        Some("unknown-graphify-selected-query"),
        &Deadline::new(30000),
    )
    .unwrap();
    assert_eq!(session.engine.store.latest().unwrap(), None);
    let before = counts(&session);
    for selected in [
        json!("f".repeat(64)),
        json!("invalid"),
        Value::Null,
        json!(7),
    ] {
        for (name, mut args) in query_cases() {
            args["graphViewId"] = selected.clone();
            assert!(
                yoke::graphify_adapter::invoke(&mut session, name, &args, &Deadline::new(30000))
                    .is_err(),
                "{name}: {args}"
            );
        }
    }
    assert_eq!(session.engine.store.latest().unwrap(), None);
    assert_eq!(counts(&session), before);
    assert!(session.engine.servers.is_empty());
}

#[test]
fn mismatched_published_view_scope_cannot_fall_back_to_live_capture() {
    let mut f = fixture(AnalyzerEpoch::V8);
    f.session
        .engine
        .store
        .db
        .execute(
            "UPDATE records SET value=json_set(value,'$.workspaceId',?1) WHERE kind=?2 AND id=?3",
            rusqlite::params!["e".repeat(64), graph_view::RECORD_KIND, f.selected.id],
        )
        .unwrap();
    fs::write(
        f.root.path().join("main.rs"),
        "fn mismatch_must_not_capture(){}\n",
    )
    .unwrap();
    let before = counts(&f.session);
    let latest = f.session.engine.store.latest().unwrap();
    for (name, mut args) in query_cases() {
        args["graphViewId"] = json!(f.selected.id);
        assert!(
            yoke::graphify_adapter::invoke(&mut f.session, name, &args, &Deadline::new(30000))
                .is_err(),
            "{name} accepted an inconsistent source scope"
        );
    }
    assert_eq!(counts(&f.session), before);
    assert_eq!(f.session.engine.store.latest().unwrap(), latest);
}

#[test]
fn omitted_selector_keeps_the_default_view_after_a_nondefault_view_was_published() {
    let mut f = fixture(AnalyzerEpoch::V8);
    for (name, args) in query_cases() {
        let result =
            yoke::graphify_adapter::invoke(&mut f.session, name, &args, &Deadline::new(30000))
                .unwrap();
        assert_eq!(
            result["viewId"], f.default.id,
            "{name} changed the omission default"
        );
        if name == "get_node" {
            assert_eq!(result["node"]["name"], "α");
        } else if name == "shortest_path" {
            assert_eq!(result["found"], true);
            assert_eq!(result["hops"], 2);
        } else {
            assert!(labels(&result).contains("beta"));
        }
    }
    let package = yoke::graphify_adapter::invoke(
        &mut f.session,
        "get_node",
        &json!({"label":"selected-query-app"}),
        &Deadline::new(30000),
    )
    .unwrap();
    assert_eq!(package["node"]["kind"], "package");
    assert_eq!(package["viewId"], f.default.id);
}

const PARALLEL_PATH_SOURCE: &str =
    "fn α(){ beta(); beta(); }\r\nfn beta(){ α(); gamma(); }\r\nfn gamma(){}\r\n";

#[test]
fn selected_shortest_path_preserves_parallel_hop_provenance_through_historical_replay() {
    let mut f = fixture_with_source(AnalyzerEpoch::V8, PARALLEL_PATH_SOURCE);
    let alpha = node_id(&f.selected, "function", "α");
    let beta = node_id(&f.selected, "function", "beta");
    let gamma = node_id(&f.selected, "function", "gamma");
    let original = serde_json::to_vec(&f.selected).unwrap();
    let deadline = Deadline::new(30000);
    let legacy_args = json!({"source":alpha,"target":gamma,"max_hops":2});
    let legacy =
        yoke::graphify_adapter::invoke(&mut f.session, "shortest_path", &legacy_args, &deadline)
            .unwrap();
    assert!(legacy.get("pathHops").is_none());
    let mut first_args = legacy_args.clone();
    first_args["path_policy"] = json!("first_observation_v1");
    assert_eq!(
        yoke::graphify_adapter::invoke(&mut f.session, "shortest_path", &first_args, &deadline)
            .unwrap(),
        legacy
    );

    let args = json!({
        "source":alpha,"target":gamma,"max_hops":2,
        "path_policy":"weakest_source_categories_v1"
    });
    let result = selected_call(&mut f, "shortest_path", args.clone());
    assert_eq!(result["found"], true);
    assert_eq!(result["hops"], 2);
    assert_eq!(
        result["pathHops"].as_array().map(Vec::len),
        Some(2),
        "The selected policy must expose each traversed hop with every original parallel claim"
    );
    let hops = result["pathHops"].as_array().unwrap();
    for (index, (source, target, expected_ranges)) in [
        (&alpha, &beta, vec![(9u64, 15u64), (17, 23)]),
        (&beta, &gamma, vec![(45u64, 52u64)]),
    ]
    .into_iter()
    .enumerate()
    {
        let originals: Vec<_> = f
            .selected
            .observations
            .iter()
            .filter(|edge| edge["source"] == *source && edge["target"] == *target)
            .cloned()
            .collect();
        assert_eq!(originals.len(), if index == 0 { 2 } else { 1 });
        assert_eq!(hops[index]["traversalSourceId"], *source);
        assert_eq!(hops[index]["traversalTargetId"], *target);
        assert_eq!(
            hops[index]["selectedObservationId"],
            result["pathObservationIds"][index]
        );
        assert_eq!(hops[index]["observations"], json!(originals));
        assert_eq!(
            hops[index]["observationIds"],
            json!(originals.iter().map(|edge| &edge["id"]).collect::<Vec<_>>())
        );
        let evidence = hops[index]["evidence"].as_array().unwrap();
        assert_eq!(evidence.len(), if index == 0 { 2 } else { 1 });
        let evidence_ids: BTreeSet<_> = evidence
            .iter()
            .map(|site| site["id"].as_str().unwrap().to_owned())
            .collect();
        let claimed_ids: BTreeSet<_> = originals
            .iter()
            .flat_map(|edge| {
                edge["evidenceIds"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|id| id.as_str().unwrap().to_owned())
            })
            .collect();
        assert_eq!(evidence_ids, claimed_ids);
        let original_evidence: Vec<_> = f
            .selected
            .evidence
            .iter()
            .filter(|site| claimed_ids.contains(site["id"].as_str().unwrap()))
            .cloned()
            .collect();
        assert_eq!(hops[index]["evidence"], json!(original_evidence));
        let mut ranges = vec![];
        for site in evidence {
            assert_eq!(site["path"], "main.rs");
            assert_eq!(
                site["contentHash"],
                "3fe26558c099cd3ee3ef2bcb3ec246b40c1e21e8694d7884b87f3946bf82e8dd"
            );
            let start = site["startByte"].as_u64().unwrap();
            let end = site["endByte"].as_u64().unwrap();
            assert_eq!(
                &PARALLEL_PATH_SOURCE.as_bytes()[start as usize..end as usize],
                if index == 0 {
                    b"beta()" as &[u8]
                } else {
                    b"gamma()"
                }
            );
            ranges.push((start, end));
        }
        ranges.sort();
        assert_eq!(ranges, expected_ranges);
    }
    assert_eq!(
        serde_json::to_vec(
            &graph_view::load(&f.session.engine.store, &f.selected.id, &deadline).unwrap()
        )
        .unwrap(),
        original
    );
    fs::write(f.root.path().join("main.rs"), "fn newer(){}\n").unwrap();
    let (latest, _, _, _) = f.session.engine.index(&deadline).unwrap();
    assert_ne!(latest.id, f.selected.snapshot_id);
    fs::write(f.root.path().join("main.rs"), "fn not_indexed(){}\n").unwrap();
    let before = counts(&f.session);
    let replay = selected_call(&mut f, "shortest_path", args);
    assert_eq!(replay, result);
    assert_eq!(counts(&f.session), before);
    assert_eq!(f.session.engine.store.latest().unwrap(), Some(latest.id));
    assert_eq!(
        fs::read(f.root.path().join("main.rs")).unwrap(),
        b"fn not_indexed(){}\n"
    );
    assert!(f.session.engine.servers.is_empty());
}

#[test]
fn selected_undirected_hops_preserve_opposite_stored_directions_and_all_parallel_copies() {
    let mut f = fixture_with_source(AnalyzerEpoch::V8, PARALLEL_PATH_SOURCE);
    let alpha = node_id(&f.selected, "function", "α");
    let beta = node_id(&f.selected, "function", "beta");
    let args = json!({
        "source":beta,"target":alpha,"max_hops":1,
        "path_policy":"weakest_source_categories_v1"
    });
    let directed = selected_call(&mut f, "shortest_path", args.clone());
    assert_eq!(directed["found"], true);
    assert_eq!(
        directed["pathHops"][0]["observations"]
            .as_array()
            .map(Vec::len),
        Some(1),
        "Directed hop provenance must exclude copies whose stored direction opposes traversal"
    );
    let mut undirected_args = args;
    undirected_args["undirected"] = json!(true);
    let undirected = selected_call(&mut f, "shortest_path", undirected_args);
    assert_eq!(undirected["found"], true);
    let hop = &undirected["pathHops"][0];
    assert_eq!(
        hop["observations"].as_array().map(Vec::len),
        Some(3),
        "Undirected hop provenance must retain the two forward calls and original reverse call"
    );
    assert_eq!(hop["traversalSourceId"], beta);
    assert_eq!(hop["traversalTargetId"], alpha);
    let originals: Vec<_> = f
        .selected
        .observations
        .iter()
        .filter(|edge| {
            (edge["source"] == alpha && edge["target"] == beta)
                || (edge["source"] == beta && edge["target"] == alpha)
        })
        .cloned()
        .collect();
    assert_eq!(originals.len(), 3);
    assert_eq!(hop["observations"], json!(originals));
    let sites = hop["evidence"].as_array().unwrap();
    assert_eq!(sites.len(), 3);
    let claimed_ids: BTreeSet<_> = originals
        .iter()
        .flat_map(|edge| {
            edge["evidenceIds"]
                .as_array()
                .unwrap()
                .iter()
                .map(|id| id.as_str().unwrap().to_owned())
        })
        .collect();
    let original_evidence: Vec<_> = f
        .selected
        .evidence
        .iter()
        .filter(|site| claimed_ids.contains(site["id"].as_str().unwrap()))
        .cloned()
        .collect();
    assert_eq!(hop["evidence"], json!(original_evidence));
    let mut ranges: Vec<_> = sites
        .iter()
        .map(|site| {
            assert_eq!(site["path"], "main.rs");
            assert_eq!(
                site["contentHash"],
                "3fe26558c099cd3ee3ef2bcb3ec246b40c1e21e8694d7884b87f3946bf82e8dd"
            );
            (
                site["startByte"].as_u64().unwrap(),
                site["endByte"].as_u64().unwrap(),
            )
        })
        .collect();
    ranges.sort();
    assert_eq!(ranges, [(9, 15), (17, 23), (39, 43)]);
}

#[test]
fn selected_parallel_hop_output_refuses_capacity_without_publishing_or_losing_source_bytes() {
    let source = format!(
        "fn α(){{ {} }}\r\nfn beta(){{}}\r\n",
        "beta();".repeat(1200)
    );
    let mut f = fixture_with_source(AnalyzerEpoch::V8, &source);
    let alpha = node_id(&f.selected, "function", "α");
    let beta = node_id(&f.selected, "function", "beta");
    assert_eq!(
        f.selected
            .observations
            .iter()
            .filter(|edge| edge["source"] == alpha && edge["target"] == beta)
            .count(),
        1200
    );
    let legacy = selected_call(
        &mut f,
        "shortest_path",
        json!({"source":alpha,"target":beta,"max_hops":1}),
    );
    assert_eq!(legacy["found"], true);
    assert_eq!(legacy["observations"].as_array().unwrap().len(), 1);
    assert!(legacy.get("pathHops").is_none());
    let original = serde_json::to_vec(&f.selected).unwrap();
    let before = counts(&f.session);
    let latest = f.session.engine.store.latest().unwrap();
    let error = yoke::graphify_adapter::invoke(
        &mut f.session,
        "shortest_path",
        &json!({
            "graphViewId":f.selected.id,"source":alpha,"target":beta,"max_hops":1,
            "path_policy":"weakest_source_categories_v1"
        }),
        &Deadline::new(30000),
    )
    .expect_err("Complete parallel provenance exceeds capacity and must not be silently clipped");
    assert!(error.to_string().contains("capacity"), "{error:#}");
    assert_eq!(counts(&f.session), before);
    assert_eq!(f.session.engine.store.latest().unwrap(), latest);
    assert_eq!(
        serde_json::to_vec(
            &graph_view::load(
                &f.session.engine.store,
                &f.selected.id,
                &Deadline::new(30000)
            )
            .unwrap()
        )
        .unwrap(),
        original
    );
    assert_eq!(
        fs::read(f.root.path().join("main.rs")).unwrap(),
        source.as_bytes()
    );
    assert!(f.session.engine.servers.is_empty());
}
