use std::fs;
use tempfile::TempDir;
use yoke::{Engine, capture::Deadline, markdown_graph};
#[test]
fn commonmark_nested_labels_multiline_definitions_and_indented_code_have_correct_links() {
    let d = TempDir::new().unwrap();
    fs::write(d.path().join("guide.md"),"[nested **label**](target.md)\n[ref][definition]\n\n[definition]:\n  target.md \"Title\"\n[Unicode](caf%C3%A9.md)\n\n    [[fake]]\n").unwrap();
    fs::write(d.path().join("target.md"), "# Target\n").unwrap();
    fs::write(d.path().join("café.md"), "# Café\n").unwrap();
    let mut e = Engine::open(d.path()).unwrap();
    let (s, g, _, _) = e.index(&Deadline::new(10000)).unwrap();
    let out = markdown_graph::extract(&e.store, &s, &g, &Deadline::new(10000)).unwrap();
    assert_eq!(
        out.observations
            .iter()
            .filter(|o| o["targetLabel"] == "target.md" && o["relation"] == "links_document")
            .count(),
        3
    );
    assert!(
        out.observations
            .iter()
            .any(|o| o["targetLabel"] == "café.md" && o["target"].is_string())
    );
    assert!(
        !out.observations
            .iter()
            .any(|o| o["targetLabel"] == "fake.md")
    );
}

#[test]
fn frontmatter_links_preserve_captured_targets_and_exact_byte_ranges() {
    let root = TempDir::new().unwrap();
    fs::create_dir(root.path().join("docs")).unwrap();
    fs::write(root.path().join("related.md"), "# Related\n").unwrap();
    let markdown = "---\r\ntitle: Übersicht 🦀\r\nnotes:\r\n    - '[[related#Intro]]'\r\n    - '[contact](https://example.invalid/info)'\r\n# [[commented]]\r\n---\r\n\r\n# Body\r\n";
    fs::write(root.path().join("docs/guide.md"), markdown).unwrap();
    let deadline = Deadline::new(30000);
    let mut engine = Engine::open(root.path()).unwrap();
    let (snapshot, graph, _, _) = engine.index(&deadline).unwrap();
    let out = markdown_graph::extract(&engine.store, &snapshot, &graph, &deadline).unwrap();
    let observations = [
        ("related.md", "links_document", "[[related#Intro]]"),
        (
            "https://example.invalid/info",
            "links_external",
            "[contact](https://example.invalid/info)",
        ),
    ];
    for (target, relation, authored) in observations {
        let edge = out
            .observations
            .iter()
            .find(|edge| edge["targetLabel"] == target && edge["relation"] == relation)
            .unwrap();
        assert_eq!(edge["target"].is_string(), relation == "links_document");
        let source = out
            .evidence
            .iter()
            .find(|source| source["id"] == edge["evidenceIds"][0])
            .unwrap();
        let start = source["startByte"].as_u64().unwrap() as usize;
        let end = source["endByte"].as_u64().unwrap() as usize;
        assert_eq!(&markdown[start..end], authored);
    }
    assert!(
        out.observations
            .iter()
            .all(|edge| edge["targetLabel"] != "docs/commented.md")
    );
    assert!(
        out.nodes
            .iter()
            .any(|node| node["kind"] == "frontmatter_key" && node["name"] == "notes")
    );
}

