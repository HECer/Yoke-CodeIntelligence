use super::*;

fn counted(reset: bool) -> (tempfile::TempDir, Session) {
    counted_with_policy(reset, false)
}
fn counted_with_policy(reset: bool, denied: bool) -> (tempfile::TempDir, Session) {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("main.rs"),
        "// owned 🌍\r\nfn example() {}\r\n",
    )
    .unwrap();
    let mut host = json!({"tokenCountEstimator":"CHAR_COUNT","steering":{},
        "dashboard":{"management":{"toolUsageReset":reset,"sessionControls":false,"memories":false}}});
    if denied {
        host["steering"]["global"] = json!({"excludedTools":["ci_session"]});
    }
    let host: HostProfile = serde_json::from_value(host).unwrap();
    let session = Session::open(
        Engine::open(root.path()).unwrap(),
        host,
        Some("counted"),
        &d(),
    )
    .unwrap();
    (root, session)
}
fn count(session: &mut Session) {
    yoke::compatibility::invoke_serena(
        session,
        "read_file",
        &json!({"relative_path":"main.rs"}),
        &d(),
    )
    .unwrap();
}
fn reset(revision: &Value) -> Value {
    json!({"kind":"resetToolUsage","expectedStatisticsRevision":revision})
}

#[test]
fn exact_revision_reset_changes_only_owned_ephemeral_totals_under_independent_grant() {
    let (root, mut session) = counted(true);
    let original = fs::read(root.path().join("main.rs")).unwrap();
    let (address, token) = start(&mut session);
    count(&mut session);
    let before = view(address, &token);
    assert_eq!(before["capabilities"]["toolUsageStatistics"], true);
    assert_eq!(before["capabilities"]["toolUsageReset"], true);
    assert_eq!(before["view"]["management"]["sessionControls"], false);
    let revision = before["view"]["toolUsageStatisticsDisplay"]["revision"].clone();
    assert_eq!(
        before["view"]["toolUsageStatisticsDisplay"]["tools"]["read_file"]["numTimesCalled"],
        "1"
    );
    let (status, reply) = call(&mut session, address, &token, reset(&revision));
    assert_eq!(status, 200, "{reply}");
    assert_eq!(reply["sourceWritten"], false);
    assert_eq!(reply["metadataChanged"], false);
    assert_eq!(reply["toolUsageChanged"], true);
    let after = view(address, &token);
    assert!(
        after["view"]["toolUsageStatisticsDisplay"]["tools"]
            .as_object()
            .unwrap()
            .is_empty()
    );
    assert_ne!(
        after["view"]["toolUsageStatisticsDisplay"]["revision"],
        revision
    );
    assert_eq!(after["view"]["revision"], before["view"]["revision"]);
    assert_eq!(
        after["view"]["management"]["sessionStateSha256"],
        before["view"]["management"]["sessionStateSha256"]
    );
    assert_eq!(fs::read(root.path().join("main.rs")).unwrap(), original);
}

#[test]
fn counters_added_after_review_and_replayed_reset_requests_refuse_without_clearing() {
    let (_root, mut session) = counted(true);
    let (address, token) = start(&mut session);
    count(&mut session);
    let reviewed = view(address, &token)["view"]["toolUsageStatisticsDisplay"]["revision"].clone();
    count(&mut session);
    let (status, reply) = call(&mut session, address, &token, reset(&reviewed));
    assert_eq!(status, 409);
    assert_eq!(reply["notApplied"], true);
    assert_eq!(
        view(address, &token)["view"]["toolUsageStatisticsDisplay"]["tools"]["read_file"]["numTimesCalled"],
        "2"
    );
    let current = view(address, &token)["view"]["toolUsageStatisticsDisplay"]["revision"].clone();
    assert_eq!(call(&mut session, address, &token, reset(&current)).0, 200);
    assert_eq!(call(&mut session, address, &token, reset(&current)).0, 409);
    count(&mut session);
    assert_eq!(call(&mut session, address, &token, reset(&reviewed)).0, 409);
    assert_eq!(
        view(address, &token)["view"]["toolUsageStatisticsDisplay"]["tools"]["read_file"]["numTimesCalled"],
        "1"
    );
}

