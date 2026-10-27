//! Actual owned activation processes and lifecycle/protocol canaries. Authored
//! during a source-only freeze; execution/qualification is a separate host gate.
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};
use tempfile::TempDir;
use yoke::{
    Engine,
    capture::Deadline,
    project_activation::{Runtime, Selection, Shell, ShellKind},
    project_session::{HostProfile, RECORD_KIND, Session},
};

fn d() -> Deadline {
    Deadline::new(20000)
}
fn root() -> TempDir {
    let r = tempfile::tempdir().unwrap();
    fs::write(
        r.path().join("main.rs"),
        "// original α\nfn original() {}\n",
    )
    .unwrap();
    r
}
fn shell() -> Shell {
    #[cfg(windows)]
    let (path, kind) = (
        PathBuf::from(std::env::var_os("SystemRoot").expect("Host Windows installation"))
            .join("System32/WindowsPowerShell/v1.0/powershell.exe"),
        ShellKind::Powershell,
    );
    #[cfg(not(windows))]
    let (path, kind) = (PathBuf::from("/bin/sh"), ShellKind::Posix);
    Shell {
        executable: path.canonicalize().expect("Explicit test host shell"),
        kind,
    }
}
// These dependencies belong to the explicitly authored test command. They are
// included before profile construction and exact command hashing, inside the
// command's existing budget; production grants never add a shell prelude.
fn owned_command(command: &str) -> String {
    #[cfg(windows)]
    {
        format!(
            "{}\n{}\n{command}",
            include_str!("fixtures/windows-powershell-utility.ps1"),
            include_str!("fixtures/windows-powershell-management.ps1")
        )
    }
    #[cfg(not(windows))]
    {
        command.to_owned()
    }
}
fn append_body() -> &'static str {
    if cfg!(windows) {
        "[IO.File]::AppendAllText((Join-Path (Get-Location).Path 'activation.log'), \"ran`n\"); [Console]::Out.Write('private-output'); [Console]::Error.Write('private-error')"
    } else {
        "printf 'ran\\n' >> activation.log; printf private-output; printf private-error >&2"
    }
}
fn append() -> String {
    owned_command(append_body())
}
fn count(root: &Path) -> usize {
    fs::read_to_string(root.join("activation.log"))
        .unwrap_or_default()
        .lines()
        .count()
}
fn profile(command: &str, seconds: Value) -> HostProfile {
    serde_json::from_value(json!({"activationCommands":{"global":{"activation_command":command,"activation_command_timeout":seconds}}})).unwrap()
}
fn selection(host: &HostProfile, project: &str, root: &Path, command: &str) -> Selection {
    Selection {
        project: project.into(),
        root: root.canonicalize().unwrap(),
        command_sha256: yoke::model::hash(command),
        configuration_sha256: host.activation_commands.as_ref().unwrap().fingerprint(),
        shell: shell(),
    }
}
fn grant(host: &mut HostProfile, primary: &Path, selected: &[Selection]) {
    host.activation_runtime = Some(Runtime::authorize(host, primary, selected, &d()).unwrap());
}
fn open(root: &Path, host: HostProfile, id: &str) -> Session {
    Session::open(Engine::open(root).unwrap(), host, Some(id), &d()).unwrap()
}
fn config(s: &mut Session) -> Value {
    s.invoke("configuration", &json!({}), &d()).unwrap()
}
fn receipt(s: &mut Session) -> Value {
    config(s)["projectActivation"]["lastAttempt"].clone()
}

#[test]
fn declarations_saved_profiles_and_inspection_never_create_execution_authority() {
    let r = root();
    let command = append();
    let host = profile(&command, json!(180));
    let serialized = serde_json::to_vec(&host).unwrap();
    let restored: HostProfile = serde_json::from_slice(&serialized).unwrap();
    let error = Session::open(
        Engine::open(r.path()).unwrap(),
        restored,
        Some("no-grant"),
        &d(),
    )
    .err()
    .unwrap();
    assert!(
        error
            .to_string()
            .contains("activation_host_permission_required")
    );
    assert_eq!(count(r.path()), 0);
    let mut engine = Engine::open(r.path()).unwrap();
    // Independent language-server trust is not shell authorization. If the
    // implementation accidentally conflates them, this owned command canary runs.
    let selected_shell = shell();
    engine
        .register_server(yoke::lsp::Server {
            command: selected_shell.executable.display().to_string(),
            args: vec!["-c".into(), append()],
            language: "rust".into(),
            trust: true,
        })
        .unwrap();
    assert!(Session::open(engine, host.clone(), Some("server-trust"), &d()).is_err());
    assert_eq!(count(r.path()), 0);
    let mut inspect =
        Session::inspect(Engine::open(r.path()).unwrap(), host, Some("inspect"), &d()).unwrap();
    let c = config(&mut inspect);
    assert!(c["activeProject"].is_null());
    assert_eq!(c["selectedProject"], "default");
    for (op, args) in [
        ("listProjects", json!({})),
        ("checkOnboarding", json!({})),
        ("activationReceipt", json!({})),
        ("fs.read", json!({"path":"main.rs"})),
    ] {
        inspect.invoke(op, &args, &d()).unwrap();
    }
    assert_eq!(count(r.path()), 0);
    assert!(
        inspect
            .invoke("activate", &json!({"project":"default"}), &d())
            .is_err()
    );
    assert_eq!(
        inspect
            .engine
            .store
            .db
            .query_row(
                "SELECT count(*) FROM records WHERE kind=?1",
                [RECORD_KIND],
                |r| r.get::<_, u64>(0)
            )
            .unwrap(),
        0
    );
}

#[test]
fn independent_wrong_command_project_root_and_configuration_pins_refuse_before_canaries() {
    let a = root();
    let b = root();
    let command = append();
    let host = profile(&command, json!(180));
    let baseline = selection(&host, "default", a.path(), &command);
    for variant in 0..4 {
        let mut wrong = baseline.clone();
        match variant {
            0 => wrong.command_sha256 = yoke::model::hash("a different exact command"),
            1 => wrong.project = "unregistered".into(),
            2 => wrong.root = b.path().canonicalize().unwrap(),
            _ => {
                wrong.configuration_sha256 =
                    yoke::model::hash("independently different configuration")
            }
        }
        assert!(Runtime::authorize(&host, a.path(), &[wrong], &d()).is_err());
        assert_eq!(count(a.path()), 0);
        assert_eq!(count(b.path()), 0);
    }
    let mut changed = host.clone();
    grant(&mut changed, a.path(), &[baseline]);
    changed.activation_commands = profile("different-command", json!(180)).activation_commands;
    assert!(
        Session::open(
            Engine::open(a.path()).unwrap(),
            changed,
            Some("changed"),
            &d()
        )
        .is_err()
    );
    assert_eq!(count(a.path()), 0);
}

