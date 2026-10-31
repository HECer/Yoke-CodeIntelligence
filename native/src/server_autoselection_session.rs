//! Existing Session metadata owns creation choices; no provider starts here.
use super::*;
use crate::server_autoselection::{self, Decision};

pub(super) fn validate_choices(
    host: &HostProfile,
    choices: &BTreeMap<String, Decision>,
) -> Result<()> {
    server_autoselection::validate_retained(choices)?;
    if choices.is_empty() {
        return Ok(());
    }
    let policy = host
        .language_server_auto_selection
        .as_ref()
        .context("language_auto_host_selection_missing")?;
    for (name, choice) in choices {
        ensure!(
            name == "default" || host.projects.iter().any(|p| p.name == *name),
            "language_auto_project_not_host_registered"
        );
        ensure!(
            choice
                .backend_id
                .as_ref()
                .is_none_or(|id| policy.priority(id).is_some_and(|p| p > 0)),
            "language_auto_decision_excluded_backend"
        );
    }
    Ok(())
}
impl Session {
    pub(super) fn automatic_language_applies(&self, name: &str) -> bool {
        self.host.language_server_auto_selection.is_some()
            && self
                .host
                .language_servers
                .as_ref()
                .and_then(|s| s.select(name))
                .is_none()
            && !self.state.language_server_overrides.contains_key(name)
    }
    pub(super) fn ensure_automatic_language(
        &mut self,
        name: &str,
        deadline: &Deadline,
    ) -> Result<()> {
        let Some(policy) = self.host.language_server_auto_selection.clone() else {
            return Ok(());
        };
        if !self.automatic_language_applies(name) {
            return Ok(());
        }
        deadline.check()?;
        let choice = if let Some(choice) = self.state.automatic_language_servers.get(name) {
            choice.clone()
        } else {
            ensure!(
                !self.inspection_only,
                "language_auto_inspection_requires_initialized_selection"
            );
            self.check_current_state(
                deadline,
                "Session changed before automatic language selection",
            )?;
            let engine = if name == "default" {
                &mut self.engine
            } else {
                self.others
                    .get_mut(name)
                    .context("Registered project unavailable")?
            };
            let choice = server_autoselection::detect(engine, &policy, deadline)?;
            let mut next = self.state.clone();
            next.automatic_language_servers
                .insert(name.to_owned(), choice.clone());
            validate_choices(&self.host, &next.automatic_language_servers)?;
            self.update(next, deadline)?;
            choice
        };
        let engine = if name == "default" {
            &mut self.engine
        } else {
            self.others
                .get_mut(name)
                .context("Registered project unavailable")?
        };
        choice.apply(engine)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn setup(granted: bool) -> (tempfile::TempDir, Session) {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("main.rs"), "fn owned() {}\n").unwrap();
        std::fs::write(root.path().join("owned.py"), "def owned(): pass\n").unwrap();
        let mut host: HostProfile = serde_json::from_value(
            json!({"languageServerAutoSelection":{"priorities":{"python":0}}}),
        )
        .unwrap();
        if granted {
            host.dashboard=Some(serde_json::from_value(json!({"management":{"languageServers":{"backendIds":["rust","python"],"legacyLanguages":["rust"]}}})).unwrap());
        } else {
            host.dashboard = Some(serde_json::from_value(json!({})).unwrap());
        }
        let mut engine = Engine::open(root.path()).unwrap();
        for id in ["rust", "python"] {
            let server = crate::lsp::ConfiguredServer {
                server: crate::lsp::Server {
                    language: id.into(),
                    command: std::env::current_exe().unwrap().display().to_string(),
                    args: vec![],
                    trust: true,
                },
                configuration: crate::lsp::HostConfiguration {
                    allow_source_execution: true,
                    ..Default::default()
                },
            };
            if id == "rust" {
                engine.register_configured_server(server.clone()).unwrap();
            }
            engine.register_backend(id, server).unwrap();
        }
        let session =
            Session::open(engine, host, Some("dashboard-auto"), &Deadline::new(20000)).unwrap();
        (root, session)
    }
    #[test]
    fn granted_dashboard_backend_and_legacy_overrides_reset_to_the_original_creation_choice() {
        use crate::session_dashboard::management::LanguageSelection;
        let (_root, mut session) = setup(true);
        let d = Deadline::new(20000);
        let original = session.state.automatic_language_servers.clone();
        assert_eq!(
            session.engine.selected_backend_id("main.rs").unwrap(),
            Some("rust")
        );
        session
            .dashboard_select_languages(
                Some(LanguageSelection::Backends {
                    ids: vec!["python".into()],
                }),
                &d,
            )
            .unwrap();
        assert!(!session.engine.automatic_backend_selection);
        assert_eq!(
            session.engine.selected_backend_id("main.rs").unwrap(),
            Some("python")
        );
        session.dashboard_select_languages(None, &d).unwrap();
        assert!(session.engine.automatic_backend_selection);
        assert_eq!(
            session.engine.selected_backend_id("main.rs").unwrap(),
            Some("rust")
        );
        assert_eq!(
            session.engine.selected_backend_id("owned.py").unwrap(),
            None
        );
        session
            .dashboard_select_languages(
                Some(LanguageSelection::Legacy {
                    languages: vec!["rust".into()],
                }),
                &d,
            )
            .unwrap();
        assert!(!session.engine.automatic_backend_selection);
        assert!(
            session
                .engine
                .semantic_host("main.rs", "rust")
                .unwrap()
                .is_some()
        );
        session.dashboard_select_languages(None, &d).unwrap();
        assert_eq!(session.state.automatic_language_servers, original);
        session.check_tool_state(&d).unwrap();
    }
    #[test]
    fn auto_selection_never_grants_dashboard_authority_or_ungiven_backend_ids() {
        use crate::session_dashboard::management::LanguageSelection;
        for granted in [false, true] {
            let (_root, mut session) = setup(granted);
            let view = session.dashboard_view().unwrap();
            assert_eq!(view["selectedBackends"], json!(["rust"]));
            assert_eq!(view["automaticLanguageSelectionActive"], true);
            assert_eq!(view["automaticLanguageSelectionInitialized"], true);
            let before = session.persisted.clone();
            let id = if granted { "graphql" } else { "python" };
            assert!(
                session
                    .dashboard_select_languages(
                        Some(LanguageSelection::Backends {
                            ids: vec![id.into()]
                        }),
                        &Deadline::new(20000)
                    )
                    .is_err()
            );
            assert_eq!(session.persisted, before);
            assert!(session.engine.automatic_backend_selection);
        }
    }
    #[test]
    fn retained_decision_schema_refuses_unknown_backends_and_cannot_grant_source_history() {
        let (_root, session) = setup(false);
        let value = serde_json::to_value(&session.state).unwrap();
        super::super::validate_retention_record(&session.engine.store, &session.state.id, &value)
            .unwrap();
        for key in ["backendId", "filenameSetHash"] {
            let mut altered = value.clone();
            altered["automaticLanguageServers"]["default"][key] = json!("unknown");
            assert!(
                super::super::validate_retention_record(
                    &session.engine.store,
                    &session.state.id,
                    &altered
                )
                .is_err()
            );
        }
        let mut altered = value.clone();
        altered["automaticLanguageServers"]["default"]["matchedFiles"] = json!(true);
        assert!(
            super::super::validate_retention_record(
                &session.engine.store,
                &session.state.id,
                &altered
            )
            .is_err()
        );
    }
    #[test]
    fn filename_metadata_equal_to_a_real_snapshot_never_retains_that_source() {
        let (root, mut session) = setup(false);
        let d = Deadline::new(20000);
        let old = session.engine.index(&d).unwrap().0.id;
        std::fs::write(root.path().join("main.rs"), "fn changed() {}\n").unwrap();
        let current = session.engine.index(&d).unwrap().0.id;
        assert_ne!(old, current);
        let mut next = session.state.clone();
        next.automatic_language_servers
            .get_mut("default")
            .unwrap()
            .filename_set_hash = old.clone();
        session.update(next, &d).unwrap();
        let plan = crate::retention::plan(
            &session.engine.store,
            &crate::retention::Policy::default(),
            &d,
        )
        .unwrap();
        assert_eq!(plan["snapshotIds"], json!([old]));
        let id = plan["planId"].as_str().unwrap();
        crate::retention::apply(&session.engine.store, id, id, true, &d).unwrap();
        assert!(session.engine.store.snapshot(&old).is_err());
        assert!(session.engine.store.snapshot(&current).is_ok());
        assert_eq!(
            std::fs::read(root.path().join("main.rs")).unwrap(),
            b"fn changed() {}\n"
        );
    }
    #[test]
    fn individually_bounded_selection_maps_cannot_publish_an_unresumable_session() {
        use crate::session_dashboard::management::LanguageSelection;
        let (_root, mut session) = setup(false);
        let d = Deadline::new(20000);
        let before = session.persisted.clone();
        let revision = session.state.revision;
        let mut next = session.state.clone();
        let decision = next.automatic_language_servers["default"].clone();
        let ids = [
            "typescript",
            "typescript_vts",
            "typescript_tsgo",
            "python",
            "python_basedpyright",
            "python_pyrefly",
            "csharp",
            "csharp_omnisharp",
            "ruby",
            "ruby_solargraph",
            "graphql",
        ];
        for n in 0..100 {
            let name = format!("p{n:03}{}", "x".repeat(96));
            next.onboarded_projects.insert(name.clone());
            next.removed_projects.insert(name.clone());
            next.automatic_language_servers
                .insert(name.clone(), decision.clone());
            next.language_server_overrides.insert(
                name,
                LanguageSelection::Backends {
                    ids: ids.iter().map(|id| (*id).into()).collect(),
                },
            );
        }
        server_autoselection::validate_retained(&next.automatic_language_servers).unwrap();
        super::super::dashboard_languages::validate_retained(&next.language_server_overrides)
            .unwrap();
        assert!(serde_json::to_vec(&next).unwrap().len() > 64000);
        assert!(
            session
                .update(next, &d)
                .unwrap_err()
                .to_string()
                .contains("capacity")
        );
        assert_eq!(session.persisted, before);
        assert_eq!(session.state.revision, revision);
        session.check_current_state(&d, "unchanged").unwrap();
    }
}
