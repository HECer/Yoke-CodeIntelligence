//! Conservative file usage proof fixtures, without proprietary IDE or actual compiler claims.
use serde_json::{Value, json};
use std::{fs, path::Path};
use yoke::{
    Engine,
    analysis::AnalyzerEpoch,
    capture::Deadline,
    changes, lsp,
    project_session::{HostProfile, Session},
};
const SOURCE: &str = "// owned café 🦀\r\nexport function unusedCafé(): number { return 42; }\r\n";
fn d() -> Deadline {
    Deadline::new(30_000)
}
struct Fixture {
    session: Session,
    log: tempfile::TempDir,
    root: tempfile::TempDir,
}
fn fixture(epoch: AnalyzerEpoch, mode: &str) -> Fixture {
    let root = tempfile::tempdir().unwrap();
    let log = tempfile::tempdir().unwrap();
    fs::write(root.path().join("spare_module.ts"), SOURCE).unwrap();
    fs::write(
        root.path().join("other.ts"),
        "export const untouched = 7;\r\n",
    )
    .unwrap();
    fs::write(
        root.path().join("tsconfig.json"),
        "{\"compilerOptions\":{\"strict\":true},\"include\":[\"*.ts\"]}",
    )
    .unwrap();
    let mut engine = Engine::open(root.path()).unwrap();
    engine.capture_epoch = epoch;
    engine
        .register_configured_server(lsp::ConfiguredServer {
            server: lsp::Server {
                command: "node".into(),
                args: vec![
                    Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("tests/fixtures/file-usage-lsp.mjs")
                        .display()
                        .to_string(),
                    log.path().join("protocol.log").display().to_string(),
                    mode.into(),
                ],
                language: "typescript".into(),
                trust: true,
            },
            configuration: lsp::HostConfiguration {
                passive_profile: Some(lsp::PassiveProfile::ContractFixture),
                ..Default::default()
            },
        })
        .unwrap();
    let session = Session::open(
        engine,
        HostProfile::default(),
        Some("owned-file-usage"),
        &d(),
    )
    .unwrap();
    Fixture { session, log, root }
}
fn invoke(f: &mut Fixture, args: Value) -> anyhow::Result<Value> {
    yoke::agent_repl::invoke_facade(&mut f.session, "jb.safe_delete", args, None, &d())
}
fn unpublished(f: &Fixture) {
    assert_eq!(f.session.engine.store.db.query_row("SELECT COUNT(*) FROM records WHERE kind IN ('plan','approval','receipt','journal')",[],|r|r.get::<_,u64>(0)).unwrap(),0);
    assert_eq!(
        f.session
            .engine
            .store
            .db
            .query_row(
                "SELECT COUNT(*) FROM snapshots WHERE json_extract(value,'$.kind')='derived'",
                [],
                |r| r.get::<_, u64>(0)
            )
            .unwrap(),
        0
    );
    assert!(f.root.path().join("spare_module.ts").is_file());
}
fn transcript(f: &Fixture) -> Vec<Value> {
    let p = f.log.path().join("protocol.log");
    if !p.exists() {
        return vec![];
    }
    fs::read_to_string(p)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}
#[test]
fn omitted_and_null_file_selectors_require_self_bound_compiler_queries_for_both_epochs() {
    for epoch in [AnalyzerEpoch::V8, AnalyzerEpoch::V9] {
        for selector in [None, Some(Value::Null)] {
            for forced in [None, Some(Value::Bool(false))] {
                let mut f = fixture(epoch, "ok");
                let mut args = json!({"relative_path":"spare_module.ts"});
                if let Some(selector) = &selector {
                    args["name_path"] = selector.clone();
                }
                if let Some(forced) = forced {
                    args["delete_even_if_used"] = forced;
                }
                let value = invoke(&mut f, args).unwrap();
                assert_eq!(value["forced"], false);
                assert_eq!(value["usageCheck"]["performed"], true);
                assert_eq!(value["usageCheck"]["providerRequestCount"], 2);
                assert_eq!(value["usageCheck"]["exhaustiveCompilerProof"], false);
                assert_eq!(value["complete"], false);
                assert_eq!(value["commandsExecuted"], false);
                assert_eq!(value["plan"]["files"].as_array().unwrap().len(), 1);
                assert_eq!(value["plan"]["files"][0]["path"], "spare_module.ts");
                assert!(value["plan"]["files"][0]["afterHash"].is_null());
                let snap = f
                    .session
                    .engine
                    .store
                    .snapshot(value["plan"]["resultSnapshotId"].as_str().unwrap())
                    .unwrap();
                assert_eq!(
                    yoke::capture::analyzer_epoch_for_snapshot(&f.session.engine.store, &snap)
                        .unwrap(),
                    epoch
                );
                assert!(!snap.files.iter().any(|file| file.path == "spare_module.ts"));
                assert_eq!(
                    fs::read(f.root.path().join("spare_module.ts")).unwrap(),
                    SOURCE.as_bytes()
                );
                let calls = transcript(&f);
                assert!(
                    calls
                        .iter()
                        .any(|call| call["method"] == "textDocument/definition")
                );
                assert!(
                    calls
                        .iter()
                        .any(|call| call["method"] == "textDocument/references"
                            && call["params"]["context"]["includeDeclaration"] == true)
                );
                assert!(
                    !calls
                        .iter()
                        .any(|call| call["method"] == "workspace/executeCommand")
                );
            }
        }
    }
}
#[test]
fn missing_empty_foreign_and_truncated_provider_responses_cannot_be_absence_proofs() {
    for mode in [
        "missing-references",
        "empty-definition",
        "empty-references",
        "foreign-reference",
        "overflow-references",
    ] {
        let mut f = fixture(AnalyzerEpoch::V9, mode);
        assert!(
            invoke(&mut f, json!({"relative_path":"spare_module.ts"})).is_err(),
            "{mode}"
        );
        unpublished(&f);
        assert!(!transcript(&f).is_empty());
    }
}