#[test]
fn initial_resume_repeated_and_cached_activation_precede_publication_and_preserve_source() {
    let r = root();
    let command = append();
    let mut host = profile(&command, json!(180));
    let pin = selection(&host, "default", r.path(), &command);
    grant(&mut host, r.path(), &[pin]);
    let mut engine = Engine::open(r.path()).unwrap();
    let (before, _, _, _) = engine.index(&d()).unwrap();
    let original = before
        .files
        .iter()
        .find(|f| f.path == "main.rs")
        .unwrap()
        .clone();
    let mut s = Session::open(engine, host.clone(), Some("life"), &d()).unwrap();
    assert_eq!(count(r.path()), 1);
    let first = receipt(&mut s);
    assert_eq!(first["phase"], "activated");
    assert_eq!(first["sessionRevisionBefore"], 0);
    assert_eq!(
        first["observation"]["requestedTimeoutNs"],
        180_000_000_000u64
    );
    for op in ["configuration", "listProjects", "activationReceipt"] {
        s.invoke(op, &json!({}), &d()).unwrap();
    }
    assert_eq!(count(r.path()), 1);
    let private = s.take_activation_diagnostic().unwrap();
    assert_eq!(private.stdout, b"private-output");
    assert_eq!(private.stderr, b"private-error");
    let public = config(&mut s).to_string();
    assert!(
        !public.contains("private-output")
            && !public.contains("private-error")
            && !public.contains(&command)
    );
    for expected in [2, 3] {
        s.invoke("activate", &json!({"project":"default"}), &d())
            .unwrap();
        assert_eq!(count(r.path()), expected);
    }
    assert_eq!(
        s.engine
            .store
            .read_blob_exact(&original.hash, original.size, 4_000_000)
            .unwrap(),
        fs::read(r.path().join("main.rs")).unwrap()
    );
    drop(s);
    let mut passive = Session::inspect(
        Engine::open(r.path()).unwrap(),
        host.clone(),
        Some("life"),
        &d(),
    )
    .unwrap();
    assert_eq!(receipt(&mut passive)["phase"], "activated");
    assert_eq!(count(r.path()), 3);
    drop(passive);
    let resumed = open(r.path(), host, "life");
    assert_eq!(count(r.path()), 4);
    drop(resumed);
}

#[test]
fn nonzero_exit_keeps_external_effects_and_continues_with_complete_cleanup_receipt() {
    let r = root();
    let command = format!("{}; exit 7", append());
    let mut host = profile(&command, json!(0.75));
    // PowerShell startup can exceed a fraction under load; use a separate ample
    // positive fraction here while the deterministic conversion test pins 0.125.
    host.activation_commands = profile(&command, json!(10.5)).activation_commands;
    let pin = selection(&host, "default", r.path(), &command);
    grant(&mut host, r.path(), &[pin]);
    let mut s = open(r.path(), host, "nonzero");
    assert_eq!(count(r.path()), 1);
    let r = receipt(&mut s);
    assert_eq!(r["phase"], "activated");
    assert_eq!(r["observation"]["exitCode"], 7);
    assert_eq!(r["observation"]["commandTimedOut"], false);
    assert_eq!(r["observation"]["childTreeCleanupProven"], true);
    assert_eq!(r["observation"]["streamsComplete"], true);
    assert_eq!(r["observation"]["requestedTimeoutNs"], 10_500_000_000u64);
}

#[test]
fn command_timeout_with_descendants_requires_checked_tree_cleanup_and_remaining_parent_time() {
    let r = root();
    #[cfg(windows)]
    let command = {
        use base64::Engine as _;
        // A fresh PowerShell child does not inherit its parent's loaded module.
        // Encode the fixed owned payload so nested CLI quoting cannot alter it.
        let child = format!(
            "{}\nStart-Sleep -Seconds 30",
            include_str!("fixtures/windows-powershell-utility.ps1")
        );
        let utf16: Vec<_> = child.encode_utf16().collect();
        assert!(utf16.len() <= 4096);
        let encoded = base64::prelude::BASE64_STANDARD.encode(
            utf16
                .into_iter()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>(),
        );
        owned_command(&format!(
            "[IO.File]::WriteAllText((Join-Path (Get-Location).Path 'started'),'1'); $p=Start-Process -FilePath (Join-Path $PSHOME 'powershell.exe') -ArgumentList '-NoProfile','-EncodedCommand','{encoded}' -PassThru -WindowStyle Hidden; $p.WaitForExit()"
        ))
    };
    #[cfg(not(windows))]
    let command = owned_command("printf started > started; sleep 30 & wait");
    let mut host = profile(&command, json!(2.5));
    let pin = selection(&host, "default", r.path(), &command);
    grant(&mut host, r.path(), &[pin]);
    let start = Instant::now();
    let mut s = open(r.path(), host, "timeout");
    assert!(r.path().join("started").exists());
    assert!(start.elapsed() < Duration::from_secs(10));
    let r = receipt(&mut s);
    assert_eq!(r["phase"], "activated");
    assert_eq!(r["observation"]["commandTimedOut"], true);
    assert_eq!(r["observation"]["childTreeCleanupProven"], true);
    assert_eq!(r["observation"]["streamsComplete"], true);
}

