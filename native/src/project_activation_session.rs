//! Session integration. The last bounded receipt is private state metadata, not
//! a transaction or rollback journal for arbitrary shell effects.
use super::*;
use crate::project_activation::{self, Diagnostic};

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Phase {
    Attempting,
    Activated,
    OutcomeUnknown,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Receipt {
    schema: String,
    attempt_id: String,
    session_key: String,
    project: String,
    command_sha256: String,
    configuration_sha256: String,
    authority_sha256: String,
    root_sha256: String,
    session_revision_before: u64,
    phase: Phase,
    #[serde(skip_serializing_if = "Option::is_none")]
    observation: Option<Observation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    refusal: Option<Refusal>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Refusal {
    CaptureRefused,
    CleanupUnproven,
    StaleAfterAttempt,
    PublicationUncertain,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Observation {
    exit_code: Option<i32>,
    command_timed_out: bool,
    requested_timeout_ns: u64,
    effective_timeout_ns: u64,
    stdout_bytes: usize,
    stderr_bytes: usize,
    stdout_sha256: String,
    stderr_sha256: String,
    child_tree_cleanup_proven: bool,
    streams_complete: bool,
}
impl From<&Diagnostic> for Observation {
    fn from(d: &Diagnostic) -> Self {
        Self {
            exit_code: d.exit_code,
            command_timed_out: d.timed_out,
            requested_timeout_ns: d.requested_timeout_ns,
            effective_timeout_ns: d.effective_timeout_ns,
            stdout_bytes: d.stdout.len(),
            stderr_bytes: d.stderr.len(),
            stdout_sha256: crate::model::hash(&d.stdout),
            stderr_sha256: crate::model::hash(&d.stderr),
            child_tree_cleanup_proven: true,
            streams_complete: true,
        }
    }
}
pub(super) struct Attempt {
    key: String,
    persisted: String,
    receipt: Receipt,
}
fn key(id: &str) -> String {
    format!("{}/{}", project_activation::VERSION, id)
}
fn raw(engine: &Engine, id: &str) -> Result<Option<String>> {
    session_bytes(&engine.store.db, id)
}
fn session_bytes(db: &rusqlite::Connection, id: &str) -> Result<Option<String>> {
    let bytes: Option<Vec<u8>> = db
        .query_row(
            "SELECT substr(CAST(value AS BLOB),1,64001) FROM records WHERE kind=?1 AND id=?2",
            params![RECORD_KIND, id],
            |r| r.get(0),
        )
        .optional()?;
    bytes
        .map(|bytes| {
            ensure!(bytes.len() <= 64000, "activation_session_capacity");
            String::from_utf8(bytes)
                .map_err(|_| anyhow::anyhow!("activation_session_encoding_invalid"))
        })
        .transpose()
}
fn check(engine: &Engine, id: &str, expected: Option<&str>, d: &Deadline) -> Result<()> {
    d.check()?;
    engine.store.set_deadline(d)?;
    ensure!(
        raw(engine, id)?.as_deref() == expected,
        "activation_session_changed"
    );
    // Independently reopen the selected existing namespace. A shell replacing
    // .yoke must not let an old SQLite connection publish to detached state.
    let current = crate::store::Store::open_existing_read_only(&engine.root, d)?;
    ensure!(
        current.workspace_id == engine.store.workspace_id,
        "activation_state_namespace_changed"
    );
    let named = session_bytes(&current.db, id)?;
    ensure!(
        named.as_deref() == expected,
        "activation_named_session_changed"
    );
    Ok(())
}
pub(super) fn check_selected(
    engine: &Engine,
    host: &HostProfile,
    state: &State,
    expected: Option<&str>,
    d: &Deadline,
) -> Result<()> {
    let Some(config) = &host.activation_commands else {
        return Ok(());
    };
    if config.resolve(&state.active_project)?.command.is_none() {
        return Ok(());
    }
    let root = if state.active_project == "default" {
        &engine.root
    } else {
        &host
            .projects
            .iter()
            .find(|p| p.name == state.active_project)
            .context("activation_project_not_available")?
            .root
    };
    host.activation_runtime
        .as_ref()
        .context("activation_host_permission_required")?
        .check(config, &state.active_project, root, d)?;
    check(engine, &state.id, expected, d)
}
fn read_receipt(engine: &Engine, id: &str, d: &Deadline) -> Result<Option<(String, Receipt)>> {
    d.check()?;
    engine.store.set_deadline(d)?;
    let value: Option<String> = engine
        .store
        .db
        .query_row(
            "SELECT value FROM meta WHERE key=?1 AND length(CAST(value AS BLOB))<=8192",
            [key(id)],
            |r| r.get(0),
        )
        .optional()?;
    let exists: bool = engine.store.db.query_row(
        "SELECT EXISTS(SELECT 1 FROM meta WHERE key=?1)",
        [key(id)],
        |r| r.get(0),
    )?;
    ensure!(!exists || value.is_some(), "activation_receipt_capacity");
    value
        .map(|text| {
            let receipt: Receipt = crate::json::decode(crate::json::parse(text.as_bytes(), 8192)?)?;
            ensure!(
                receipt.schema == project_activation::VERSION
                    && receipt.session_key == id
                    && [
                        &receipt.attempt_id,
                        &receipt.command_sha256,
                        &receipt.configuration_sha256,
                        &receipt.authority_sha256,
                        &receipt.root_sha256
                    ]
                    .iter()
                    .all(|v| valid_id(v)),
                "activation_receipt_invalid"
            );
            label(&receipt.project)?;
            ensure!(
                match receipt.phase {
                    Phase::Attempting => receipt.observation.is_none() && receipt.refusal.is_none(),
                    Phase::Activated => receipt.observation.is_some() && receipt.refusal.is_none(),
                    Phase::OutcomeUnknown => receipt.refusal.is_some(),
                },
                "activation_receipt_phase_invalid"
            );
            if let Some(o) = &receipt.observation {
                ensure!(
                    o.stdout_bytes <= 64000
                        && o.stderr_bytes <= 16000
                        && o.requested_timeout_ns > 0
                        && o.effective_timeout_ns > 0
                        && o.effective_timeout_ns <= o.requested_timeout_ns
                        && o.child_tree_cleanup_proven
                        && o.streams_complete
                        && valid_id(&o.stdout_sha256)
                        && valid_id(&o.stderr_sha256),
                    "activation_receipt_invalid"
                );
            }
            Ok((text, receipt))
        })
        .transpose()
}
fn check_named_receipt(
    engine: &Engine,
    id: &str,
    expected: Option<&str>,
    d: &Deadline,
) -> Result<()> {
    d.check()?;
    let current = crate::store::Store::open_existing_read_only(&engine.root, d)?;
    ensure!(
        current.workspace_id == engine.store.workspace_id,
        "activation_state_namespace_changed"
    );
    let value: Option<String> = current
        .db
        .query_row(
            "SELECT value FROM meta WHERE key=?1 AND length(CAST(value AS BLOB))<=8192",
            [key(id)],
            |r| r.get(0),
        )
        .optional()?;
    let exists: bool = current.db.query_row(
        "SELECT EXISTS(SELECT 1 FROM meta WHERE key=?1)",
        [key(id)],
        |r| r.get(0),
    )?;
    ensure!(
        !exists || value.is_some(),
        "activation_named_receipt_capacity"
    );
    ensure!(
        value.as_deref() == expected,
        "activation_named_attempt_changed"
    );
    Ok(())
}
pub(super) fn activated_witness(engine: &Engine, id: &str, d: &Deadline) -> Result<Option<String>> {
    let row = read_receipt(engine, id, d)?;
    ensure!(
        row.as_ref()
            .is_none_or(|(_, r)| matches!(r.phase, Phase::Activated)),
        "activation_prior_outcome_unknown_no_automatic_retry"
    );
    let witness = row.map(|(text, _)| text);
    check_named_receipt(engine, id, witness.as_deref(), d)?;
    Ok(witness)
}
impl Attempt {
    fn check_named(&self, engine: &Engine, d: &Deadline) -> Result<()> {
        check_named_receipt(engine, &self.receipt.session_key, Some(&self.persisted), d)
    }
    fn publish(
        &mut self,
        engine: &Engine,
        phase: Phase,
        refusal: Option<Refusal>,
        d: &Deadline,
    ) -> Result<()> {
        d.check()?;
        engine.store.set_deadline(d)?;
        self.check_named(engine, d)?;
        self.receipt.phase = phase;
        self.receipt.refusal = refusal;
        let text = String::from_utf8(crate::json::encode_bounded(&self.receipt, 8192, d)?)?;
        let changed = engine.store.db.execute(
            "UPDATE meta SET value=?1 WHERE key=?2 AND value=?3",
            params![text, self.key, self.persisted],
        )?;
        ensure!(changed == 1, "activation_receipt_changed_outcome_unknown");
        self.persisted = text;
        self.check_named(engine, d)?;
        Ok(())
    }
    pub(super) fn activated(mut self, engine: &Engine, d: &Deadline) -> Result<String> {
        self.publish(engine, Phase::Activated, None, d)
            .context("activation_state_publication_may_have_completed")?;
        Ok(self.persisted)
    }
    pub(super) fn uncertain(mut self, engine: &Engine, d: &Deadline) {
        // Best effort bookkeeping must never replace the original CAS/capture
        // error. If this fails, the durable Attempting receipt remains unknown.
        let _ = self.publish(
            engine,
            Phase::OutcomeUnknown,
            Some(Refusal::PublicationUncertain),
            d,
        );
    }
}
pub(super) fn run(
    engine: &Engine,
    host: &HostProfile,
    state: &State,
    expected: Option<&str>,
    project: &str,
    d: &Deadline,
) -> Result<Option<(Attempt, Diagnostic)>> {
    let Some(config) = &host.activation_commands else {
        return Ok(None);
    };
    let selected = config.resolve(project)?;
    let Some(command) = selected.command else {
        return Ok(None);
    };
    let runtime = host
        .activation_runtime
        .as_ref()
        .context("activation_host_permission_required")?;
    let root = if project == "default" {
        &engine.root
    } else {
        &host
            .projects
            .iter()
            .find(|p| p.name == project && !p.query_only)
            .context("activation_project_not_available")?
            .root
    };
    ensure!(
        !state.removed_projects.contains(project),
        "Project removed from session"
    );
    runtime.check(config, project, root, d)?;
    check(engine, &state.id, expected, d)?;
    ensure!(
        d.remaining() > project_activation::ACTIVATION_RESERVE,
        "activation_enclosing_budget_insufficient"
    );
    let prior = read_receipt(engine, &state.id, d)?;
    ensure!(
        prior
            .as_ref()
            .is_none_or(|(_, r)| matches!(r.phase, Phase::Activated)),
        "activation_prior_outcome_unknown_no_automatic_retry"
    );
    let mut nonce = [0u8; 16];
    getrandom::fill(&mut nonce).map_err(|_| anyhow::anyhow!("activation_nonce_unavailable"))?;
    let receipt = Receipt {
        schema: project_activation::VERSION.into(),
        attempt_id: identity(&nonce),
        session_key: state.id.clone(),
        project: project.into(),
        command_sha256: crate::model::hash(command),
        configuration_sha256: config.fingerprint(),
        authority_sha256: runtime.binding().into(),
        root_sha256: identity(root),
        session_revision_before: state.revision,
        phase: Phase::Attempting,
        observation: None,
        refusal: None,
    };
    let text = String::from_utf8(crate::json::encode_bounded(&receipt, 8192, d)?)?;
    // A short reservation CAS prevents cooperating sessions with the same
    // state from both entering the irreversible attempt. No SQL lock spans it.
    let tx = engine.store.db.unchecked_transaction()?;
    let current = session_bytes(&tx, &state.id)?;
    ensure!(current.as_deref() == expected, "activation_session_changed");
    if let Some((previous, _)) = prior {
        ensure!(
            tx.execute(
                "UPDATE meta SET value=?1 WHERE key=?2 AND value=?3",
                params![text, key(&state.id), previous]
            )? == 1,
            "activation_attempt_raced"
        );
    } else {
        ensure!(
            tx.execute(
                "INSERT OR IGNORE INTO meta(key,value) VALUES(?1,?2)",
                params![key(&state.id), text]
            )? == 1,
            "activation_attempt_raced"
        );
    }
    tx.commit()?;
    let mut attempt = Attempt {
        key: key(&state.id),
        persisted: text,
        receipt,
    };
    let result = (|| {
        check(engine, &state.id, expected, d)?;
        attempt.check_named(engine, d)?;
        runtime.check(config, project, root, d)?;
        runtime.run(config, project, root, d)
    })();
    let diagnostic = match result {
        Ok(output) => output,
        Err(error) => {
            let cleanup_unproven = crate::process::cleanup_unproven(&error);
            let _ = attempt.publish(
                engine,
                Phase::OutcomeUnknown,
                Some(if cleanup_unproven {
                    Refusal::CleanupUnproven
                } else {
                    Refusal::CaptureRefused
                }),
                d,
            );
            return Err(error.context(format!(
                "activation_attempt_{}_refused_external_effects_possible",
                attempt.receipt.attempt_id
            )));
        }
    };
    attempt.receipt.observation = Some(Observation::from(&diagnostic));
    if let Err(error) = runtime
        .check(config, project, root, d)
        .and_then(|()| check(engine, &state.id, expected, d))
        .and_then(|()| attempt.check_named(engine, d))
    {
        let _ = attempt.publish(
            engine,
            Phase::OutcomeUnknown,
            Some(Refusal::StaleAfterAttempt),
            d,
        );
        return Err(error.context("activation_attempt_outcome_unknown_external_effects_possible"));
    }
    if diagnostic.timed_out {
        eprintln!(
            "project_activation: project={project} attempt={} command_timeout; tree cleanup and streams proven; activation pending",
            attempt.receipt.attempt_id
        );
    } else if diagnostic.exit_code != Some(0) {
        eprintln!(
            "project_activation: project={project} attempt={} nonzero_exit={:?}; tree cleanup and streams proven; activation pending",
            attempt.receipt.attempt_id, diagnostic.exit_code
        );
    }
    Ok(Some((attempt, diagnostic)))
}
impl Session {
    pub fn activation_effects_possible(&self) -> bool {
        !self.activation_ready_projects.is_empty() || self.activation_blocked
    }
    pub(super) fn admit_activation_engine(&self, project: &str, d: &Deadline) -> Result<()> {
        ensure!(
            !self.activation_blocked,
            "activation_outcome_unknown_backend_refused"
        );
        let Some(config) = &self.host.activation_commands else {
            return Ok(());
        };
        if self.inspection_only {
            ensure!(
                self.inspection_source_read,
                "activation_inspection_backend_unavailable"
            );
            return Ok(());
        }
        // Even a command-free project must reject a newer attempt from another
        // handle. The private pending witness is installed only after run()
        // has proved capture completion and cleanup, before this handle opens its backend.
        let current = read_receipt(&self.engine, &self.state.id, d)?;
        ensure!(
            current.as_ref().map(|(text, _)| text.as_str())
                == self.activation_receipt_witness.as_deref(),
            "activation_receipt_changed_backend_refused"
        );
        check_named_receipt(
            &self.engine,
            &self.state.id,
            self.activation_receipt_witness.as_deref(),
            d,
        )?;
        if config.resolve(project)?.command.is_none() {
            return Ok(());
        }
        ensure!(
            self.activation_ready_projects.contains(project),
            "activation_required_before_backend_use_explicitly_activate_project"
        );
        let root = if project == "default" {
            &self.engine.root
        } else {
            &self
                .host
                .projects
                .iter()
                .find(|p| p.name == project)
                .context("activation_project_not_available")?
                .root
        };
        self.host
            .activation_runtime
            .as_ref()
            .context("activation_host_permission_required")?
            .check(config, project, root, d)
    }
    /// Explicit private Rust host access. No protocol operation exposes raw logs.
    pub fn take_activation_diagnostic(&mut self) -> Option<Diagnostic> {
        self.activation_diagnostic.take()
    }
    pub(super) fn activation_view(&self, d: &Deadline) -> Result<Option<Value>> {
        let Some(config) = &self.host.activation_commands else {
            return Ok(None);
        };
        let selected = config.resolve(&self.state.active_project)?;
        let receipt = read_receipt(&self.engine, &self.state.id, d)?.map(|(_, r)| r);
        Ok(Some(
            json!({"contract":project_activation::VERSION,"configurationSha256":config.fingerprint(),"commandSelected":selected.command.is_some(),
            "commandSha256":selected.command.map(crate::model::hash),"requestedTimeoutNs":u64::try_from(selected.timeout.as_nanos())?,
            "hostAuthorityPresent":self.host.activation_runtime.is_some(),"inspectionOnly":self.inspection_only,"lastAttempt":receipt,
            "externalEffectsTransactional":false,"automaticRetry":false}),
        ))
    }
    pub(super) fn activate_selected(&mut self, next: State, d: &Deadline) -> Result<()> {
        ensure!(
            !self.inspection_only,
            "activation_inspection_session_read_only"
        );
        let project = next.active_project.clone();
        let attempt = match run(
            &self.engine,
            &self.host,
            &self.state,
            Some(&self.persisted),
            &project,
            d,
        ) {
            Ok(attempt) => attempt,
            Err(error) => {
                self.process_cleanup_unproven |= crate::process::cleanup_unproven(&error);
                self.activation_blocked |= self.process_cleanup_unproven
                    || match read_receipt(&self.engine, &self.state.id, d) {
                        Ok(Some((_, r))) => {
                            matches!(r.phase, Phase::Attempting | Phase::OutcomeUnknown)
                        }
                        Ok(None) => false,
                        Err(_) => true,
                    };
                return Err(error);
            }
        };
        let previously_ready = self.activation_ready_projects.contains(&project);
        if let Some((attempt, _)) = &attempt {
            self.activation_ready_projects.insert(project.clone());
            self.activation_receipt_witness = Some(attempt.persisted.clone());
        }
        let result = (|| {
            self.project_engine(&project, d)?;
            // Recheck root/config/session after fresh engine admission too.
            if let Some(config) = &self.host.activation_commands
                && config.resolve(&project)?.command.is_some()
            {
                let root = if project == "default" {
                    &self.engine.root
                } else {
                    &self
                        .host
                        .projects
                        .iter()
                        .find(|p| p.name == project)
                        .context("activation_project_not_available")?
                        .root
                };
                self.host
                    .activation_runtime
                    .as_ref()
                    .context("activation_host_permission_required")?
                    .check(config, &project, root, d)?;
                self.check_current_state(d, "activation_session_changed")?;
            }
            self.update(next, d)?;
            check_selected(
                &self.engine,
                &self.host,
                &self.state,
                Some(&self.persisted),
                d,
            )
        })();
        match (result, attempt) {
            (Ok(()), Some((attempt, diagnostic))) => {
                self.activation_diagnostic = Some(diagnostic);
                let result = attempt.activated(&self.engine, d).map(|witness| {
                    self.activation_receipt_witness = Some(witness);
                });
                self.activation_blocked |= result.is_err();
                result
            }
            (Err(error), Some((attempt, _))) => {
                self.activation_blocked = true;
                if !previously_ready {
                    self.activation_ready_projects.remove(&project);
                }
                attempt.uncertain(&self.engine, d);
                Err(error
                    .context("activation_publication_outcome_unknown_external_effects_possible"))
            }
            (result, None) => result,
        }
    }
}
