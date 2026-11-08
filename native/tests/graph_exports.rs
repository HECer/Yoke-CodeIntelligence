use std::fs;
use tempfile::TempDir;
use yoke::{
    Engine,
    capture::Deadline,
    graph_exports::{self, ExportFormat, ExportOptions},
    model::{Graph, Snapshot, identity},
};
fn fixture() -> (TempDir, Engine, Snapshot, Graph) {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("main.rs"),
        "fn a(){b();b();unknown();}\nfn b(){a();}\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("notes.md"),
        "# </script><img src=x onerror=alert(1)> ' \" & [x](javascript:alert(1))\n",
    )
    .unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    let (s, g, _, _) = engine.index(&Deadline::new(10000)).unwrap();
    (dir, engine, s, g)
}
#[test]
fn cypher_is_escaped_and_keeps_each_directed_parallel_observation() {
    let (_dir, engine, s, g) = fixture();
    let bundle = graph_exports::generate(
        &engine.store,
        &s,
        &g,
        &ExportOptions::new(ExportFormat::Cypher),
        &Deadline::new(10000),
    )
    .unwrap();
    let text = &bundle.files[0].content;
    assert_eq!(
        text.matches("MERGE (s)-[r:YOKE_OBSERVATION").count(),
        g.observations.len()
    );
    for arc in &g.observations {
        assert!(text.contains(&arc.id));
        assert!(text.contains(&arc.source));
    }
    assert!(text.contains("\\\""));
    assert!(text.contains("unresolved"));
    assert!(!text.contains("not runtime or compiler proof".to_uppercase().as_str()));
    assert!(text.contains("not runtime or compiler proof"));
    assert_eq!(bundle.graph_hash, identity(&g));
    assert!(!bundle.complete);
    assert_eq!(bundle.omitted_observations, 0);
}
#[test]
fn canvas_is_parseable_directional_and_source_bound() {
    let (_dir, engine, s, g) = fixture();
    let bundle = graph_exports::generate(
        &engine.store,
        &s,
        &g,
        &ExportOptions::new(ExportFormat::Canvas),
        &Deadline::new(10000),
    )
    .unwrap();
    let value: serde_json::Value = serde_json::from_str(&bundle.files[0].content).unwrap();
    assert_eq!(
        value["edges"].as_array().unwrap().len(),
        g.observations.len()
    );
    for edge in value["edges"].as_array().unwrap() {
        assert_eq!(edge["toEnd"], "arrow");
        assert_eq!(edge["fromEnd"], "none");
    }
    assert_eq!(value["yoke"]["snapshotId"], s.id);
    assert_eq!(value["yoke"]["complete"], false);
    assert!(
        value["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|node| node["text"].as_str().unwrap_or("").contains("UTF-8 bytes"))
    );
}
#[test]
fn svg_and_html_exports_never_execute_hostile_source_markup() {
    let (_dir, engine, s, g) = fixture();
    for format in [
        ExportFormat::Svg,
        ExportFormat::Tree,
        ExportFormat::Callflow,
    ] {
        let bundle = graph_exports::generate(
            &engine.store,
            &s,
            &g,
            &ExportOptions::new(format),
            &Deadline::new(10000),
        )
        .unwrap();
        let content = &bundle.files[0].content;
        assert!(!content.contains("<img src=x"));
        assert!(!content.contains("href=\"javascript:"));
        assert!(content.contains("not runtime or compiler proof"));
        assert!(content.contains("&lt;/script&gt;") || content.contains("\\u003c/script\\u003e"));
        if format == ExportFormat::Svg {
            assert!(content.contains("marker-end"));
        } else {
            assert!(content.contains("connect-src 'none'"));
            assert!(!content.contains("https://cdn"));
        }
    }
}
#[test]
fn markdown_cards_have_collision_safe_names_and_preserve_human_notes() {
    let (_dir, engine, s, g) = fixture();
    for format in [ExportFormat::Wiki, ExportFormat::Obsidian] {
        let bundle = graph_exports::generate(
            &engine.store,
            &s,
            &g,
            &ExportOptions::new(format),
            &Deadline::new(10000),
        )
        .unwrap();
        let paths: std::collections::BTreeSet<_> =
            bundle.files.iter().map(|f| f.path.clone()).collect();
        assert_eq!(paths.len(), bundle.files.len());
        assert!(
            bundle
                .files
                .iter()
                .any(|f| f.content.contains("Human notes"))
        );
        assert!(
            bundle
                .files
                .iter()
                .all(|f| !f.path.contains("..") && !f.path.contains(':'))
        );
        let output = TempDir::new().unwrap();
        let destination = output.path().join("cards");
        graph_exports::write_new_bundle(&bundle, &destination, &Deadline::new(10000)).unwrap();
        let card = bundle
            .files
            .iter()
            .find(|f| f.path.ends_with(".md") && f.path != "index.md")
            .unwrap();
        let card_path = destination.join(&card.path);
        fs::write(&card_path, "My human notes\n").unwrap();
        assert!(
            graph_exports::write_new_bundle(&bundle, &destination, &Deadline::new(10000)).is_err()
        );
        assert_eq!(fs::read_to_string(&card_path).unwrap(), "My human notes\n");
        assert_eq!(identity(&engine.store.graph(&s.id).unwrap()), identity(&g));
    }
}
#[test]
fn bounded_projection_reports_omissions_and_publishes_exact_bundle() {
    let (_dir, engine, s, g) = fixture();
    let options = ExportOptions {
        format: ExportFormat::Canvas,
        max_nodes: 1,
        max_edges: 1,
    };
    let bundle =
        graph_exports::generate(&engine.store, &s, &g, &options, &Deadline::new(10000)).unwrap();
    assert!(bundle.omitted_nodes > 0);
    assert!(bundle.omitted_observations > 0);
    assert!(!bundle.complete);
    let saved: serde_json::Value =
        serde_json::from_slice(&engine.store.read_artifact(&bundle.artifact_id).unwrap()).unwrap();
    assert_eq!(saved["snapshotId"], s.id);
    assert_eq!(saved["graphHash"], identity(&g));
    let output = TempDir::new().unwrap();
    assert!(
        graph_exports::write_new_bundle(&bundle, &output.path().join("expired"), &Deadline::new(0))
            .is_err()
    );
    assert!(!output.path().join("expired").exists());
}
#[test]
fn export_rejects_forged_graph_invalid_limits_and_expired_deadline() {
    let (_dir, engine, s, g) = fixture();
    let mut forged = g.clone();
    forged.observations[0].target = None;
    assert!(
        graph_exports::generate(
            &engine.store,
            &s,
            &forged,
            &ExportOptions::new(ExportFormat::Svg),
            &Deadline::new(10000)
        )
        .is_err()
    );
    assert!(
        graph_exports::generate(
            &engine.store,
            &s,
            &g,
            &ExportOptions::new(ExportFormat::Svg),
            &Deadline::new(0)
        )
        .is_err()
    );
    let options = ExportOptions {
        format: ExportFormat::Svg,
        max_nodes: 0,
        max_edges: 1,
    };
    assert!(
        graph_exports::generate(&engine.store, &s, &g, &options, &Deadline::new(10000)).is_err()
    );
}

#[test]
fn standalone_exports_include_verified_hashes_and_neutralize_canvas_markdown() {
    let (_dir, engine, s, g) = fixture();
    let bundle = graph_exports::generate(
        &engine.store,
        &s,
        &g,
        &ExportOptions::new(ExportFormat::Canvas),
        &Deadline::new(10000),
    )
    .unwrap();
    let value: serde_json::Value = serde_json::from_str(&bundle.files[0].content).unwrap();
    let bindings = value["yoke"]["sourceBindings"]
        .as_array()
        .expect("Standalone source bindings required");
    assert_eq!(bindings.len(), g.entities.len());
    for binding in bindings {
        let file = s.files.iter().find(|f| f.path == binding["path"]).unwrap();
        assert_eq!(binding["contentHash"], file.hash);
    }
    let hostile = value["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["text"].as_str().unwrap().contains("javascript:"))
        .unwrap();
    assert!(
        !hostile["text"]
            .as_str()
            .unwrap()
            .contains("[x](javascript:")
    );
    assert!(
        hostile["text"]
            .as_str()
            .unwrap()
            .contains("\\[x\\]\\(javascript:")
    );
    let cypher = graph_exports::generate(
        &engine.store,
        &s,
        &g,
        &ExportOptions::new(ExportFormat::Cypher),
        &Deadline::new(10000),
    )
    .unwrap();
    for file in &s.files {
        assert!(cypher.files[0].content.contains(&file.hash));
    }
    assert!(cypher.files[0].content.contains("evidenceAnchorsJson"));
}

#[test]
fn svg_replaces_xml_forbidden_display_controls_but_keeps_source_hash() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("notes.md"), "# Hello\u{1} World\n").unwrap();
    let mut engine = Engine::open(dir.path()).unwrap();
    let (s, g, _, _) = engine.index(&Deadline::new(10000)).unwrap();
    assert!(g.entities.iter().any(|n| n.name.contains('\u{1}')));
    let bundle = graph_exports::generate(
        &engine.store,
        &s,
        &g,
        &ExportOptions::new(ExportFormat::Svg),
        &Deadline::new(10000),
    )
    .unwrap();
    assert!(!bundle.files[0].content.contains('\u{1}'));
    assert!(bundle.files[0].content.contains('\u{fffd}'));
    assert!(bundle.files[0].content.contains(&s.files[0].hash));
    assert_eq!(
        fs::read(dir.path().join("notes.md")).unwrap(),
        b"# Hello\x01 World\n"
    );
}

