use yoke::{
    export,
    model::{Entity, Graph, Observation},
};
fn fixture() -> Graph {
    let hostile = "</script><script>globalThis.INJECTED=1</script>&\"'\u{2028}\u{2029}";
    let entity = Entity {
        id: "node\" & < hostile".into(),
        key: "sample:file:0".into(),
        snapshot_id: "a".repeat(64),
        path: "sample.rs".into(),
        name: hostile.into(),
        qualified_name: hostile.into(),
        kind: "file".into(),
        language: "rust".into(),
        signature: hostile.into(),
        start_byte: 0,
        end_byte: 1,
        start_line: 1,
        end_line: 1,
        name_start_byte: None,
        name_end_byte: None,
        parent_key: None,
        producer: "fixture".into(),
    };
    Graph {
        snapshot_id: "a".repeat(64),
        entities: vec![entity.clone()],
        observations: vec![Observation {
            id: "edge\" & < hostile".into(),
            snapshot_id: "a".repeat(64),
            source: entity.id.clone(),
            target: Some(entity.id),
            target_label: hostile.into(),
            relation: "calls".into(),
            resolution: "syntactic".into(),
            evidence_ids: vec![],
            producer: "fixture".into(),
        }],
        ..Default::default()
    }
}
#[test]
fn offline_html_preserves_canonical_graph_and_inert_hostile_labels() {
    let graph = fixture();
    let before = serde_json::to_value(&graph).unwrap();
    let html = String::from_utf8(export::serialize(&graph, "html").unwrap()).unwrap();
    let marker = "<script type=\"application/json\" id=\"data\">";
    let embedded = html
        .split_once(marker)
        .unwrap()
        .1
        .split_once("</script>")
        .unwrap()
        .0;
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(embedded).unwrap(),
        before
    );
    assert!(!html.contains("<script>globalThis.INJECTED"));
    assert!(html.contains("connect-src 'none'"));
    assert_eq!(serde_json::to_value(&graph).unwrap(), before);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&export::serialize(&graph, "json").unwrap())
            .unwrap(),
        before
    );
}
#[test]
fn graphml_dynamic_attributes_are_xml_values_not_markup() {
    let graph = fixture();
    let xml = String::from_utf8(export::serialize(&graph, "graphml").unwrap()).unwrap();
    let mut reader = quick_xml::Reader::from_str(&xml);
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    loop {
        match reader.read_event().unwrap() {
            quick_xml::events::Event::Start(e) if e.name().as_ref() == "node" => {
                for attr in e.attributes() {
                    let attr = attr.unwrap();
                    if attr.key.as_ref() == "id" {
                        nodes.push(
                            attr.normalized_value(quick_xml::XmlVersion::Implicit1_0)
                                .unwrap()
                                .into_owned(),
                        );
                    }
                }
            }
            quick_xml::events::Event::Start(e) if e.name().as_ref() == "edge" => {
                for attr in e.attributes() {
                    let attr = attr.unwrap();
                    if attr.key.as_ref() == "source" {
                        edges.push(
                            attr.normalized_value(quick_xml::XmlVersion::Implicit1_0)
                                .unwrap()
                                .into_owned(),
                        );
                    }
                }
            }
            quick_xml::events::Event::Eof => break,
            _ => {}
        }
    }
    assert_eq!(nodes, [graph.entities[0].id.clone()]);
    assert_eq!(edges, [graph.entities[0].id.clone()]);
}

#[test]
fn community_metadata_requires_exact_canonical_source_and_preserves_graph() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("main.rs"),
        "fn helper(){} fn main(){helper();helper();}\n",
    )
    .unwrap();
    let mut engine = yoke::Engine::open(root.path()).unwrap();
    let deadline = yoke::capture::Deadline::new(10000);
    let (snapshot, graph, _, _) = engine.index(&deadline).unwrap();
    let html = String::from_utf8(
        export::serialize_explorer(&engine.store, &snapshot, &graph, &deadline).unwrap(),
    )
    .unwrap();
    let marker = "<script type=\"application/json\" id=\"communities\">";
    let metadata: serde_json::Value = serde_json::from_str(
        html.split_once(marker)
            .unwrap()
            .1
            .split_once("</script>")
            .unwrap()
            .0,
    )
    .unwrap();
    assert_eq!(metadata["snapshotId"], snapshot.id);
    assert_eq!(metadata["graphHash"], yoke::model::identity(&graph));
    assert!(metadata["communities"].is_array());
    let marker = "<script type=\"application/json\" id=\"data\">";
    let embedded: serde_json::Value = serde_json::from_str(
        html.split_once(marker)
            .unwrap()
            .1
            .split_once("</script>")
            .unwrap()
            .0,
    )
    .unwrap();
    assert_eq!(embedded, serde_json::to_value(&graph).unwrap());
    let mut tampered = graph.clone();
    tampered.entities[0].signature = "foreign metadata".into();
    assert!(export::serialize_explorer(&engine.store, &snapshot, &tampered, &deadline).is_err());
}

#[test]
fn template_markers_inside_source_metadata_are_preserved_verbatim() {
    let mut graph = fixture();
    graph.entities[0].signature = "__YOKE_COMMUNITY_DATA__ __YOKE_GRAPH_DATA__".into();
    let html = String::from_utf8(export::serialize(&graph, "html").unwrap()).unwrap();
    let marker = "<script type=\"application/json\" id=\"data\">";
    let embedded: serde_json::Value = serde_json::from_str(
        html.split_once(marker)
            .unwrap()
            .1
            .split_once("</script>")
            .unwrap()
            .0,
    )
    .unwrap();
    assert_eq!(embedded, serde_json::to_value(&graph).unwrap());
}

#[test]
fn source_bound_html_writer_is_create_only_and_idempotent() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("main.rs"), "fn actual(){}\n").unwrap();
    let mut engine = yoke::Engine::open(root.path()).unwrap();
    let deadline = yoke::capture::Deadline::new(10000);
    let (snapshot, graph, _, _) = engine.index(&deadline).unwrap();
    let output = root.path().join("exports/graph.html");
    export::write_explorer(&engine.store, &snapshot, &graph, &output, &deadline).unwrap();
    let before = std::fs::read(&output).unwrap();
    export::write_explorer(&engine.store, &snapshot, &graph, &output, &deadline).unwrap();
    assert_eq!(std::fs::read(&output).unwrap(), before);
    std::fs::write(&output, b"existing host output").unwrap();
    assert!(export::write_explorer(&engine.store, &snapshot, &graph, &output, &deadline).is_err());
    assert_eq!(std::fs::read(&output).unwrap(), b"existing host output");
}

#[test]
fn native_markdown_metadata_unrepresentable_in_xml_is_refused_without_changing_graph() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("notes.md"), "# Captured \u{1} heading\n").unwrap();
    let mut engine = yoke::Engine::open(root.path()).unwrap();
    let (snapshot, graph, _, _) = engine.index(&yoke::capture::Deadline::new(10000)).unwrap();
    assert_eq!(snapshot.files.len(), 1);
    assert!(
        graph
            .entities
            .iter()
            .any(|e| e.qualified_name.contains('\u{1}')),
        "Fixture must reproduce a real native metadata value"
    );
    let before = serde_json::to_value(&graph).unwrap();
    assert!(
        export::serialize(&graph, "graphml").is_err(),
        "GraphML silently published forbidden XML control bytes"
    );
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&export::serialize(&graph, "json").unwrap())
            .unwrap(),
        before
    );
    assert!(export::serialize(&graph, "html").is_ok());
    assert_eq!(serde_json::to_value(&graph).unwrap(), before);
}