#[test]
fn headings_keep_exact_ranges_and_citations_keep_their_section_and_owner() {
    let root = TempDir::new().unwrap();
    let code = "struct Widget;\nimpl Widget { fn render(&self) {} }\nfn startup() {}\n";
    fs::write(root.path().join("main.rs"), code).unwrap();
    fs::write(
        root.path().join("other.rs"),
        "struct Other;\nimpl Other { fn render(&self) {} }\n",
    )
    .unwrap();
    let markdown = "Before: \x60startup()\x60\r\n\r\n# Übersicht 🦀\r\n\r\n## \x60Widget.render\x60\r\n\r\n\x60main.rs::Widget::render()\x60 and \x60render\x60 and \x60time.render\x60.\r\n\r\n## Repeated\r\n\r\n## Repeated\r\n\r\n~~~md\r\n# Fake\r\n\x60startup()\x60\r\n~~~\r\n\r\nLast Setext\r\n===========\r\n";
    fs::write(root.path().join("guide.md"), markdown).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let deadline = Deadline::new(30000);
    let (snapshot, graph, _, _) = engine.index(&deadline).unwrap();
    let view = yoke::graph_view::build(
        &engine.store,
        &snapshot,
        &graph,
        &yoke::graph_view::BuildOptions {
            structural_kinds: vec![yoke::graph_view::StructuralKind::Markdown],
            ..yoke::graph_view::BuildOptions::default()
        },
        &deadline,
    )
    .unwrap();
    let heading = |name: &str| {
        view.nodes
            .iter()
            .find(|node| node["kind"] == "heading" && node["name"] == name)
            .unwrap()
    };
    let top = heading("Übersicht 🦀");
    let section = heading("Widget.render");
    let binding = &top["sourceBindings"][0];
    let start = binding["startByte"].as_u64().unwrap() as usize;
    let end = binding["endByte"].as_u64().unwrap() as usize;
    assert_eq!(&markdown[start..end], "# Übersicht 🦀");
    assert!(
        view.observations
            .iter()
            .any(|edge| edge["relation"] == "contains"
                && edge["source"] == top["id"]
                && edge["target"] == section["id"])
    );
    let cites: Vec<_> = view
        .observations
        .iter()
        .filter(|edge| edge["relation"] == "cites_symbol")
        .collect();
    let first = cites
        .iter()
        .find(|edge| edge["targetLabel"] == "startup()")
        .unwrap();
    assert!(
        view.nodes
            .iter()
            .any(|node| node["id"] == first["source"] && node["kind"] == "document")
    );
    assert_eq!(
        cites
            .iter()
            .filter(|edge| edge["targetLabel"] == "startup()")
            .count(),
        1
    );
    let render = graph
        .entities
        .iter()
        .find(|node| node.path == "main.rs" && node.name == "render")
        .unwrap();
    for spelling in ["Widget.render", "main.rs::Widget::render()"] {
        let edge = cites
            .iter()
            .find(|edge| edge["targetLabel"] == spelling)
            .unwrap();
        assert_eq!(edge["source"], section["id"], "{edge}");
        assert_eq!(edge["target"], render.id, "{edge}");
    }
    let ambiguous = cites
        .iter()
        .find(|edge| edge["targetLabel"] == "render")
        .unwrap();
    assert_eq!(ambiguous["resolution"], "ambiguous_name");
    assert!(ambiguous["target"].is_null());
    assert!(
        cites
            .iter()
            .all(|edge| edge["targetLabel"] != "time.render")
    );
    let same: Vec<_> = view
        .nodes
        .iter()
        .filter(|node| node["kind"] == "heading" && node["name"] == "Repeated")
        .collect();
    assert_eq!(same.len(), 2);
    assert_ne!(same[0]["id"], same[1]["id"]);
    assert!(
        view.nodes
            .iter()
            .all(|node| node["kind"] != "heading" || node["name"] != "Fake")
    );
    heading("Last Setext");
    let loaded = yoke::graph_view::load(&engine.store, &view.id, &deadline).unwrap();
    assert_eq!(loaded.id, view.id);
    assert_eq!(loaded.observations, view.observations);
}