#[test]
fn tampered_export_bundle_cannot_write_any_destination_bytes() {
    let (_dir, engine, s, g) = fixture();
    let mut bundle = graph_exports::generate(
        &engine.store,
        &s,
        &g,
        &ExportOptions::new(ExportFormat::Wiki),
        &Deadline::new(10000),
    )
    .unwrap();
    let output = TempDir::new().unwrap();
    let destination = output.path().join("result");
    bundle.files[0].path = "../outside.md".into();
    assert!(graph_exports::write_new_bundle(&bundle, &destination, &Deadline::new(10000)).is_err());
    assert!(!destination.exists());
    assert!(!output.path().join("outside.md").exists());
}

#[test]
fn wiki_contains_actual_community_articles_and_escaped_source_links() {
    let (_dir, engine, s, g) = fixture();
    let bundle = graph_exports::generate(
        &engine.store,
        &s,
        &g,
        &ExportOptions::new(ExportFormat::Wiki),
        &Deadline::new(10000),
    )
    .unwrap();
    let communities: Vec<_> = bundle
        .files
        .iter()
        .filter(|f| f.path.starts_with("communities/"))
        .collect();
    assert!(
        !communities.is_empty(),
        "Wiki needs community articles as well as symbol cards"
    );
    assert!(
        communities
            .iter()
            .all(|f| f.content.contains("Leiden") && f.content.contains("Human notes"))
    );
    assert!(bundle.files[0].content.contains("communities/"));
}
