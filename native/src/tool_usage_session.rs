//! Count-only ephemeral statistics; existing Session freshness remains authoritative.
use super::*;

impl Session {
    pub(super) fn next_tool_usage_revision(&self) -> Result<u64> {
        self.tool_usage_revision
            .checked_add(1)
            .context("tool_usage_statistics_revision_overflow")
    }
    pub(crate) fn preflight_tool_usage(
        &self,
        name: &str,
        args: &Value,
        deadline: &Deadline,
    ) -> Result<()> {
        if let Some(stats) = &self.tool_usage {
            self.require_serena_tool(name, deadline)?;
            self.next_tool_usage_revision()?;
            stats.preflight(name, args, deadline)?;
        }
        Ok(())
    }
    pub(crate) fn record_tool_usage(
        &mut self,
        name: &str,
        args: &Value,
        result: &mut Value,
        deadline: &Deadline,
    ) -> Result<()> {
        if result["status"] == "error" {
            return Ok(());
        }
        if self.tool_usage.is_some() {
            let outcome = (|| -> Result<()> {
                self.check_tool_state(deadline)?;
                let revision = self.next_tool_usage_revision()?;
                if self.tool_usage.as_ref().expect("Selected immutable statistics policy").annotate_native_result(result) {
                    let inventory = crate::compatibility::contracts();
                    let method = inventory["tools"].as_array().expect("Owned tool inventory")
                        .iter().find(|tool| tool["name"] == name).and_then(|tool| tool["facadeMethod"].as_str()).unwrap_or("");
                    if let Some(limit) = crate::tool_budgets::answer_limit(self.tool_budgets().as_ref(), method, args)? {
                        ensure!(serde_json::to_string(result)?.chars().count() as u64 <= limit,
                            "Result with token-count disclosure metadata exceeds max_answer_chars={limit}");
                    }
                }
                let measured = self
                    .tool_usage
                    .as_ref()
                    .expect("Selected immutable statistics policy")
                    .measure_checked(name, args, result, deadline, &|| {
                        self.check_tool_state(deadline)
                    })?;
                self.check_tool_state(deadline)?;
                self.tool_usage
                    .as_mut()
                    .expect("Selected immutable statistics policy")
                    .commit(measured, deadline)?;
                self.tool_usage_revision = revision;
                Ok(())
            })()
            .context("tool_usage_statistics_failed_after_operation_no_rollback");
            if let Some(handle) = &self.dashboard {
                handle.finish(
                    None,
                    outcome.is_err(),
                    self.persisted.clone(),
                    self.dashboard_view(),
                );
            }
            outcome?;
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool_usage::{
        Estimator,
        http::{Host, Spec},
    };
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    #[test]
    fn exhausted_ephemeral_statistics_revision_refuses_before_counting_or_clear() {
        let root = tempfile::tempdir().unwrap();
        let bytes = b"fn owned() {}\n";
        std::fs::write(root.path().join("main.rs"), bytes).unwrap();
        let deadline = Deadline::new(20000);
        let host = HostProfile {
            token_count_estimator: Some(Estimator::CharCount),
            ..Default::default()
        };
        let mut session = Session::open(
            Engine::open(root.path()).unwrap(),
            host,
            Some("revision-limit"),
            &deadline,
        )
        .unwrap();
        session.tool_usage_revision = u64::MAX;
        let before = session.tool_usage.as_ref().unwrap().view();
        assert!(
            session
                .preflight_tool_usage("read_file", &json!({"relative_path":"main.rs"}), &deadline)
                .unwrap_err()
                .to_string()
                .contains("statistics_revision_overflow")
        );
        assert!(
            session
                .invoke("clearToolUsage", &json!({}), &deadline)
                .is_err()
        );
        let mut result = json!({"text":"owned"});
        assert!(
            session
                .record_tool_usage("read_file", &json!({}), &mut result, &deadline)
                .is_err()
        );
        assert_eq!(session.tool_usage.as_ref().unwrap().view(), before);
        assert_eq!(session.tool_usage_revision, u64::MAX);
        assert_eq!(std::fs::read(root.path().join("main.rs")).unwrap(), bytes);
    }

    #[test]
    fn disclosure_metadata_cannot_bypass_an_existing_exact_answer_limit() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("main.rs"), "fn owned() {}\n").unwrap();
        let runtime = Host::new(
            Spec {
                endpoint: "https://api.anthropic.com/v1/messages/count_tokens".into(),
                credential_name: "owned".into(),
                allow_loopback_http: false,
                additional_root_certificates: vec![],
            },
            true,
            Arc::new(|_| anyhow::bail!("owned_reader_must_not_run")),
        )
        .unwrap();
        let host = HostProfile {
            token_count_estimator: Some(Estimator::AnthropicClaudeSonnet4),
            token_count_runtime: Some(runtime.clone()),
            ..Default::default()
        };
        let deadline = Deadline::new(20000);
        let mut session = Session::open(
            Engine::open(root.path()).unwrap(),
            host,
            Some("answer-limit"),
            &deadline,
        )
        .unwrap();
        let mut result = json!({"schemaVersion":1,"status":"partial","data":{},"metrics":{"localOnly":true,"externalUsage":"none","serializedBytes":30,"estimatedTokens":10,"tokenMethod":"utf8_bytes_div_3"}});
        let exact = serde_json::to_string(&result).unwrap().chars().count();
        let error = session
            .record_tool_usage(
                "read_file",
                &json!({"relative_path":"main.rs","max_answer_chars":exact}),
                &mut result,
                &deadline,
            )
            .unwrap_err();
        assert!(error.to_string().contains("after_operation_no_rollback"));
        assert!(format!("{error:#}").contains("max_answer_chars"));
        assert_eq!(runtime.attempts(), 0);
        assert!(
            session.tool_usage.as_ref().unwrap().view()["tools"]
                .as_object()
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn changed_or_revoked_after_success_refuses_before_disclosure_with_no_rollback_context() {
        for revoke in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let bytes = b"fn owned() {}\n";
            std::fs::write(root.path().join("main.rs"), bytes).unwrap();
            let reads = Arc::new(AtomicUsize::new(0));
            let counted = reads.clone();
            let runtime = Host::new(
                Spec {
                    endpoint: "https://api.anthropic.com/v1/messages/count_tokens".into(),
                    credential_name: "owned".into(),
                    allow_loopback_http: false,
                    additional_root_certificates: vec![],
                },
                true,
                Arc::new(move |_| {
                    counted.fetch_add(1, Ordering::SeqCst);
                    anyhow::bail!("owned_credential_reader_must_not_run")
                }),
            )
            .unwrap();
            let host = HostProfile {
                token_count_estimator: Some(Estimator::AnthropicClaudeSonnet4),
                token_count_runtime: Some(runtime.clone()),
                ..Default::default()
            };
            let deadline = Deadline::new(20000);
            let mut session = Session::open(
                Engine::open(root.path()).unwrap(),
                host.clone(),
                Some("changed"),
                &deadline,
            )
            .unwrap();
            let mut other = Session::open(
                Engine::open(root.path()).unwrap(),
                host,
                Some("changed"),
                &deadline,
            )
            .unwrap();
            let mut result = session
                .invoke("fs.read", &json!({"path":"main.rs"}), &deadline)
                .unwrap();
            if revoke {
                runtime.revoke();
            } else {
                other
                    .invoke("switchModes", &json!({"modes":["planning"]}), &deadline)
                    .unwrap();
            }
            let error = session
                .record_tool_usage(
                    "read_file",
                    &json!({"relative_path":"main.rs"}),
                    &mut result,
                    &deadline,
                )
                .unwrap_err();
            assert!(error.to_string().contains("after_operation_no_rollback"));
            assert_eq!(reads.load(Ordering::SeqCst), 0);
            assert_eq!(runtime.attempts(), 0);
            assert!(
                session.tool_usage.as_ref().unwrap().view()["tools"]
                    .as_object()
                    .unwrap()
                    .is_empty()
            );
            assert_eq!(std::fs::read(root.path().join("main.rs")).unwrap(), bytes);
        }
    }
}
