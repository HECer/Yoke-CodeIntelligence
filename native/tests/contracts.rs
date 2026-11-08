use std::{collections::BTreeMap, fs};
use tempfile::TempDir;
use yoke::{
    Engine,
    capture::Deadline,
    model::{Entity, Graph, Snapshot},
};

fn indexed(files: &[(&str, &str)]) -> (TempDir, Snapshot, Graph, BTreeMap<String, String>) {
    let dir = TempDir::new().unwrap();
    let mut contents = BTreeMap::new();
    for (path, text) in files {
        let target = dir.path().join(path);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(target, text).unwrap();
        contents.insert((*path).into(), (*text).into());
    }
    let mut engine = Engine::open(dir.path()).unwrap();
    let (snapshot, graph, _, _) = engine.index(&Deadline::new(120000)).unwrap();
    (dir, snapshot, graph, contents)
}
fn entity<'a>(graph: &'a Graph, path: &str, kind: &str, name: &str) -> &'a Entity {
    graph
        .entities
        .iter()
        .find(|e| e.path == path && e.kind == kind && e.name == name)
        .unwrap_or_else(|| {
            panic!(
                "Missing {path} {kind} {name}: {:?}",
                graph
                    .entities
                    .iter()
                    .map(|e| (&e.path, &e.kind, &e.name))
                    .collect::<Vec<_>>()
            )
        })
}

#[test]
fn sql_comments_and_string_data_cannot_fabricate_migrations_or_accesses() {
    let sql = "-- CREATE TABLE ghost (id INT);\nSELECT 'CREATE TABLE fake (x int); FROM hidden';\nSELECT $$CREATE TABLE dollar_ghost (x int)$$;\n/* UPDATE hidden; */\nCREATE TABLE \"café\" (id INT);\nINSERT INTO \"café\" VALUES (1);\nALTER TABLE \"café\" ADD COLUMN label TEXT;\n";
    let (_dir, snapshot, graph, contents) = indexed(&[("migration.sql", sql)]);
    let table = entity(&graph, "migration.sql", "migration_table", "café");
    assert_eq!(
        &sql[table.name_start_byte.unwrap()..table.name_end_byte.unwrap()],
        "\"café\""
    );
    assert!(
        !graph
            .entities
            .iter()
            .any(|e| ["ghost", "fake", "hidden", "dollar_ghost"].contains(&e.name.as_str()))
    );
    let accesses: Vec<_> = graph
        .observations
        .iter()
        .filter(|o| o.relation == "possibly_accesses_table")
        .collect();
    assert_eq!(accesses.len(), 2);
    for edge in accesses {
        assert_eq!(edge.target.as_deref(), Some(table.id.as_str()));
        assert_eq!(edge.resolution, "heuristic");
        let evidence = graph
            .evidence
            .iter()
            .find(|e| e.id == edge.evidence_ids[0])
            .unwrap();
        assert_eq!(evidence.content_hash, snapshot.files[0].hash);
        let anchor = &contents[&evidence.path][evidence.start_byte..evidence.end_byte];
        assert!(anchor == "INSERT INTO \"café\"" || anchor == "ALTER TABLE \"café\"");
    }
}

#[test]
fn prisma_fields_and_mapped_models_link_to_migrations_as_candidates() {
    let (_dir, _, graph, _) = indexed(&[
        (
            "schema.prisma",
            "// model Ghost { id Int }\nmodel User {\n id Int @id\n displayName String? @map(\"display_name\")\n @@map(\"users\")\n}\n",
        ),
        (
            "migrations/001.sql",
            "CREATE TABLE users (id INT, display_name TEXT);",
        ),
        (
            "app.ts",
            "async function load() { return prisma.user.findMany(); }",
        ),
    ]);
    let model = entity(&graph, "schema.prisma", "orm_model", "User");
    let field = entity(&graph, "schema.prisma", "schema_field", "User.displayName");
    assert!(graph.observations.iter().any(|e| e.source == model.id
        && e.target.as_deref() == Some(&field.id)
        && e.relation == "has_field"
        && e.resolution == "syntactic"));
    assert!(graph.observations.iter().any(|e| e.source == model.id
        && e.relation == "possibly_implemented_by_migration"
        && e.target.is_some()
        && e.resolution == "heuristic"));
    let operation = entity(&graph, "app.ts", "orm_operation", "user");
    assert!(graph.observations.iter().any(|e| e.source == operation.id
        && e.target.as_deref() == Some(&model.id)
        && e.relation == "possibly_accesses_table"
        && e.resolution == "heuristic"));
    assert!(!graph.entities.iter().any(|e| e.name == "Ghost"));
}

