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
        2
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