fn saved_session(s: &Session) -> (String, String) {
    s.engine
        .store
        .db
        .query_row(
            "SELECT id,value FROM records WHERE kind=?1",
            [RECORD_KIND],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap()
}
fn assert_revoked_backend(s: &mut Session) {
    let error = s
        .active_engine_at(&d())
        .err()
        .expect("A superseded activation witness must refuse backend admission");
    let message = error.to_string();
    assert!(
        message.contains("activation_receipt_changed_backend_refused")
            || message.contains("activation_named_attempt_changed"),
        "{error:#}"
    );
}
fn assert_unknown_attempt(s: &Session, id: &str) {
    let text: String = s
        .engine
        .store
        .db
        .query_row(
            "SELECT value FROM meta WHERE key=?1",
            [format!("{}/{}", yoke::project_activation::VERSION, id)],
            |row| row.get(0),
        )
        .unwrap();
    let value: Value = serde_json::from_str(&text).unwrap();
    assert!(matches!(
        value["phase"].as_str(),
        Some("attempting" | "outcome_unknown")
    ));
}
#[test]
fn failed_resume_revokes_an_older_live_handles_cached_backend_without_state_change() {
    let r = root();
    let command = if cfg!(windows) {
        "if ([IO.File]::Exists('activation.once')) { [Console]::Out.Write(('x' * 70000)) } else { [IO.File]::WriteAllText('activation.once', 'ready') }"
    } else {
        "if [ -f activation.once ]; then i=0; while [ $i -lt 70000 ]; do printf x; i=$((i+1)); done; else printf ready > activation.once; fi"
    };
    let mut host = profile(command, json!(180));
    let pin = selection(&host, "default", r.path(), command);
    grant(&mut host, r.path(), &[pin]);
    let mut older = open(r.path(), host.clone(), "shared-failure");
    older.active_engine_at(&d()).unwrap();
    let before = saved_session(&older);
    assert!(
        Session::open(
            Engine::open(r.path()).unwrap(),
            host,
            Some("shared-failure"),
            &d()
        )
        .is_err()
    );
    assert_eq!(
        saved_session(&older),
        before,
        "The ordinary session CAS must remain unchanged in this regression"
    );
    assert_unknown_attempt(&older, &before.0);
    assert_revoked_backend(&mut older);
}
#[test]
fn successful_resume_replaces_the_older_handles_durable_activation_witness() {
    let r = root();
    let command = append();
    let mut host = profile(&command, json!(180));
    let pin = selection(&host, "default", r.path(), &command);
    grant(&mut host, r.path(), &[pin]);
    let mut older = open(r.path(), host.clone(), "shared-success");
    older.active_engine_at(&d()).unwrap();
    let before = saved_session(&older);
    let mut newer = open(r.path(), host, "shared-success");
    assert_eq!(count(r.path()), 2);
    assert_eq!(saved_session(&older), before);
    newer.active_engine_at(&d()).unwrap();
    assert_revoked_backend(&mut older);
}
#[test]
fn uncertain_secondary_attempt_also_revokes_a_command_free_cached_project() {
    let a = root();
    let b = root();
    let command = if cfg!(windows) {
        "[Console]::Out.Write(('x' * 70000))"
    } else {
        "i=0; while [ $i -lt 70000 ]; do printf x; i=$((i+1)); done"
    };
    let mut host: HostProfile = serde_json::from_value(json!({
        "projects":[{"name":"other","root":b.path().canonicalize().unwrap()}],
        "activationCommands":{"projects":{"other":{"project":{"activation_command":command}}}}
    }))
    .unwrap();
    let pin = selection(&host, "other", b.path(), command);
    grant(&mut host, a.path(), &[pin]);
    let mut older = open(a.path(), host.clone(), "shared-secondary");
    older.active_engine_at(&d()).unwrap();
    let before = saved_session(&older);
    let mut newer = open(a.path(), host, "shared-secondary");
    assert!(
        newer
            .invoke("activate", &json!({"project":"other"}), &d())
            .is_err()
    );
    assert_eq!(saved_session(&older), before);
    assert_unknown_attempt(&older, &before.0);
    assert_revoked_backend(&mut older);
}

#[test]
fn output_capacity_and_enclosing_expiry_refuse_publication_and_do_not_automatically_retry() {
    for long in [false, true] {
        let r = root();
        let command = if long {
            if cfg!(windows) {
                "Start-Sleep -Seconds 30"
            } else {
                "sleep 30"
            }
        } else if cfg!(windows) {
            "[Console]::Out.Write(('x' * 70000))"
        } else {
            "i=0; while [ $i -lt 70000 ]; do printf x; i=$((i+1)); done"
        };
        let command = owned_command(command);
        let mut host = profile(&command, json!(180));
        let pin = selection(&host, "default", r.path(), &command);
        grant(&mut host, r.path(), &[pin]);
        let deadline = if long { Deadline::new(1000) } else { d() };
        assert!(
            Session::open(
                Engine::open(r.path()).unwrap(),
                host.clone(),
                Some("refused"),
                &deadline
            )
            .is_err()
        );
        let mut inspect = Session::inspect(
            Engine::open(r.path()).unwrap(),
            host.clone(),
            Some("refused"),
            &d(),
        )
        .unwrap();
        let receipt = receipt(&mut inspect);
        assert!(matches!(
            receipt["phase"].as_str(),
            Some("attempting" | "outcome_unknown")
        ));
        assert!(config(&mut inspect)["activeProject"].is_null());
        drop(inspect);
        let error = Session::open(Engine::open(r.path()).unwrap(), host, Some("refused"), &d())
            .err()
            .unwrap();
        assert!(error.to_string().contains("prior_outcome_unknown"));
    }
}

#[test]
fn lazy_secondary_requires_explicit_activation_and_target_cwd_before_cold_engine() {
    let a = root();
    let b = root();
    let command = if cfg!(windows) {
        owned_command(&format!(
            "if (Test-Path '.yoke') {{ exit 19 }}; {}",
            append_body()
        ))
    } else {
        format!("test ! -d .yoke || exit 19; {}", append())
    };
    let mut host:HostProfile=serde_json::from_value(json!({"projects":[{"name":"other","root":b.path().canonicalize().unwrap()}],"activationCommands":{"projects":{"other":{"project":{"activation_command":command}}}}})).unwrap();
    let pin = selection(&host, "other", b.path(), &command);
    grant(&mut host, a.path(), &[pin]);
    let mut s = open(a.path(), host, "secondary");
    assert_eq!(count(a.path()), 0);
    assert!(!b.path().join(".yoke").exists());
    assert!(
        s.invoke(
            "queryProject",
            &json!({"project":"other","tool":"ci_status","arguments":{}}),
            &d()
        )
        .is_err()
    );
    assert_eq!(count(b.path()), 0);
    assert!(!b.path().join(".yoke").exists());
    s.invoke("activate", &json!({"project":"other"}), &d())
        .unwrap();
    assert_eq!(count(b.path()), 1);
    assert_eq!(receipt(&mut s)["observation"]["exitCode"], 0);
    assert!(b.path().join(".yoke").exists());
    // Repeated explicit activation executes again even with the cached engine;
    // its deliberate nonzero status still continues under the proven policy.
    s.invoke("activate", &json!({"project":"other"}), &d())
        .unwrap();
    assert_eq!(receipt(&mut s)["observation"]["exitCode"], 19);
}

#[test]
fn removing_an_active_secondary_runs_the_cached_default_hook_before_state_publication() {
    for refuse in [false, true] {
        let a = root();
        let b = root();
        let command = if cfg!(windows) {
            format!(
                "{}; if ([IO.File]::Exists('deny-default')) {{ [Console]::Out.Write(('x' * 70000)) }}",
                append()
            )
        } else {
            format!(
                "{}; if [ -f deny-default ]; then i=0; while [ $i -lt 70000 ]; do printf x; i=$((i+1)); done; fi",
                append()
            )
        };
        let mut host: HostProfile = serde_json::from_value(json!({
            "projects":[{"name":"other","root":b.path().canonicalize().unwrap()}],
            "activationCommands":{"global":{"activation_command":command}}
        }))
        .unwrap();
        let selections = [
            selection(&host, "default", a.path(), &command),
            selection(&host, "other", b.path(), &command),
        ];
        grant(&mut host, a.path(), &selections);
        let secondary_source = fs::read(b.path().join("main.rs")).unwrap();
        let mut session = open(a.path(), host, "remove-active");
        session.active_engine_at(&d()).unwrap();
        session
            .invoke("activate", &json!({"project":"other"}), &d())
            .unwrap();
        assert_eq!(count(a.path()), 1);
        assert_eq!(count(b.path()), 1);
        let before = saved_session(&session);
        if refuse {
            fs::write(a.path().join("deny-default"), "owned refusal canary").unwrap();
        }
        let result = session.invoke("removeProject", &json!({"project":"other"}), &d());
        assert_eq!(count(a.path()), 2);
        assert_eq!(count(b.path()), 1);
        assert_eq!(
            fs::read(b.path().join("main.rs")).unwrap(),
            secondary_source
        );
        let projects = session.invoke("listProjects", &json!({}), &d()).unwrap();
        if refuse {
            assert!(result.is_err());
            assert_eq!(saved_session(&session), before);
            assert_eq!(config(&mut session)["activeProject"], "other");
            assert!(
                projects["items"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|project| project["name"] == "other")
            );
            assert_unknown_attempt(&session, &before.0);
            assert!(session.active_engine_at(&d()).is_err());
        } else {
            assert_eq!(result.unwrap()["sourceDeleted"], false);
            assert_eq!(config(&mut session)["activeProject"], "default");
            assert!(
                projects["items"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|project| project["name"] != "other")
            );
            assert_eq!(receipt(&mut session)["project"], "default");
            assert_eq!(receipt(&mut session)["phase"], "activated");
            session.active_engine_at(&d()).unwrap();
            assert_eq!(count(a.path()), 2);
        }
    }
}

#[test]
fn replacing_state_during_an_attempt_with_a_prior_clone_cannot_publish_or_reuse_a_backend() {
    let r = root();
    let command = if cfg!(windows) {
        format!(
            "{}; if ([IO.Directory]::Exists('.activation-clone')) {{ try {{ [IO.Directory]::Move('.yoke','.activation-old'); [IO.Directory]::Move('.activation-clone','.yoke'); [IO.File]::WriteAllText('namespace-replaced','1') }} catch {{ [IO.File]::WriteAllText('namespace-refused','1'); exit 23 }} }}",
            append()
        )
    } else {
        format!(
            "{}; if [ -d .activation-clone ]; then mv .yoke .activation-old && mv .activation-clone .yoke && printf replaced > namespace-replaced || exit 23; fi",
            append()
        )
    };
    let mut host = profile(&command, json!(180));
    let pin = selection(&host, "default", r.path(), &command);
    grant(&mut host, r.path(), &[pin]);
    let mut session = open(r.path(), host, "state-clone");
    session.active_engine_at(&d()).unwrap();
    let before = saved_session(&session);
    let prior_receipt = receipt(&mut session);
    let clone = r.path().join(".activation-clone");
    fs::create_dir(&clone).unwrap();
    for directory in ["blobs", "trees"] {
        fs::create_dir(clone.join(directory)).unwrap();
    }
    for lock in ["clients.lock", "maintenance.lock"] {
        fs::write(clone.join(lock), []).unwrap();
    }
    // SQLite creates an independent, committed clone before the new attempt's
    // nonce exists. Copying only live database bytes would omit WAL state.
    session
        .engine
        .store
        .db
        .execute(
            "VACUUM main INTO ?1",
            [clone.join("index.sqlite").to_str().unwrap()],
        )
        .unwrap();
    let result = session.invoke("activate", &json!({"project":"default"}), &d());
    let replaced = r.path().join("namespace-replaced").exists();
    #[cfg(not(windows))]
    assert!(
        replaced,
        "The owned Unix namespace replacement must complete"
    );
    assert_eq!(count(r.path()), 2);
    if replaced {
        assert!(result.is_err());
        assert_eq!(saved_session(&session), before);
        assert!(session.active_engine_at(&d()).is_err());
        let named = yoke::store::Store::open_existing_read_only(r.path(), &d()).unwrap();
        assert_eq!(named.workspace_id, session.engine.store.workspace_id);
        let named_session: (String, String) = named
            .db
            .query_row(
                "SELECT id,value FROM records WHERE kind=?1",
                [RECORD_KIND],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(named_session, before);
        let named_receipt: String = named
            .db
            .query_row(
                "SELECT value FROM meta WHERE key=?1",
                [format!(
                    "{}/{}",
                    yoke::project_activation::VERSION,
                    before.0
                )],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&named_receipt).unwrap(),
            prior_receipt
        );
    } else {
        // Windows may deny the owned rename while SQLite holds the original
        // namespace. That branch must retain the original names and complete
        // the nonzero command through the existing cleanup/receipt policy.
        assert!(r.path().join("namespace-refused").exists());
        assert!(!r.path().join(".activation-old").exists());
        assert!(r.path().join(".yoke/index.sqlite").is_file());
        result.unwrap();
        assert_eq!(receipt(&mut session)["phase"], "activated");
        assert_eq!(receipt(&mut session)["observation"]["exitCode"], 23);
        session.active_engine_at(&d()).unwrap();
    }
}

#[test]
fn unicode_quoting_and_physical_root_replacement_do_not_rebind_authority() {
    let parent = root();
    let selected = parent.path().join("α-'quoted'-İ");
    fs::create_dir(&selected).unwrap();
    let command = if cfg!(windows) {
        "[IO.File]::WriteAllText((Join-Path (Get-Location).Path 'quote.txt'), 'α ''single'' \"double\"', [Text.UTF8Encoding]::new($false))"
    } else {
        "printf '%s' 'α '\"'\"'single'\"'\"' \"double\"' > quote.txt"
    };
    let command = owned_command(command);
    let mut host = profile(&command, json!(180));
    let pin = selection(&host, "default", &selected, &command);
    grant(&mut host, &selected, &[pin]);
    let s = open(&selected, host.clone(), "unicode");
    assert_eq!(
        fs::read_to_string(selected.join("quote.txt")).unwrap(),
        "α 'single' \"double\""
    );
    drop(s);
    let moved = parent.path().join("moved");
    let renamed = fs::rename(&selected, &moved);
    #[cfg(not(windows))]
    renamed
        .as_ref()
        .expect("Owned project rename must succeed outside Windows");
    if renamed.is_ok() {
        fs::create_dir(&selected).unwrap();
        assert!(
            Session::open(
                Engine::open(&selected).unwrap(),
                host,
                Some("replacement"),
                &d()
            )
            .is_err()
        );
        assert!(!selected.join("quote.txt").exists());
    }
}

#[test]
fn stale_session_during_an_irreversible_attempt_refuses_before_secondary_backend_publication() {
    let a = root();
    let b = root();
    let command = if cfg!(windows) {
        "[IO.File]::WriteAllText((Join-Path (Get-Location).Path 'started'),'1'); while (!(Test-Path 'release')) { Start-Sleep -Milliseconds 5 }; [IO.File]::WriteAllText((Join-Path (Get-Location).Path 'effect'),'irreversible')"
    } else {
        "printf started > started; while [ ! -f release ]; do sleep 0.01; done; printf irreversible > effect"
    };
    let command = owned_command(command);
    let mut host:HostProfile=serde_json::from_value(json!({"projects":[{"name":"other","root":b.path().canonicalize().unwrap()}],"activationCommands":{"projects":{"other":{"project":{"activation_command":command}}}}})).unwrap();
    let pin = selection(&host, "other", b.path(), &command);
    grant(&mut host, a.path(), &[pin]);
    let mut s = open(a.path(), host.clone(), "raced");
    let before = config(&mut s);
    let sqlite = s.engine.store.dir.join("index.sqlite");
    let other = b.path().to_owned();
    let racer = std::thread::spawn(move || {
        let end = Instant::now() + Duration::from_secs(15);
        while !other.join("started").exists() {
            assert!(Instant::now() < end);
            std::thread::sleep(Duration::from_millis(5));
        }
        let db = rusqlite::Connection::open(sqlite).unwrap();
        db.busy_timeout(Duration::from_secs(2)).unwrap();
        let text: String = db
            .query_row(
                "SELECT value FROM records WHERE kind=?1",
                [RECORD_KIND],
                |r| r.get(0),
            )
            .unwrap();
        let mut state: Value = serde_json::from_str(&text).unwrap();
        state["revision"] = json!(71);
        assert_eq!(
            db.execute(
                "UPDATE records SET value=?1 WHERE kind=?2 AND value=?3",
                rusqlite::params![state.to_string(), RECORD_KIND, text]
            )
            .unwrap(),
            1
        );
        fs::write(other.join("release"), "1").unwrap();
    });
    let result = s.invoke("activate", &json!({"project":"other"}), &d());
    racer.join().unwrap();
    let error = result.err().unwrap();
    assert!(error.to_string().contains("outcome_unknown"));
    assert!(b.path().join("effect").exists());
    assert!(!b.path().join(".yoke").exists());
    drop(s);
    let mut inspect =
        Session::inspect(Engine::open(a.path()).unwrap(), host, Some("raced"), &d()).unwrap();
    assert_eq!(
        config(&mut inspect)["activeProject"],
        before["activeProject"]
    );
    let r = receipt(&mut inspect);
    assert_eq!(r["phase"], "outcome_unknown");
    assert_eq!(r["refusal"], "stale_after_attempt");
    assert!(r.get("notApplied").is_none());
}

#[test]
fn session_cas_failure_after_command_is_unknown_and_blocks_cached_engine_reuse() {
    let r = root();
    let command = append();
    let mut host = profile(&command, json!(180));
    let pin = selection(&host, "default", r.path(), &command);
    grant(&mut host, r.path(), &[pin]);
    let mut s = open(r.path(), host.clone(), "cas");
    assert_eq!(count(r.path()), 1);
    let before = config(&mut s);
    s.engine.store.db.execute_batch("CREATE TRIGGER activation_test_cas BEFORE UPDATE ON records WHEN old.kind='native_agent_session_v1' BEGIN SELECT RAISE(IGNORE); END;").unwrap();
    let result = s.invoke("activate", &json!({"project":"default"}), &d());
    assert!(result.is_err());
    assert_eq!(count(r.path()), 2);
    assert_eq!(config(&mut s)["revision"], before["revision"]);
    assert_eq!(receipt(&mut s)["phase"], "outcome_unknown");
    assert!(s.active_engine_at(&d()).is_err());
    s.engine
        .store
        .db
        .execute_batch("DROP TRIGGER activation_test_cas;")
        .unwrap();
    drop(s);
    assert!(Session::open(Engine::open(r.path()).unwrap(), host, Some("cas"), &d()).is_err());
    assert_eq!(count(r.path()), 2);
}

#[test]
fn lost_success_response_replays_a_safe_receipt_without_an_implicit_retry() {
    let r = root();
    let command = append();
    let mut host = profile(&command, json!(180));
    let pin = selection(&host, "default", r.path(), &command);
    grant(&mut host, r.path(), &[pin]);
    let mut s = open(r.path(), host.clone(), "lost");
    // Deliberately lose the caller's successful value after publication.
    drop(
        s.invoke("activate", &json!({"project":"default"}), &d())
            .unwrap(),
    );
    let id = receipt(&mut s)["attemptId"].clone();
    drop(s);
    assert_eq!(count(r.path()), 2);
    let mut passive =
        Session::inspect(Engine::open(r.path()).unwrap(), host, Some("lost"), &d()).unwrap();
    let recovered = receipt(&mut passive);
    assert_eq!(recovered["attemptId"], id);
    assert_eq!(recovered["phase"], "activated");
    assert!(recovered.get("notApplied").is_none());
    assert_eq!(count(r.path()), 2);
}

#[test]
fn startup_project_and_mode_precedence_bind_only_the_actual_selected_hook() {
    let a = root();
    let b = root();
    let command = append();
    let mut host:HostProfile=serde_json::from_value(json!({"startupProject":"other","context":"agent","projects":[{"name":"other","root":b.path().canonicalize().unwrap()}],"activationCommands":{"global":{"activation_command":"ungranted-primary-canary"},"projects":{"other":{"project":{"activation_command":command,"activation_command_timeout":5.5}}}},"steering":{"global":{"defaultModes":["editing"]},"projects":{"other":{"project":{"defaultModes":["planning"]}}}}})).unwrap();
    let pin = selection(&host, "other", b.path(), &command);
    grant(&mut host, a.path(), &[pin]);
    let mut s = open(a.path(), host, "startup");
    assert_eq!(count(a.path()), 0);
    assert_eq!(count(b.path()), 1);
    let c = config(&mut s);
    assert_eq!(c["activeProject"], "other");
    assert!(c["modes"].as_array().unwrap().contains(&json!("planning")));
    assert_eq!(receipt(&mut s)["project"], "other");
    assert!(!b.path().join(".yoke").exists());
    s.active_engine_at(&d()).unwrap();
    assert!(b.path().join(".yoke").exists());
    assert_eq!(count(b.path()), 1);
    assert!(
        s.invoke("activate", &json!({"project":"default"}), &d())
            .is_err()
    );
    assert_eq!(count(a.path()), 0);
}

#[test]
fn native_and_serena_protocols_reject_forged_grants_and_share_explicit_activation() {
    use yoke::protocol::{ConnectionState, Profile};
    for profile in [Profile::Session, Profile::Serena] {
        let r = root();
        let command = append();
        let mut host = profile_host(&command);
        let pin = selection(&host, "default", r.path(), &command);
        grant(&mut host, r.path(), &[pin]);
        let mut s = open(r.path(), host, "protocol");
        let mut connection = ConnectionState::default();
        connection
            .message_session(
                &mut s,
                profile,
                br#"{"jsonrpc":"2.0","id":0,"method":"initialize"}"#,
            )
            .unwrap();
        let name = if profile == Profile::Session {
            "ci_session"
        } else {
            "activate_project"
        };
        for forged in [
            json!({"allowProjectActivation":true}),
            json!({"activationRuntime":{"trusted":true}}),
            json!({"command":"private-canary"}),
            json!({"timeoutMs":"deadline-cleaned"}),
            json!({"root":"/"}),
            json!({"session_id":"foreign"}),
        ] {
            let mut args = if profile == Profile::Session {
                json!({"operation":"activate","args":{"project":"default"}})
            } else {
                json!({"project":"default","session_id":"protocol"})
            };
            let target = if profile == Profile::Session {
                args["args"].as_object_mut().unwrap()
            } else {
                args.as_object_mut().unwrap()
            };
            target.extend(forged.as_object().unwrap().clone());
            let message = json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":name,"arguments":args}});
            let response = connection
                .message_session(&mut s, profile, &serde_json::to_vec(&message).unwrap())
                .unwrap()
                .unwrap();
            assert!(response.get("error").is_some() || response["result"]["isError"] == true);
            assert_eq!(count(r.path()), 1);
        }
        let args = if profile == Profile::Session {
            json!({"operation":"activate","args":{"project":"default"}})
        } else {
            json!({"project":"default","session_id":"protocol"})
        };
        let message = json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":name,"arguments":args}});
        let response = connection
            .message_session(&mut s, profile, &serde_json::to_vec(&message).unwrap())
            .unwrap()
            .unwrap();
        assert!(
            response.get("error").is_none() && response["result"]["isError"] == false,
            "{response}"
        );
        assert_eq!(count(r.path()), 2);
    }
}
fn profile_host(command: &str) -> HostProfile {
    profile(command, json!(180))
}

fn cli(root: &Path, args: &[String], input: &[u8]) -> yoke::process::CapturedOutput {
    let mut command = Command::new(env!("CARGO_BIN_EXE_yoke"));
    command.current_dir(root).args(args);
    yoke::process::run_capture(
        command,
        input,
        &Deadline::new(30000),
        yoke::process::CaptureLimits {
            stdin_bytes: 128000,
            stdout_bytes: 4_000_000,
            stderr_bytes: 64000,
        },
    )
    .unwrap()
}
fn cli_files(root: &Path) -> (PathBuf, PathBuf) {
    let command = append();
    let host = profile(&command, json!(180));
    let pin = selection(&host, "default", root, &command);
    let profile_path = root.join("host.json");
    let selection_path = root.join("activation-selection.json");
    fs::write(&profile_path, serde_json::to_vec(&host).unwrap()).unwrap();
    fs::write(&selection_path, serde_json::to_vec(&vec![pin]).unwrap()).unwrap();
    (profile_path, selection_path)
}
#[test]
fn actual_cli_separates_passive_inspection_host_opt_in_and_model_request_fields() {
    let r = root();
    let (host, selection) = cli_files(r.path());
    let requests = r.path().join("requests.json");
    fs::write(
        &requests,
        serde_json::to_vec(&json!([{"operation":"activate","args":{"project":"default"}}]))
            .unwrap(),
    )
    .unwrap();
    let base = vec![
        "--root".into(),
        r.path().display().to_string(),
        "session".into(),
        "--id".into(),
        "cli".into(),
        "--host-profile".into(),
        host.display().to_string(),
        "--requests".into(),
        requests.display().to_string(),
    ];
    for flags in [
        vec![],
        vec!["--allow-project-activation".into()],
        vec![
            "--activation-selection".into(),
            selection.display().to_string(),
        ],
    ] {
        let mut args = base.clone();
        args.extend(flags);
        let result = cli(r.path(), &args, &[]);
        assert_ne!(result.exit_code, Some(0));
        assert!(result.stdout.is_empty());
        assert_eq!(count(r.path()), 0);
    }
    let mut authorized = base.clone();
    authorized.extend([
        "--allow-project-activation".into(),
        "--activation-selection".into(),
        selection.display().to_string(),
    ]);
    fs::write(
        &requests,
        serde_json::to_vec(&json!([{"operation":"configuration","args":{}}])).unwrap(),
    )
    .unwrap();
    let passive = cli(r.path(), &authorized, &[]);
    assert_eq!(
        passive.exit_code,
        Some(0),
        "{}",
        String::from_utf8_lossy(&passive.stderr)
    );
    assert_eq!(count(r.path()), 0);
    let value: Value = serde_json::from_slice(&passive.stdout).unwrap();
    assert!(value["results"][0]["activeProject"].is_null());
    fs::write(&requests,serde_json::to_vec(&json!([{"operation":"activate","args":{"project":"default","activationRuntime":{"trusted":true}}}])).unwrap()).unwrap();
    let forged = cli(r.path(), &base, &[]);
    assert_ne!(forged.exit_code, Some(0));
    assert_eq!(count(r.path()), 0);
    fs::write(
        &requests,
        serde_json::to_vec(&json!([{"operation":"activate","args":{"project":"default"}}]))
            .unwrap(),
    )
    .unwrap();
    let actual = cli(r.path(), &authorized, &[]);
    assert_eq!(
        actual.exit_code,
        Some(0),
        "{}",
        String::from_utf8_lossy(&actual.stderr)
    );
    assert!(actual.output_complete);
    assert_eq!(count(r.path()), 2);
    let value: Value = serde_json::from_slice(&actual.stdout).unwrap();
    assert!(value["sourceWritten"].is_null());
    assert_eq!(value["activationExternalEffectsPossible"], true);
    let public = String::from_utf8_lossy(&actual.stdout);
    assert!(
        !public.contains("private-output")
            && !public.contains("private-error")
            && !public.contains(&append())
    );
}

#[test]
fn actual_cli_stdio_native_and_serena_lifecycles_share_the_host_selection() {
    for profile in ["native", "serena"] {
        let r = root();
        let (host, selection) = cli_files(r.path());
        let (name, args) = if profile == "native" {
            (
                "ci_session",
                json!({"operation":"activate","args":{"project":"default"}}),
            )
        } else {
            (
                "activate_project",
                json!({"project":"default","session_id":"activation-stdio"}),
            )
        };
        let input = format!(
            "{}\n{}\n",
            json!({"jsonrpc":"2.0","id":1,"method":"initialize"}),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":name,"arguments":args}})
        );
        // Keep host process arguments separate from the JSON request argument.
        let host_args = vec![
            "--root".into(),
            r.path().display().to_string(),
            "--allow-project-activation".into(),
            "--activation-selection".into(),
            selection.display().to_string(),
            "serve-session".into(),
            "--id".into(),
            "activation-stdio".into(),
            "--profile".into(),
            profile.into(),
            "--host-profile".into(),
            host.display().to_string(),
        ];
        let output = cli(r.path(), &host_args, input.as_bytes());
        assert_eq!(
            output.exit_code,
            Some(0),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.output_complete);
        assert_eq!(count(r.path()), 2);
        let responses = String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .collect::<Vec<_>>();
        let response = responses.iter().find(|v| v["id"] == 2).unwrap();
        assert_eq!(response["result"]["isError"], false, "{response}");
    }
}

fn http(
    address: std::net::SocketAddr,
    method: &str,
    session: Option<&str>,
    body: Option<Value>,
) -> (u16, std::collections::BTreeMap<String, String>, Value) {
    use std::io::{Read, Write};
    let body = body
        .map(|v| serde_json::to_vec(&v).unwrap())
        .unwrap_or_default();
    let session = session
        .map(|s| format!("Mcp-Session-Id: {s}\r\nMcp-Protocol-Version: 2025-11-25\r\n"))
        .unwrap_or_default();
    let header = format!(
        "{method} /mcp HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nAccept: application/json, text/event-stream\r\n{session}Content-Length: {}\r\n\r\n",
        hex::encode([64; 32]),
        body.len()
    );
    let mut socket =
        std::net::TcpStream::connect_timeout(&address, Duration::from_secs(5)).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    socket
        .set_write_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    socket.write_all(header.as_bytes()).unwrap();
    socket.write_all(&body).unwrap();
    let mut bytes = Vec::new();
    loop {
        let mut chunk = [0u8; 8192];
        let n = socket.read(&mut chunk).unwrap();
        assert!(n > 0);
        bytes.extend_from_slice(&chunk[..n]);
        assert!(bytes.len() <= 1_000_000);
        if let Some(end) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
            let text = std::str::from_utf8(&bytes[..end]).unwrap();
            let status = text.split_whitespace().nth(1).unwrap().parse().unwrap();
            let headers = text
                .split("\r\n")
                .skip(1)
                .filter_map(|line| line.split_once(':'))
                .map(|(k, v)| (k.to_ascii_lowercase(), v.trim().to_owned()))
                .collect::<std::collections::BTreeMap<_, _>>();
            let length: usize = headers["content-length"].parse().unwrap();
            assert!(length <= 900000);
            if bytes.len() >= end + 4 + length {
                assert_eq!(bytes.len(), end + 4 + length);
                let body = &bytes[end + 4..];
                return (
                    status,
                    headers,
                    if body.is_empty() {
                        Value::Null
                    } else {
                        serde_json::from_slice(body).unwrap()
                    },
                );
            }
        }
    }
}
#[test]
fn actual_http_initialization_and_explicit_activation_keep_protocol_authority_separate() {
    use yoke::{
        mcp_http::{self, Principal, Target},
        protocol::Profile,
    };
    for profile in [Profile::Session, Profile::Serena] {
        let r = root();
        let command = append();
        let mut host = profile_host(&command);
        let pin = selection(&host, "default", r.path(), &command);
        grant(&mut host, r.path(), &[pin]);
        assert!(
            Principal::new(
                "native",
                zeroize::Zeroizing::new(vec![64; 32]),
                Target::new(r.path(), Profile::Native, host.clone())
            )
            .is_err()
        );
        assert_eq!(count(r.path()), 0);
        let principal = Principal::new(
            "host",
            zeroize::Zeroizing::new(vec![64; 32]),
            Target::new(r.path(), profile, host),
        )
        .unwrap();
        assert_eq!(count(r.path()), 0, "Principal admission must be passive");
        let server = mcp_http::serve(mcp_http::Config::default(), vec![principal]).unwrap();
        let address = server.local_addr();
        let (status, headers, value) = http(
            address,
            "POST",
            None,
            Some(
                json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"owned-activation","version":"1"}}}),
            ),
        );
        assert_eq!(status, 200, "{value}");
        let sid = &headers["mcp-session-id"];
        assert_eq!(count(r.path()), 1);
        assert_eq!(
            http(
                address,
                "POST",
                Some(sid),
                Some(json!({"jsonrpc":"2.0","method":"notifications/initialized"}))
            )
            .0,
            202
        );
        let (name, args) = if profile == Profile::Session {
            (
                "ci_session",
                json!({"operation":"activate","args":{"project":"default"}}),
            )
        } else {
            // The public owning Session label is independent of the transport
            // secret. Obtain it through the same public configuration surface.
            let (status, _, response) = http(
                address,
                "POST",
                Some(sid),
                Some(
                    json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"get_current_config","arguments":{}}}),
                ),
            );
            assert_eq!(status, 200);
            assert_eq!(response["result"]["isError"], false, "{response}");
            let configuration: Value =
                serde_json::from_str(response["result"]["content"][0]["text"].as_str().unwrap())
                    .unwrap();
            let label = configuration["data"]["sessionId"].as_str().unwrap();
            assert_ne!(label, sid);
            let (_, _, refused) = http(
                address,
                "POST",
                Some(sid),
                Some(
                    json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"activate_project","arguments":{"project":"default","session_id":sid}}}),
                ),
            );
            assert_eq!(refused["result"]["isError"], true, "{refused}");
            assert_eq!(count(r.path()), 1);
            (
                "activate_project",
                json!({"project":"default","session_id":label}),
            )
        };
        let (_, _, response) = http(
            address,
            "POST",
            Some(sid),
            Some(
                json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":name,"arguments":args}}),
            ),
        );
        assert_eq!(response["result"]["isError"], false, "{response}");
        assert_eq!(count(r.path()), 2);
        assert!(!response.to_string().contains("private-output"));
        assert_eq!(http(address, "DELETE", Some(sid), None).0, 204);
        server.shutdown().unwrap();
    }
}