#[test]
fn graphql_and_protobuf_ignore_comment_and_description_headers() {
    let (_dir, _, graph, _) = indexed(&[
        (
            "schema.graphql",
            "# type Ghost { nope: String }\n\"\"\"type Hidden { bad: ID }\"\"\"\ntype Query {\n user(id: ID!): User!\n}\ntype User { name: String! }\n",
        ),
        (
            "schema.proto",
            "syntax = \"proto3\";\n// message Ghost { }\nmessage User { string name = 1; }\nservice Directory { rpc Get (User) returns (User); }\n",
        ),
    ]);
    let query = entity(&graph, "schema.graphql", "schema_type", "Query");
    let field = entity(&graph, "schema.graphql", "schema_field", "Query.user");
    assert!(graph.observations.iter().any(|e| e.source == query.id
        && e.target.as_deref() == Some(&field.id)
        && e.relation == "has_field"));
    entity(&graph, "schema.proto", "schema_type", "User");
    entity(&graph, "schema.proto", "schema_type", "Directory");
    assert!(
        !graph
            .entities
            .iter()
            .any(|e| ["Ghost", "Hidden", "bad", "nope"].contains(&e.name.as_str()))
    );
}

#[test]
fn openapi_operations_use_the_actual_nested_operation_key_after_unicode() {
    let source = "{\"openapi\":\"3.1.0\",\"description\":\"🌍 /café\",\"examples\":{\"/café\":{\"get\":\"fake\"}},\"paths\":{\"/café\":{\"get\":{\"operationId\":\"readCafe\"},\"post\":{\"operationId\":\"writeCafe\"}}}}";
    let (_dir, _, graph, _) = indexed(&[
        ("api.json", source),
        (
            "client.ts",
            "fetch('/café'); fetch('/café', {method: 'POST'});",
        ),
    ]);
    for (method, operation) in [("GET", "readCafe"), ("POST", "writeCafe")] {
        let endpoint = entity(
            &graph,
            "api.json",
            "openapi_endpoint",
            &format!("{method} /café"),
        );
        let anchor = &source[endpoint.start_byte..endpoint.end_byte];
        assert!(
            anchor.starts_with(&format!("\"{}\":", method.to_lowercase())),
            "{anchor}"
        );
        assert!(anchor.contains(operation));
        assert_eq!(
            graph
                .observations
                .iter()
                .filter(|e| e.target.as_deref() == Some(&endpoint.id)
                    && e.relation == "possibly_consumes_contract"
                    && e.resolution == "heuristic")
                .count(),
            1
        );
    }
}

#[test]
fn literal_ast_contracts_ignore_comments_strings_and_dynamic_arguments() {
    let source = "// router.get('/ghost', handler); process.env.GHOST; bus.emit('ghost');\nconst text = \"fetch('/fake'); getenv('FAKE'); bus.on('fake')\";\nfunction run(id: string) {\n router.get('/café', handler);\n bus.emit('order.created', id); bus.on('order.created', handler);\n const name = process.env.API_URL;\n fetch(`/dynamic/${id}`);\n}\n";
    let (_dir, _, graph, _) = indexed(&[("app.ts", source)]);
    entity(&graph, "app.ts", "route", "GET /café");
    let env = entity(&graph, "app.ts", "configuration_key", "API_URL");
    assert_eq!(&source[env.start_byte..env.end_byte], "process.env.API_URL");
    assert!(!graph.entities.iter().any(|e| {
        ["GHOST", "FAKE", "ghost", "fake"].contains(&e.name.as_str())
            || e.kind == "http_client_call"
    }));
    assert!(
        graph
            .observations
            .iter()
            .any(|e| e.relation == "possible_event_delivery"
                && e.resolution == "heuristic"
                && e.target.is_some())
    );
}

#[test]
fn duplicate_and_cross_service_contract_names_remain_unresolved() {
    let api = "{\"openapi\":\"3.1.0\",\"paths\":{\"/users\":{\"get\":{}}}}";
    let (_dir, _, graph, _) = indexed(&[
        ("services/a/client.ts", "fetch('/users');"),
        ("services/a/api.json", api),
        ("services/a/duplicate.json", api),
        ("services/b/client.ts", "fetch('/users');"),
        ("services/c/api.json", api),
    ]);
    let candidates: Vec<_> = graph
        .observations
        .iter()
        .filter(|e| e.relation == "possibly_consumes_contract")
        .collect();
    assert_eq!(candidates.len(), 2);
    assert!(
        candidates
            .iter()
            .all(|e| e.target.is_none() && e.resolution == "unresolved")
    );
}