#[test]
fn vault_links_only_search_captured_paths_and_relative_citations_cannot_fall_through() {
    let root = TempDir::new().unwrap();
    for dir in ["docs", "wiki", "wiki/nested", "pkg", "pkg/src", "outside"] {
        fs::create_dir_all(root.path().join(dir)).unwrap();
    }
    for path in [
        "hub.md",
        "wiki/hub.md",
        "wiki/nested/cafe\u{301}.md",
        "wiki/read.mdx",
        "wiki/100%.md",
    ] {
        fs::write(root.path().join(path), "target\n").unwrap();
    }
    fs::write(root.path().join("pkg/src/example.rs"), "fn verify() {}\n").unwrap();
    fs::write(root.path().join(".gitignore"), "outside/\n").unwrap();
    fs::write(root.path().join("outside/secret.md"), "uncaptured\n").unwrap();
    let text = "[[hub]] [[nested/café#Intro]] [[wiki/read.mdx]] [[100%]] [[secret]] [[../../hub]]\n\n[no fallback](hub.md)\n\n[unused]: ../wiki/read.mdx\n\n\x60src/example.rs::verify\x60 \x60./src/example.rs::verify\x60\n";
    fs::write(root.path().join("docs/page.md"), text).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let deadline = Deadline::new(30000);
    let (snapshot, graph, _, _) = engine.index(&deadline).unwrap();
    let out = markdown_graph::extract(&engine.store, &snapshot, &graph, &deadline).unwrap();
    let links: Vec<_> = out
        .observations
        .iter()
        .filter(|edge| edge["relation"] == "links_document")
        .collect();
    for path in [
        "hub.md",
        "wiki/nested/cafe\u{301}.md",
        "wiki/read.mdx",
        "wiki/100%.md",
    ] {
        assert!(
            links
                .iter()
                .any(|edge| edge["targetLabel"] == path && edge["target"].is_string()),
            "{path}"
        );
    }
    for path in ["docs/secret.md", "docs/hub.md"] {
        assert!(
            links
                .iter()
                .any(|edge| edge["targetLabel"] == path && edge["target"].is_null()),
            "{path}"
        );
    }
    assert!(
        out.observations
            .iter()
            .any(|edge| edge["relation"] == "links_external" && edge["targetLabel"] == "../../hub")
    );
    assert_eq!(
        links
            .iter()
            .filter(|edge| edge["targetLabel"] == "wiki/read.mdx" && edge["target"].is_string())
            .count(),
        2
    );
    assert!(
        out.observations
            .iter()
            .any(|edge| edge["targetLabel"] == "src/example.rs::verify"
                && edge["target"].is_string())
    );
    assert!(
        out.observations
            .iter()
            .any(|edge| edge["targetLabel"] == "./src/example.rs::verify"
                && edge["target"].is_null())
    );
    // A subsequent disk edit cannot change a link in the old capture.
    fs::write(root.path().join("docs/hub.md"), "new sibling\n").unwrap();
    let replay = markdown_graph::extract(&engine.store, &snapshot, &graph, &deadline).unwrap();
    assert_eq!(out.artifact_id, replay.artifact_id);
}
#[test]
fn markdown_wiki_reference_frontmatter_and_symbol_citations_are_source_bound() {
    let d = TempDir::new().unwrap();
    fs::write(d.path().join("guide.md"),"---\nowner: team\n---\n# Guide\n[[target|Related]]\n[reference][target]\n\n[target]: target.md#Details\n`main.rs::run`\n```md\n[[fake]]\n```\n").unwrap();
    fs::write(d.path().join("target.md"), "# Details\n").unwrap();
    fs::write(d.path().join("main.rs"), "fn run(){}\n").unwrap();
    let mut e = Engine::open(d.path()).unwrap();
    let (s, g, _, _) = e.index(&Deadline::new(10000)).unwrap();
    let out = markdown_graph::extract(&e.store, &s, &g, &Deadline::new(10000)).unwrap();
    assert!(
        out.nodes
            .iter()
            .any(|n| n["kind"] == "frontmatter_key" && n["name"] == "owner")
    );
    assert!(
        out.observations
            .iter()
            .filter(|o| o["relation"] == "links_document" && o["targetLabel"] == "target.md")
            .count()
            >= 2
    );
    assert!(
        out.observations
            .iter()
            .any(|o| o["relation"] == "cites_symbol"
                && o["targetLabel"] == "main.rs::run"
                && o["resolution"] == "unique_name_candidate")
    );
    assert!(
        !out.observations
            .iter()
            .any(|o| o["targetLabel"] == "fake.md")
    );
}