#[test]
fn actual_configuration_discovery_redacts_activation_bodies_without_running_them() {
    let r = root();
    let command = append();
    let input = r.path().join("project.yml");
    fs::write(&input, "activation_command_timeout: 1.5\n").unwrap();
    let selected = r.path().join("discovery.json");
    let host = profile(&command, json!(180));
    fs::write(&selected,serde_json::to_vec(&json!({"hostProfile":host,"files":[{"path":input,"target":{"kind":"project","project":"default"}}]})).unwrap()).unwrap();
    let args = vec![
        "session-config".into(),
        "discover".into(),
        "--selection".into(),
        selected.display().to_string(),
        "--allow-configuration-read".into(),
        "--allow-policy-projection".into(),
    ];
    let output = cli(r.path(), &args, &[]);
    assert_eq!(
        output.exit_code,
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(!text.contains(&command));
    let value: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(value["selectionReplayable"], false);
    assert_eq!(value["projection"]["activationCommandsExecuted"], false);
    assert_eq!(count(r.path()), 0);
    assert!(!r.path().join(".yoke").exists());
}

#[test]
fn absent_and_null_activation_keep_exact_default_session_rows_and_configuration_bytes() {
    let r = root();
    let mut s = open(r.path(), HostProfile::default(), "legacy-exact");
    let before = config(&mut s);
    let text: String = s
        .engine
        .store
        .db
        .query_row(
            "SELECT value FROM records WHERE kind=?1",
            [RECORD_KIND],
            |r| r.get(0),
        )
        .unwrap();
    drop(s);
    let host: HostProfile = serde_json::from_value(json!({"activationCommands":null})).unwrap();
    assert_eq!(
        serde_json::to_vec(&host).unwrap(),
        serde_json::to_vec(&HostProfile::default()).unwrap()
    );
    let mut resumed = open(r.path(), host, "legacy-exact");
    assert_eq!(
        serde_json::to_vec(&config(&mut resumed)).unwrap(),
        serde_json::to_vec(&before).unwrap()
    );
    let after: String = resumed
        .engine
        .store
        .db
        .query_row(
            "SELECT value FROM records WHERE kind=?1",
            [RECORD_KIND],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(after, text);
    assert_eq!(count(r.path()), 0);
}

#[test]
fn same_physical_root_alias_does_not_transfer_a_grant_between_registered_projects() {
    let a = root();
    let b = root();
    let command = append();
    let mut host:HostProfile=serde_json::from_value(json!({"projects":[{"name":"alpha","root":b.path().canonicalize().unwrap()},{"name":"beta","root":b.path().canonicalize().unwrap().join(".")}],"activationCommands":{"projects":{"alpha":{"project":{"activation_command":command}},"beta":{"project":{"activation_command":command}}}}})).unwrap();
    let pin = selection(&host, "alpha", b.path(), &command);
    grant(&mut host, a.path(), &[pin]);
    let mut s = open(a.path(), host, "aliases");
    assert!(
        s.invoke("activate", &json!({"project":"beta"}), &d())
            .is_err()
    );
    assert_eq!(count(b.path()), 0);
    s.invoke("activate", &json!({"project":"alpha"}), &d())
        .unwrap();
    assert_eq!(count(b.path()), 1);
    assert!(
        s.invoke("activate", &json!({"project":"beta"}), &d())
            .is_err()
    );
    assert_eq!(count(b.path()), 1);
    assert_eq!(config(&mut s)["activeProject"], "alpha");
}

#[cfg(unix)]
#[test]
fn linked_root_selection_is_not_a_physical_root_grant() {
    let r = root();
    let actual = r.path().join("actual");
    fs::create_dir(&actual).unwrap();
    let alias = r.path().join("alias");
    std::os::unix::fs::symlink(&actual, &alias).unwrap();
    let command = append();
    let host = profile(&command, json!(180));
    let mut pin = selection(&host, "default", &actual, &command);
    pin.root = alias;
    assert!(Runtime::authorize(&host, &actual, &[pin], &d()).is_err());
    assert_eq!(count(&actual), 0);
}

#[test]
fn explicitly_authorized_shell_effects_do_not_rewrite_immutable_captured_source_or_core_grants() {
    let r = root();
    let original = fs::read(r.path().join("main.rs")).unwrap();
    let command = if cfg!(windows) {
        "[IO.File]::WriteAllText((Join-Path (Get-Location).Path 'main.rs'), \"fn changed() {}`n\", [Text.UTF8Encoding]::new($false)); exit 7"
    } else {
        "printf 'fn changed() {}\\n' > main.rs; exit 7"
    };
    let command = owned_command(command);
    let mut host = profile(&command, json!(180));
    host.primary_read_only = true;
    let pin = selection(&host, "default", r.path(), &command);
    grant(&mut host, r.path(), &[pin]);
    let mut engine = Engine::open(r.path()).unwrap();
    let (snapshot, _, _, _) = engine.index(&d()).unwrap();
    let file = snapshot
        .files
        .iter()
        .find(|f| f.path == "main.rs")
        .unwrap()
        .clone();
    let mut s = Session::open(engine, host, Some("immutable"), &d()).unwrap();
    assert_eq!(
        fs::read(r.path().join("main.rs")).unwrap(),
        b"fn changed() {}\n"
    );
    assert_eq!(
        s.engine
            .store
            .read_blob_exact(&file.hash, file.size, 4_000_000)
            .unwrap(),
        original
    );
    assert_eq!(
        s.engine
            .store
            .snapshot(&snapshot.id)
            .unwrap()
            .files
            .iter()
            .find(|f| f.path == "main.rs")
            .unwrap()
            .hash,
        file.hash
    );
    assert!(!s.engine.allow_writes);
    assert!(s.require_edit_permission().is_err());
    assert_eq!(receipt(&mut s)["observation"]["exitCode"], 7);
}

#[test]
fn repl_facade_reads_and_forged_fields_cannot_implicitly_run_another_activation() {
    let r = root();
    let command = append();
    let mut host = profile(&command, json!(180));
    let pin = selection(&host, "default", r.path(), &command);
    grant(&mut host, r.path(), &[pin]);
    let mut s = open(r.path(), host, "facade");
    assert_eq!(count(r.path()), 1);
    for (method, args) in [
        ("cfg.get_current_config", json!({})),
        ("fs.read_file", json!({"relative_path":"main.rs"})),
    ] {
        let value = yoke::agent_repl::invoke_facade(&mut s, method, args, None, &d()).unwrap();
        assert!(!value.to_string().contains("private-output"));
        assert_eq!(count(r.path()), 1);
    }
    assert!(
        yoke::agent_repl::invoke_facade(
            &mut s,
            "cfg.get_current_config",
            json!({"allowProjectActivation":true}),
            None,
            &d()
        )
        .is_err()
    );
    assert_eq!(count(r.path()), 1);
    s.invoke("activate", &json!({"project":"default"}), &d())
        .unwrap();
    yoke::agent_repl::invoke_facade(
        &mut s,
        "fs.read_file",
        json!({"relative_path":"main.rs"}),
        None,
        &d(),
    )
    .unwrap();
    assert_eq!(count(r.path()), 2);
}

fn backend_engine(root: &Path, log: &Path) -> Engine {
    let name = if cfg!(windows) { "node.exe" } else { "node" };
    let executable = std::env::split_paths(
        &std::env::var_os("PATH").expect("Explicit host Node fixture prerequisite"),
    )
    .filter(|p| p.is_absolute())
    .map(|p| p.join(name))
    .find(|p| p.is_file())
    .expect("Actual host Node for labeled LSP protocol fixture")
    .canonicalize()
    .unwrap();
    let mut engine = Engine::open(root).unwrap();
    engine
        .register_configured_server(yoke::lsp::ConfiguredServer {
            server: yoke::lsp::Server {
                command: executable.display().to_string(),
                args: vec![
                    Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("tests/fixtures/activation-order-lsp.mjs")
                        .display()
                        .to_string(),
                    root.join("activation.log").display().to_string(),
                    log.display().to_string(),
                ],
                language: "rust".into(),
                trust: true,
            },
            configuration: yoke::lsp::HostConfiguration {
                passive_profile: Some(yoke::lsp::PassiveProfile::ContractFixture),
                ..Default::default()
            },
        })
        .unwrap();
    engine
}
fn backend_hover(session: &mut Session) -> u64 {
    let value = session
        .active_engine_at(&d())
        .unwrap()
        .invoke(
            "ci_find",
            json!({"languageServer":{"operation":"hover","path":"main.rs","offset":0}}),
        )
        .unwrap();
    assert_eq!(value["status"], "ok", "{value}");
    let observed: Value = serde_json::from_str(
        value["data"]["response"]["contents"]["value"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    observed["hookLines"].as_u64().unwrap()
}
#[test]
fn real_backend_initialization_observes_initial_repeated_and_resumed_hooks_in_order() {
    let r = root();
    let external = tempfile::tempdir().unwrap();
    let log = external.path().join("backend.jsonl");
    let command = append();
    let mut host = profile(&command, json!(180));
    assert!(
        Session::open(
            backend_engine(r.path(), &log),
            host.clone(),
            Some("ungranted-backend"),
            &d()
        )
        .is_err()
    );
    assert!(!log.exists());
    assert_eq!(count(r.path()), 0);
    let pin = selection(&host, "default", r.path(), &command);
    grant(&mut host, r.path(), &[pin]);
    let mut s = Session::open(
        backend_engine(r.path(), &log),
        host.clone(),
        Some("backend-order"),
        &d(),
    )
    .unwrap();
    assert!(!log.exists());
    assert_eq!(backend_hover(&mut s), 1);
    s.invoke("activate", &json!({"project":"default"}), &d())
        .unwrap();
    assert_eq!(backend_hover(&mut s), 2);
    drop(s);
    let mut inspected = Session::inspect(
        backend_engine(r.path(), &log),
        host.clone(),
        Some("backend-order"),
        &d(),
    )
    .unwrap();
    assert!(inspected.active_engine_at(&d()).is_err());
    assert_eq!(count(r.path()), 2);
    drop(inspected);
    let mut resumed = Session::open(
        backend_engine(r.path(), &log),
        host,
        Some("backend-order"),
        &d(),
    )
    .unwrap();
    assert_eq!(backend_hover(&mut resumed), 3);
    let events = fs::read_to_string(log)
        .unwrap()
        .lines()
        .map(|line| {
            serde_json::from_str::<Value>(line).unwrap()["hookLines"]
                .as_u64()
                .unwrap()
        })
        .collect::<Vec<_>>();
    assert_eq!(events, vec![1, 2, 3]);
}