#[test]
fn eight_declarations_are_checked_and_the_ninth_refuses_before_provider_start() {
    for count in [8, 9] {
        let mut f = fixture(AnalyzerEpoch::V9, "ok");
        let source: String = (0..count)
            .map(|i| format!("export function unusedCafé{i}(): number {{ return {i}; }}\r\n"))
            .collect();
        fs::write(f.root.path().join("spare_module.ts"), source).unwrap();
        let result = invoke(&mut f, json!({"relative_path":"spare_module.ts"}));
        if count == 8 {
            let value = result.unwrap();
            assert_eq!(
                value["usageCheck"]["declarations"]
                    .as_array()
                    .unwrap()
                    .len(),
                8
            );
            assert_eq!(value["usageCheck"]["providerRequestCount"], 16);
        } else {
            assert!(result.is_err());
            unpublished(&f);
            assert!(transcript(&f).is_empty());
        }
    }
}

#[test]
fn actual_file_inventory_quota_admits2000_and_refuses2001_before_provider_start() {
    for count in [2000, 2001] {
        let mut f = fixture(AnalyzerEpoch::V9, "empty-definition");
        for i in 3..count {
            fs::write(f.root.path().join(format!("entry_{i:04}.txt")), b"").unwrap();
        }
        let result = invoke(&mut f, json!({"relative_path":"spare_module.ts"}));
        let error = result.expect_err("The owned fixture supplies no definition");
        unpublished(&f);
        assert_eq!(
            transcript(&f).is_empty(),
            count == 2001,
            "{count}: {error:#}"
        );
    }
}

#[test]
fn actual_source_byte_quota_counts_each_captured_file_before_provider_start() {
    for count in [16_000_000usize, 16_000_001] {
        let mut f = fixture(AnalyzerEpoch::V9, "empty-definition");
        let existing: usize = ["spare_module.ts", "other.ts", "tsconfig.json"]
            .iter()
            .map(|path| fs::metadata(f.root.path().join(path)).unwrap().len() as usize)
            .sum();
        let mut left = count - existing;
        for i in 0..4 {
            let size = left.min(4_000_000);
            fs::write(
                f.root.path().join(format!("payload_{i}.txt")),
                vec![b'x'; size],
            )
            .unwrap();
            left -= size;
        }
        assert_eq!(left, 0);
        assert!(invoke(&mut f, json!({"relative_path":"spare_module.ts"})).is_err());
        unpublished(&f);
        assert_eq!(transcript(&f).is_empty(), count == 16_000_001);
    }
}

