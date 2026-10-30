//! Fresh source replay plus an immutable imported provider observation. No
//! network, credential read, model, execution or service publication authority.
use super::{Options, PreviewRequest};
use crate::{
    capture::Deadline, github::discussions::Capture, github_app::HostProfile, model::hash,
    store::Store,
};
use anyhow::{Context as _, Result, ensure};
use serde_json::{Value, json};

pub enum Source<'a> {
    Current,
    History {
        git: &'a crate::git_workflows::HostGit,
        options: &'a super::HistoryOptions,
        mapping: Option<&'a super::MappingSelection>,
    },
}
#[derive(Default)]
pub struct Permission {
    pub allow_discussions: bool,
    pub allow_fixture: bool,
    pub handoff_preview: bool,
}
pub struct Context<'a> {
    pub store: &'a Store,
    pub profile: &'a HostProfile,
    pub options: &'a Options,
    pub permission: &'a Permission,
    pub source: Source<'a>,
}
pub fn authorize(
    profile: &HostProfile,
    request: &PreviewRequest,
    capture: &Capture,
    permission: &Permission,
    d: &Deadline,
) -> Result<()> {
    super::authorize_profile(profile, request)?;
    ensure!(
        permission.allow_discussions,
        "github_app_digest_discussion_permission_required"
    );
    capture.validate(d)?;
    ensure!(
        capture.payload.repository_id == request.repository_id
            && capture
                .payload
                .repository
                .eq_ignore_ascii_case(&request.repository),
        "github_app_digest_discussion_scope_mismatch"
    );
    ensure!(
        permission.allow_fixture || capture.payload.transport != "explicit_host_loopback_fixture",
        "github_app_digest_fixture_discussions_denied"
    );
    Ok(())
}
fn association(value: &Value, head: &str) -> Value {
    let mut snapshots = vec![];
    if value["sourceCommitVerified"] == true
        && value["sourceCommitId"]
            .as_str()
            .is_some_and(|id| id.eq_ignore_ascii_case(head))
    {
        snapshots.push(value["snapshotId"].clone());
    }
    if let Some(rows) = value["historyMapping"]["snapshots"].as_array() {
        for row in rows {
            if row["sourceCommitVerified"] == true
                && row["sourceCommitId"]
                    .as_str()
                    .is_some_and(|id| id.eq_ignore_ascii_case(head))
                && !snapshots.contains(&row["snapshotId"])
            {
                snapshots.push(row["snapshotId"].clone());
            }
        }
    }
    let in_history = value["commits"].as_array().is_some_and(|rows| {
        rows.iter().any(|row| {
            row["sha"]
                .as_str()
                .is_some_and(|id| id.eq_ignore_ascii_case(head))
        })
    });
    json!({"status":if !snapshots.is_empty(){"reported_head_matches_locally_verified_capture"}else if in_history{"verified_commit_without_selected_source_capture"}else{"unavailable"},
        "reportedHeadSha":head,"snapshotIds":snapshots,"remoteRepositoryVerified":false,"compilerBindingEstablished":false,"runtimeDispatchEstablished":false})
}
fn fields(value: &Value, names: &[&str]) -> Value {
    let mut result = serde_json::Map::new();
    for key in names {
        if let Some(item) = value.get(*key) {
            result.insert((*key).into(), item.clone());
        }
    }
    Value::Object(result)
}
pub fn preview(
    c: &Context<'_>,
    request: &PreviewRequest,
    capture: &Capture,
    d: &Deadline,
) -> Result<Value> {
    authorize(c.profile, request, capture, c.permission, d)?;
    let mut value = match &c.source {
        Source::Current => super::preview_with_profile(c.store, c.profile, c.options, request, d)?,
        Source::History {
            git,
            options,
            mapping: Some(selection),
        } => super::preview_mapped_history_with_profile(
            c.store, c.profile, c.options, request, git, options, selection, d,
        )?,
        Source::History {
            git,
            options,
            mapping: None,
        } => super::preview_history_with_profile(
            c.store, c.profile, c.options, request, git, options, d,
        )?,
    };
    d.check()?;
    let source_digest_hash = hash(crate::json::encode_bounded(
        &value,
        c.options.max_output_bytes,
        d,
    )?);
    let mut threads = vec![];
    for pull in &capture.payload.pulls {
        d.check()?;
        let binding = association(&value, &pull.head_sha);
        if c.permission.handoff_preview {
            let observations = pull
                .observations
                .iter()
                .map(|item| {
                    let row = serde_json::to_value(item)?;
                    Ok(fields(
                        &row,
                        &[
                            "kind",
                            "id",
                            "bodyHash",
                            "createdAt",
                            "updatedAt",
                            "submittedAt",
                            "commitId",
                            "originalCommitId",
                            "inReplyToId",
                            "reviewId",
                            "state",
                        ],
                    ))
                })
                .collect::<Result<Vec<_>>>()?;
            threads.push(json!({"number":pull.number,"providerId":pull.provider_id,"headSha":pull.head_sha,"baseSha":pull.base_sha,"closedAt":pull.closed_at,"updatedAt":pull.updated_at,"merged":pull.merged,"coverage":pull.coverage,"observations":observations,"sourceAssociation":binding}));
        } else {
            let mut row = serde_json::to_value(pull)?;
            row["sourceAssociation"] = binding;
            threads.push(row);
        }
    }
    if c.permission.handoff_preview {
        let symbols = value["symbols"]
            .as_array()
            .context("github_app_digest_source_unavailable")?
            .iter()
            .map(|row| {
                fields(
                    row,
                    &[
                        "path",
                        "name",
                        "kind",
                        "signature",
                        "fingerprint",
                        "startByte",
                        "endByte",
                        "producer",
                        "sourceHash",
                    ],
                )
            })
            .collect::<Vec<_>>();
        let payload = json!({"snapshotId":request.snapshot_id,"installationId":request.installation_id,"repositoryId":request.repository_id,"repository":request.repository,
            "sourceCommitId":value["sourceCommitId"],"sourceTreeId":value["sourceTreeId"],"sourceCommitVerified":value["sourceCommitVerified"],
            "sourceDigestHash":source_digest_hash,"discussionCaptureId":capture.id,"symbols":symbols,"threads":threads,
            "symbolCandidates":value["symbolCandidates"],"symbolOmissions":value["symbolOmissions"],"unsupportedFiles":value["unsupportedFiles"],"visibilityUnresolved":value["visibilityUnresolved"],
            "analyzer":value["analyzer"],"nativeEntityReplayScope":value["nativeEntityReplayScope"],
            "commits":value["commits"].as_array().map(|rows|rows.iter().map(|row|fields(row,&["sha","treeId","parentIds","committerTimestamp","committerUnixSeconds","filesStatus","symbolsStatus","mappedSnapshotId","parentSnapshotId"])).collect::<Vec<_>>()).unwrap_or_default(),
            "commitMessagesIncluded":false,
            "discussionCoverageComplete":capture.payload.coverage_complete,"unacquiredPulls":capture.payload.unacquired_pulls,
            "discussionAcquiredAtUnix":capture.payload.acquired_at_unix,
            "discussionTextIncluded":false,"implementationBodyScope":"top_level_function_bodies_and_const_static_initializers_excluded",
            "scope":"verified_captured_rust_headers_and_imported_discussion_metadata","providerVerified":false,"complete":false});
        let id = hash(crate::json::encode_bounded(
            &payload,
            c.options.max_output_bytes,
            d,
        )?);
        value = json!({"schemaVersion":1,"kind":"native_source_bound_digest_handoff_preview","id":id,"payload":payload,
            "reviewRequired":true,"serviceSend":"unavailable","networkRequests":0,"modelCalls":0,"sourceWritten":false});
    } else {
        value["kind"] = json!("native_captured_repository_digest_discussions_preview");
        value["threads"] = json!(threads);
        value["sourceDigestHash"] = json!(source_digest_hash);
        value["discussionCaptureId"] = json!(capture.id);
        value["discussionCoverageComplete"] = json!(capture.payload.coverage_complete);
        value["discussionUnacquiredPulls"] = json!(capture.payload.unacquired_pulls);
        value["discussionAcquiredAtUnix"] = json!(capture.payload.acquired_at_unix);
        value["providerAtomic"] = json!(false);
        value["providerCaptureAuthenticationVerified"] = json!(false);
        let limitations = value["limitations"]
            .as_array_mut()
            .context("github_app_digest_source_unavailable")?;
        limitations[2] = json!(
            "Other captured source bytes are verified, but their export policies are not replayed. Discussions are imported unsigned host observations, not provider identity or source authority."
        );
        limitations.push(json!("Provider pages and PR metadata are observations taken across separate requests, not an atomic snapshot. Null description bodies are observed absence; incomplete endpoint coverage and unacquired PRs remain explicit."));
        limitations.push(json!("Reported Git hashes can match locally verified commits/captures. That correspondence does not verify remote ownership, source byte equality of a discussion anchor, compiler binding or runtime dispatch. Discussion text remains untrusted data."));
    }
    crate::json::encode_bounded(&value, c.options.max_output_bytes, d)
        .context("github_app_digest_capacity_exceeded")?;
    d.check()?;
    Ok(value)
}