#[test]
fn malformed_contract_documents_produce_diagnostics_instead_of_empty_success() {
    let (_dir, _, graph, _) = indexed(&[
        ("openapi.json", "{\"openapi\":\"3.1.0\",\"paths\": {"),
        ("bad.prisma", "model User {\n id Int\n"),
    ]);
    for path in ["openapi.json", "bad.prisma"] {
        assert!(
            graph
                .diagnostics
                .iter()
                .any(|d| d.path == path && d.severity == "error"),
            "{:?}",
            graph.diagnostics
        );
    }
}

#[test]
fn quoted_sql_identifiers_are_not_keywords_and_json_duplicates_are_rejected() {
    let (_dir, _, graph, _) = indexed(&[
        (
            "query.sql",
            "SELECT \"FROM\" \"ghost_alias\"; CREATE TABLE \"CREATE\" (id INT); UPDATE \"CREATE\" SET id = 1;",
        ),
        (
            "api.json",
            "{\"openapi\":\"3.1.0\",\"paths\":{\"/a\":{\"get\":{},\"get\":{}}}}",
        ),
    ]);
    assert!(
        !graph
            .entities
            .iter()
            .any(|e| e.kind == "sql_operation" && e.name == "ghost_alias")
    );
    entity(&graph, "query.sql", "migration_table", "CREATE");
    assert!(
        graph
            .diagnostics
            .iter()
            .any(|d| d.path == "api.json" && d.severity == "error")
    );
    assert!(!graph.entities.iter().any(|e| e.kind == "openapi_endpoint"));
}

#[test]
fn a_root_manifest_does_not_merge_independent_service_scopes() {
    let (_dir, _, graph, _) = indexed(&[
        ("package.json", "{\"private\":true}"),
        ("services/a/client.ts", "fetch('/only-b');"),
        (
            "services/b/api.json",
            "{\"openapi\":\"3.1.0\",\"paths\":{\"/only-b\":{\"get\":{}}}}",
        ),
    ]);
    let edge = graph
        .observations
        .iter()
        .find(|e| e.relation == "possibly_consumes_contract")
        .unwrap();
    assert!(edge.target.is_none());
    assert_eq!(edge.resolution, "unresolved");
}

#[test]
fn schema_body_braces_may_follow_the_name_on_another_line() {
    let (_dir, _, graph, _) = indexed(&[("schema.prisma", "model User\n{\n id Int\n}\n")]);
    entity(&graph, "schema.prisma", "orm_model", "User");
    entity(&graph, "schema.prisma", "schema_field", "User.id");
    assert!(graph.diagnostics.is_empty(), "{:?}", graph.diagnostics);
}

#[test]
fn python_routes_and_rust_environment_calls_are_source_bound_without_execution() {
    let (_dir, _, graph, contents) = indexed(&[
        (
            "app.py",
            "# app.get('/ghost')\n@app.get('/users')\ndef users():\n return os.getenv('API_URL')\n",
        ),
        (
            "app.rs",
            "fn configure() { let _ = std::env::var(\"APP_MODE\"); }\n",
        ),
    ]);
    entity(&graph, "app.py", "route", "GET /users");
    for (path, name, anchor) in [
        ("app.py", "API_URL", "os.getenv('API_URL')"),
        ("app.rs", "APP_MODE", "std::env::var(\"APP_MODE\")"),
    ] {
        let key = entity(&graph, path, "configuration_key", name);
        assert_eq!(&contents[path][key.start_byte..key.end_byte], anchor);
        assert_eq!(
            key.signature,
            "Environment variable name only; no value stored."
        );
    }
}

#[test]
fn typescript_jsonc_config_is_valid_but_openapi_json_remains_strict() {
    let (_dir, _, graph, _) = indexed(&[
        (
            "tsconfig.json",
            "{/* compiler configuration */\n\"compilerOptions\":{\"strict\":true,},\n}\n",
        ),
        (
            "openapi.json",
            "{/* invalid for strict OpenAPI JSON */\"openapi\":\"3.1.0\",\"paths\":{}}",
        ),
    ]);
    assert!(
        !graph.diagnostics.iter().any(|d| d.path == "tsconfig.json"),
        "{:?}",
        graph.diagnostics
    );
    assert!(
        graph
            .diagnostics
            .iter()
            .any(|d| d.path == "openapi.json" && d.severity == "error")
    );
}