#[test]
fn javascript_module_syntax_uses_the_registered_javascript_provider() {
    let mut f = fixture(AnalyzerEpoch::V8, "ok");
    fs::rename(
        f.root.path().join("spare_module.ts"),
        f.root.path().join("spare_module.js"),
    )
    .unwrap();
    fs::write(
        f.root.path().join("spare_module.js"),
        SOURCE.replace(": number", ""),
    )
    .unwrap();
    let mut server = f.session.engine.servers["typescript"].clone();
    server.language = "javascript".into();
    f.session
        .engine
        .register_configured_server(lsp::ConfiguredServer {
            server,
            configuration: lsp::HostConfiguration {
                passive_profile: Some(lsp::PassiveProfile::ContractFixture),
                ..Default::default()
            },
        })
        .unwrap();
    let value = invoke(&mut f, json!({"relative_path":"spare_module.js"})).unwrap();
    assert_eq!(value["plan"]["files"][0]["path"], "spare_module.js");
    assert_eq!(value["usageCheck"]["performed"], true);
    assert!(f.root.path().join("spare_module.js").is_file());
}
#[test]
fn path_name_and_graph_usage_candidates_refuse_before_provider_start() {
    for source in [
        "import './spare_module';\r\nexport const untouched = 7;\r\n",
        "export const untouched = 'unusedCafé';\r\n",
        "export const untouched = 'SPARE_MODULE.ts';\r\n",
    ] {
        let mut f = fixture(AnalyzerEpoch::V8, "ok");
        fs::write(f.root.path().join("other.ts"), source).unwrap();
        assert!(invoke(&mut f, json!({"relative_path":"spare_module.ts"})).is_err());
        unpublished(&f);
        assert!(transcript(&f).is_empty());
    }
}
#[test]
fn unsupported_source_shapes_paths_permissions_and_capacity_do_not_start_a_provider() {
    for case in [
        "side-effect",
        "anonymous",
        "variable",
        "syntax",
        "source-budget",
        "traversal",
        "empty-selector",
        "permission",
        "missing-provider",
        "untrusted-provider",
        "missing-passive-policy",
        "case-alias",
        "propagation",
    ] {
        let mut f = fixture(AnalyzerEpoch::V9, "ok");
        let mut args = json!({"relative_path":"spare_module.ts"});
        match case {
            "side-effect" => fs::write(
                f.root.path().join("spare_module.ts"),
                format!("{SOURCE}console.log('owned');\r\n"),
            )
            .unwrap(),
            "anonymous" => fs::write(
                f.root.path().join("spare_module.ts"),
                "export default function() { return 1; }\n",
            )
            .unwrap(),
            "variable" => fs::write(
                f.root.path().join("spare_module.ts"),
                "export const callable = () => 1;\n",
            )
            .unwrap(),
            "syntax" => fs::write(
                f.root.path().join("spare_module.ts"),
                "export function broken( {\n",
            )
            .unwrap(),
            "source-budget" => fs::write(
                f.root.path().join("spare_module.ts"),
                format!("{SOURCE}{}", " ".repeat(256_001)),
            )
            .unwrap(),
            "traversal" => args["relative_path"] = json!("../spare_module.ts"),
            "empty-selector" => args["name_path"] = json!(""),
            "permission" => {
                f.session
                    .invoke("switchModes", &json!({"modes":["planning"]}), &d())
                    .unwrap();
            }
            "missing-provider" => f.session.engine.servers.clear(),
            "untrusted-provider" => {
                f.session
                    .engine
                    .servers
                    .get_mut("typescript")
                    .unwrap()
                    .trust = false
            }
            "missing-passive-policy" => f.session.engine.server_configurations.clear(),
            "case-alias" => args["relative_path"] = json!("SPARE_MODULE.ts"),
            _ => args["propagate"] = json!(true),
        }
        assert!(invoke(&mut f, args).is_err(), "{case}");
        unpublished(&f);
        assert!(transcript(&f).is_empty(), "{case}");
    }
}
#[test]
fn real_owned_copied_verification_and_existing_approval_stale_and_recovery_gates_remain() {
    let mut f = fixture(AnalyzerEpoch::V9, "ok");
    let value = invoke(&mut f, json!({"relative_path":"spare_module.ts"})).unwrap();
    let id = value["plan"]["id"].as_str().unwrap();
    let args = json!({"planId":id,"idempotencyKey":"owned-file-usage","mode":"workspace"});
    assert!(changes::apply(&f.session.engine.store, f.root.path(), &args, true, &d()).is_err());
    let verification=yoke::verify::execute(&f.session.engine,id,"node",&["-e".into(),"const fs=require('fs');process.exit(!fs.existsSync('spare_module.ts')&&fs.readFileSync('other.ts','utf8')==='export const untouched = 7;\\r\\n'?0:1)".into()],true,&d()).unwrap();
    assert_eq!(verification["status"], "passed", "{verification}");
    assert!(changes::apply(&f.session.engine.store, f.root.path(), &args, true, &d()).is_err());
    changes::approve(&f.session.engine.store, id, false).unwrap();
    assert!(changes::apply(&f.session.engine.store, f.root.path(), &args, false, &d()).is_err());
    fs::write(
        f.root.path().join("spare_module.ts"),
        format!("{SOURCE}// newer owned bytes\r\n"),
    )
    .unwrap();
    assert!(changes::apply(&f.session.engine.store, f.root.path(), &args, true, &d()).is_err());
    assert_eq!(
        f.session
            .engine
            .store
            .db
            .query_row(
                "SELECT COUNT(*) FROM records WHERE kind='journal'",
                [],
                |row| row.get::<_, usize>(0)
            )
            .unwrap(),
        0
    );
    fs::write(f.root.path().join("spare_module.ts"), SOURCE).unwrap();
    let receipt =
        changes::apply(&f.session.engine.store, f.root.path(), &args, true, &d()).unwrap();
    assert!(!f.root.path().join("spare_module.ts").exists());
    changes::recover(&f.session.engine.store, f.root.path(), &receipt.id, true).unwrap();
    assert_eq!(
        fs::read(f.root.path().join("spare_module.ts")).unwrap(),
        SOURCE.as_bytes()
    );
}