#[test]
fn read_only_statistics_do_not_imply_reset_authority_or_affect_another_instance() {
    let (_root, mut session) = counted(false);
    let (address, token) = start(&mut session);
    count(&mut session);
    let before = view(address, &token);
    assert_eq!(before["capabilities"]["toolUsageReset"], false);
    let revision = &before["view"]["toolUsageStatisticsDisplay"]["revision"];
    let (status, reply) = call(&mut session, address, &token, reset(revision));
    assert_eq!(status, 403);
    assert_eq!(reply["notApplied"], true);
    assert_eq!(
        view(address, &token)["view"]["toolUsageStatisticsDisplay"],
        before["view"]["toolUsageStatisticsDisplay"]
    );

    let (other_root, mut other) = counted(true);
    let (other_address, other_token) = start(&mut other);
    count(&mut other);
    let other_revision =
        view(other_address, &other_token)["view"]["toolUsageStatisticsDisplay"]["revision"].clone();
    assert_eq!(
        call(
            &mut other,
            other_address,
            &other_token,
            reset(&other_revision)
        )
        .0,
        200
    );
    assert_eq!(
        view(address, &token)["view"]["toolUsageStatisticsDisplay"],
        before["view"]["toolUsageStatisticsDisplay"]
    );
    assert!(
        fs::read(other_root.path().join("main.rs"))
            .unwrap()
            .starts_with(b"// owned ")
    );
}

#[test]
fn malformed_numeric_unicode_extra_path_and_foreign_project_requests_refuse() {
    let (_root, mut session) = counted(true);
    let (address, token) = start(&mut session);
    count(&mut session);
    let before = view(address, &token);
    for revision in [
        json!("00"),
        json!("+1"),
        json!("１"),
        json!("18446744073709551616"),
        json!(1),
    ] {
        assert_eq!(call(&mut session, address, &token, reset(&revision)).0, 400);
    }
    let mut action = reset(&before["view"]["toolUsageStatisticsDisplay"]["revision"]);
    action["sourcePath"] = json!("../../foreign");
    assert_eq!(call(&mut session, address, &token, action).0, 400);
    // Project labels have an existing ASCII grammar. Keep malformed Unicode
    // refusal separate from a valid label bound to another project.
    for (project, expected_status) in [("other🌍", 400u16), ("other", 409)] {
        let mut body = request(
            &before,
            reset(&before["view"]["toolUsageStatisticsDisplay"]["revision"]),
        );
        body["project"] = json!(project);
        let bytes = wire(address, &token, body);
        let client = thread::spawn(move || raw(address, bytes));
        let until = Instant::now() + Duration::from_secs(10);
        while !client.is_finished() {
            session.poll_dashboard_management(&d()).unwrap();
            assert!(Instant::now() < until);
            thread::sleep(Duration::from_millis(1));
        }
        let (status, reply) = client.join().unwrap();
        assert_eq!(status, expected_status);
        if expected_status == 409 {
            assert_eq!(reply["notApplied"], true);
        }
        assert_eq!(
            view(address, &token)["view"]["toolUsageStatisticsDisplay"],
            before["view"]["toolUsageStatisticsDisplay"]
        );
    }
}

#[test]
fn omitted_reset_grant_preserves_default_serialization_and_invalid_types_refuse() {
    let absent: yoke::session_dashboard::ManagementConfig =
        serde_json::from_value(json!({})).unwrap();
    let disabled: yoke::session_dashboard::ManagementConfig =
        serde_json::from_value(json!({"toolUsageReset":false})).unwrap();
    assert_eq!(absent, disabled);
    assert!(
        serde_json::to_value(absent)
            .unwrap()
            .get("toolUsageReset")
            .is_none()
    );
    for value in [Value::Null, json!("true"), json!(1), json!({})] {
        assert!(
            serde_json::from_value::<yoke::session_dashboard::ManagementConfig>(
                json!({"toolUsageReset":value})
            )
            .is_err()
        );
    }
}

#[test]
fn explicit_session_tool_exclusion_refuses_reset_and_preserves_counters() {
    let (root, mut session) = counted_with_policy(true, true);
    let original = fs::read(root.path().join("main.rs")).unwrap();
    let (address, token) = start(&mut session);
    count(&mut session);
    let before = view(address, &token);
    assert_eq!(before["capabilities"]["toolUsageReset"], false);
    let revision = &before["view"]["toolUsageStatisticsDisplay"]["revision"];
    let (status, reply) = call(&mut session, address, &token, reset(revision));
    assert_eq!(status, 403);
    assert_eq!(reply["notApplied"], true);
    assert_eq!(
        view(address, &token)["view"]["toolUsageStatisticsDisplay"],
        before["view"]["toolUsageStatisticsDisplay"]
    );
    assert_eq!(fs::read(root.path().join("main.rs")).unwrap(), original);
}
