use serde_json::json;
use std::fs;
use tempfile::TempDir;
use yoke::{Engine, capture::Deadline};
#[test]
fn composite_api_headers_keep_parent_offsets_without_script_bodies() {
    let root = TempDir::new().unwrap();
    let source = "<template>🌍</template>\n<script setup lang='ts'>\nfunction embedded(\n value: string\n): string { return 'BODY_PRIVATE_MARKER'; }\n</script>\n";
    fs::write(root.path().join("App.vue"), source).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let d = Deadline::new(20000);
    let (s, g, _, _) = engine.index(&d).unwrap();
    let result =
        yoke::file_api::inspect(&engine.store, &s, &g, &json!({"file":"App.vue"}), &d).unwrap();
    let item = &result["items"][0];
    assert_eq!(item["signatureAvailable"], true);
    let a = item["signatureStartByte"].as_u64().unwrap() as usize;
    let b = item["signatureEndByte"].as_u64().unwrap() as usize;
    assert_eq!(item["signature"], source[a..b]);
    assert!(
        item["signature"]
            .as_str()
            .unwrap()
            .contains("value: string")
    );
    assert!(!result.to_string().contains("BODY_PRIVATE_MARKER"));
}
#[test]
fn exact_multiline_api_headers_exclude_bodies_and_preserve_source_byte_bindings() {
    let root = TempDir::new().unwrap();
    let source = "pub fn alpha(\n    input: &str,\n    count: usize,\n) -> String {\n    let SECRET_BODY_MARKER = input;\n    SECRET_BODY_MARKER.to_owned()\n}\nfn beta() {}\n";
    fs::write(root.path().join("main.rs"), source).unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let d = Deadline::new(20000);
    let (s, g, _, _) = engine.index(&d).unwrap();
    let result = yoke::file_api::inspect(
        &engine.store,
        &s,
        &g,
        &json!({"file":"main.rs","limit":1}),
        &d,
    )
    .unwrap();
    assert_eq!(result["total"], 2);
    assert_eq!(result["truncated"], true);
    let row = &result["items"][0];
    assert!(row["signature"].as_str().unwrap().contains("count: usize"));
    assert!(!result.to_string().contains("SECRET_BODY_MARKER"));
    let a = row["signatureStartByte"].as_u64().unwrap() as usize;
    let b = row["signatureEndByte"].as_u64().unwrap() as usize;
    assert_eq!(row["signature"], source[a..b]);
    assert_eq!(row["contentHash"], s.files[0].hash);
}
#[test]
fn basename_resolution_is_unique_and_paging_remains_exact_file_scoped() {
    let root = TempDir::new().unwrap();
    fs::create_dir_all(root.path().join("other")).unwrap();
    fs::write(root.path().join("main.rs"), "fn a(){}\nfn b(){}\n").unwrap();
    fs::write(root.path().join("other/main.rs"), "fn c(){}\n").unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    let d = Deadline::new(20000);
    let (s, g, _, _) = engine.index(&d).unwrap();
    assert!(yoke::file_api::inspect(&engine.store, &s, &g, &json!({"file":"main.rs"}), &d).is_ok());
    assert!(
        yoke::file_api::inspect(&engine.store, &s, &g, &json!({"file":"../main.rs"}), &d).is_err()
    );
    let result = yoke::file_api::inspect(
        &engine.store,
        &s,
        &g,
        &json!({"file":"main.rs","limit":1,"offset":1}),
        &d,
    )
    .unwrap();
    assert_eq!(result["items"][0]["name"], "b");
    assert!(
        yoke::file_api::inspect(
            &engine.store,
            &s,
            &g,
            &json!({"file":"main.rs","root":"C:/"}),
            &d
        )
        .is_err()
    );
}
