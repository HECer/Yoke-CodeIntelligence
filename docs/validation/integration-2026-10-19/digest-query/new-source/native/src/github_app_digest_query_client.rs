//! Explicit read-only query client. It reads only the host service token;
//! no sender Store or App signing key is needed or opened.
use super::*;
use crate::github_app::{HostProfile, digest::service};
use std::{io::Read, net::SocketAddr};
use zeroize::Zeroizing;
pub struct Options {
    pub allow_query: bool,
    pub allow_network: bool,
    pub allow_fixture: bool,
    pub max_output_bytes: usize,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            allow_query: false,
            allow_network: false,
            allow_fixture: false,
            max_output_bytes: 32_000,
        }
    }
}
fn admit(
    profile: &service::Profile,
    app: &HostProfile,
    request: &Request,
    o: &Options,
    d: &Deadline,
) -> Result<()> {
    ensure!(
        o.allow_query && o.allow_network,
        "github_app_digest_query_permission_required"
    );
    d.check()?;
    request.validate()?;
    profile.validate()?;
    app.validate()?;
    ensure!((1024..=65_536).contains(&o.max_output_bytes), CAPACITY);
    ensure!(
        !profile.fixture()? || o.allow_fixture,
        "github_app_digest_query_fixture_denied"
    );
    let permits = |g: &crate::github_app::Grant| {
        g.installation_id == request.installation_id
            && g.repository_id == request.repository_id
            && g.repository == request.repository
    };
    ensure!(
        app.app_id == request.app_id
            && app.grants.iter().any(permits)
            && profile.grants.iter().any(permits),
        "github_app_digest_query_scope_denied"
    );
    Ok(())
}
/// A single HTTPS POST, with current host grants and OS service credential.
pub fn remote(
    profile: &service::Profile,
    app: &HostProfile,
    request: &Request,
    o: &Options,
    ca: Option<&[u8]>,
    d: &Deadline,
) -> Result<Value> {
    admit(profile, app, request, o, d)?;
    ensure!(
        !profile.fixture()?,
        "github_app_digest_query_fixture_constructor_required"
    );
    let token = crate::host_credentials::read_token(
        profile
            .credential_name
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!(INVALID))?,
    )
    .map_err(|_| anyhow::anyhow!("github_app_digest_query_credential_unavailable"))?;
    let client = service::Client::build(profile, Some(token), ca, d)?;
    call(&client, profile, request, o, d)
}
/// Owned numeric-loopback fixture only. Never reads a production credential.
pub fn for_fixture(
    profile: &service::Profile,
    app: &HostProfile,
    request: &Request,
    o: &Options,
    endpoint: SocketAddr,
    token: [u8; 32],
    d: &Deadline,
) -> Result<Value> {
    admit(profile, app, request, o, d)?;
    ensure!(
        o.allow_fixture
            && profile.fixture()?
            && endpoint.ip().is_loopback()
            && endpoint.port() != 0
            && profile.endpoint == format!("http://{endpoint}/"),
        INVALID
    );
    let client = service::Client::build(profile, Some(Zeroizing::new(token.to_vec())), None, d)?;
    call(&client, profile, request, o, d)
}
fn call(
    client: &service::Client,
    profile: &service::Profile,
    request: &Request,
    o: &Options,
    d: &Deadline,
) -> Result<Value> {
    let body = crate::json::encode_bounded(&serde_json::to_value(request)?, 8192, d)?;
    let token = Zeroizing::new(hex::encode(
        client
            .token
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!(INVALID))?
            .as_slice(),
    ));
    d.check()?;
    let mut response = client
        .http
        .post(format!("{}v1/digest-query", client.endpoint))
        .bearer_auth(token.as_str())
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .header("User-Agent", "yoke-code-intelligence/0.5")
        .timeout(d.remaining())
        .body(body)
        .send()
        .map_err(|_| anyhow::anyhow!("github_app_digest_query_transport_failed"))?;
    let refused = "github_app_digest_query_response_invalid";
    ensure!(
        response.status().as_u16() == 200
            && response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|s| s.to_str().ok())
                == Some("application/json")
            && response
                .content_length()
                .is_none_or(|n| n <= profile.max_response_bytes as u64),
        refused
    );
    let mut raw = vec![];
    let mut block = [0_u8; 4096];
    loop {
        d.check()?;
        let cap = (profile.max_response_bytes - raw.len() + 1).min(block.len());
        let n = response
            .read(&mut block[..cap])
            .map_err(|_| anyhow::anyhow!(refused))?;
        if n == 0 {
            break;
        }
        ensure!(raw.len() + n <= profile.max_response_bytes, refused);
        raw.extend_from_slice(&block[..n]);
    }
    let result = crate::json::parse(&raw, profile.max_response_bytes)
        .map_err(|_| anyhow::anyhow!(refused))?;
    validate_response(&result, request).map_err(|_| anyhow::anyhow!(refused))?;
    let value = json!({"schemaVersion":1,"kind":"native_received_digest_remote_query","endpoint":profile.endpoint,"networkRequests":1,"sourceAuthority":false,"sourceWritten":false,"modelCalls":0,"result":result});
    crate::json::encode_bounded(&value, o.max_output_bytes - 1, d)
        .map_err(|_| anyhow::anyhow!(CAPACITY))?;
    d.check()?;
    Ok(value)
}
fn validate_response(v: &Value, r: &Request) -> Result<()> {
    // Metadata is untrusted data. Only the closed protocol envelope carries
    // processing/authority claims, and every row remains receipt-qualified.
    closed(
        v,
        &[
            "schemaVersion",
            "kind",
            "processor",
            "requestHash",
            "receiverWorkspaceId",
            "manifests",
            "rows",
            "visitedRows",
            "matchingRows",
            "searchBytes",
            "offset",
            "limit",
            "hasMore",
            "completeForSelectedMetadata",
            "complete",
            "sourceAuthority",
            "modelCalls",
            "networkRequests",
            "sourceWritten",
            "recordsWritten",
            "limitations",
        ],
    )?;
    ensure!(
        v["schemaVersion"] == 1
            && v["kind"] == "native_received_digest_query"
            && v["processor"] == "literal_metadata_projection_v1"
            && v["requestHash"] == identity(r)
            && v["receiverWorkspaceId"].as_str().is_some_and(valid_id)
            && v["offset"] == r.offset
            && v["limit"] == r.limit
            && v["complete"] == false
            && v["sourceAuthority"] == false
            && v["sourceWritten"] == false
            && v["recordsWritten"] == false
            && v["modelCalls"] == 0
            && v["networkRequests"] == 0,
        BINDING
    );
    let visited = v["visitedRows"]
        .as_u64()
        .filter(|n| *n <= 32000)
        .ok_or_else(|| anyhow::anyhow!(BINDING))?;
    let matching = v["matchingRows"]
        .as_u64()
        .filter(|n| *n <= visited)
        .ok_or_else(|| anyhow::anyhow!(BINDING))? as usize;
    ensure!(
        v["searchBytes"].as_u64().is_some_and(|n| n <= 32_000_000),
        BINDING
    );
    let manifests = array(v, "manifests")?;
    ensure!(manifests.len() == r.proposal_ids.len(), BINDING);
    let mut selected = BTreeMap::new();
    for (m, id) in manifests.iter().zip(&r.proposal_ids) {
        closed(
            m,
            &[
                "proposalId",
                "receiptId",
                "payloadHash",
                "digestId",
                "principalId",
                "receivedAtUnix",
                "scope",
                "sourceDigestHash",
                "discussionCaptureId",
                "discussionCoverageComplete",
                "unacquiredPulls",
                "symbolCandidates",
                "symbolOmissions",
                "unsupportedFiles",
                "visibilityUnresolved",
                "receiverSourceVerification",
            ],
        )?;
        ensure!(
            m["proposalId"] == *id
                && [
                    "receiptId",
                    "payloadHash",
                    "digestId",
                    "sourceDigestHash",
                    "discussionCaptureId"
                ]
                .iter()
                .all(|k| m[*k].as_str().is_some_and(valid_id))
                && m["principalId"]
                    .as_str()
                    .is_some_and(crate::github_app::credential_name)
                && m["receivedAtUnix"].as_u64().is_some()
                && m["receiverSourceVerification"] == "unavailable_no_sender_cas",
            BINDING
        );
        let s = &m["scope"];
        closed(
            s,
            &[
                "appId",
                "senderWorkspaceId",
                "installationId",
                "repositoryId",
                "repository",
                "reportedSnapshotId",
            ],
        )?;
        ensure!(
            s["appId"] == r.app_id
                && s["senderWorkspaceId"] == r.sender_workspace_id
                && s["installationId"] == r.installation_id
                && s["repositoryId"] == r.repository_id
                && s["repository"] == r.repository
                && s["reportedSnapshotId"].as_str().is_some_and(valid_id),
            BINDING
        );
        selected.insert(id.as_str(), m);
    }
    let rows = array(v, "rows")?;
    ensure!(
        rows.len() == r.limit.min(matching.saturating_sub(r.offset)),
        BINDING
    );
    let more = r.offset.saturating_add(rows.len()) < matching;
    ensure!(
        v["hasMore"] == more && v["completeForSelectedMetadata"] == !more,
        BINDING
    );
    let mut ids = BTreeSet::new();
    for row in rows {
        closed(
            row,
            &[
                "id",
                "kind",
                "proposalId",
                "receiptId",
                "payloadHash",
                "pointer",
                "anchorGranularity",
                "metadata",
                "provenance",
                "sourceAuthority",
                "semanticIdentityEstablished",
            ],
        )?;
        let manifest = selected
            .get(
                row["proposalId"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!(BINDING))?,
            )
            .ok_or_else(|| anyhow::anyhow!(BINDING))?;
        let kind: Kind = crate::json::decode(row["kind"].clone())?;
        let pointer = row["pointer"]
            .as_str()
            .filter(|s| pointer_matches(kind, s))
            .ok_or_else(|| anyhow::anyhow!(BINDING))?;
        let receipt = manifest["receiptId"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!(BINDING))?;
        ensure!(
            r.kinds.contains(&kind)
                && row["receiptId"] == manifest["receiptId"]
                && row["payloadHash"] == manifest["payloadHash"]
                && row["id"] == row_id(receipt, kind, pointer)
                && ids.insert(row["id"].as_str().ok_or_else(|| anyhow::anyhow!(BINDING))?)
                && row["anchorGranularity"] == "json_pointer_into_received_digest"
                && row["provenance"] == "authenticated_sender_claim_unverified_source"
                && row["sourceAuthority"] == false
                && row["semanticIdentityEstablished"] == false,
            BINDING
        );
        closed(&row["metadata"], &["context", "data"])?;
        let context = &row["metadata"]["context"];
        ensure!(context.as_object().is_some(), BINDING);
        for key in [
            "appId",
            "senderWorkspaceId",
            "installationId",
            "repositoryId",
            "repository",
            "reportedSnapshotId",
        ] {
            ensure!(context[key] == manifest["scope"][key], BINDING);
        }
    }
    Ok(())
}
fn closed(v: &Value, fields: &[&str]) -> Result<()> {
    keys(v, fields)?;
    ensure!(
        v.as_object().is_some_and(|m| m.len() == fields.len()),
        BINDING
    );
    Ok(())
}
fn pointer_matches(kind: Kind, pointer: &str) -> bool {
    let Some(suffix) = pointer.strip_prefix("/digest/payload/") else {
        return false;
    };
    if pointer.len() > 256 {
        return false;
    }
    let p = suffix.split('/').collect::<Vec<_>>();
    let ordinal = |s: &str| {
        !s.is_empty()
            && s.len() <= 6
            && (s == "0" || !s.starts_with('0'))
            && s.bytes().all(|b| b.is_ascii_digit())
    };
    match (kind, p.as_slice()) {
        (Kind::Symbol, ["symbols", i])
        | (Kind::Commit, ["commits", i])
        | (Kind::Pull, ["threads", i]) => ordinal(i),
        (Kind::Observation, ["threads", i, "observations", j]) => ordinal(i) && ordinal(j),
        (Kind::Relation, ["commits", i, "parentIds", j]) => ordinal(i) && ordinal(j),
        (Kind::Relation, ["threads", i, "sourceAssociation"]) => ordinal(i),
        (Kind::Relation, ["threads", i, "observations", j, "inReplyToId" | "reviewId"]) => {
            ordinal(i) && ordinal(j)
        }
        _ => false,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (Request, Value) {
        let r = Request {
            schema_version: 1,
            app_id: 123,
            sender_workspace_id: "a".repeat(64),
            installation_id: 42,
            repository_id: 9,
            repository: "org/repo".into(),
            proposal_ids: vec!["b".repeat(64)],
            kinds: vec![Kind::Symbol],
            text: String::new(),
            offset: 0,
            limit: 1,
        };
        let receipt = "c".repeat(64);
        let pointer = "/digest/payload/symbols/0";
        let scope = json!({"appId":123,"senderWorkspaceId":r.sender_workspace_id,"installationId":42,"repositoryId":9,"repository":"org/repo","reportedSnapshotId":"d".repeat(64)});
        let manifest = json!({"proposalId":r.proposal_ids[0],"receiptId":receipt,"payloadHash":"e".repeat(64),"digestId":"f".repeat(64),"principalId":"sender","receivedAtUnix":1700000000u64,"scope":scope,"sourceDigestHash":"1".repeat(64),"discussionCaptureId":"2".repeat(64),"discussionCoverageComplete":false,"unacquiredPulls":[],"symbolCandidates":1,"symbolOmissions":0,"unsupportedFiles":0,"visibilityUnresolved":0,"receiverSourceVerification":"unavailable_no_sender_cas"});
        let row = json!({"id":row_id(&receipt,Kind::Symbol,pointer),"kind":"symbol","proposalId":r.proposal_ids[0],"receiptId":receipt,"payloadHash":manifest["payloadHash"],"pointer":pointer,"anchorGranularity":"json_pointer_into_received_digest","metadata":{"context":scope,"data":{"name":"unverified fixture"}},"provenance":"authenticated_sender_claim_unverified_source","sourceAuthority":false,"semanticIdentityEstablished":false});
        let v = json!({"schemaVersion":1,"kind":"native_received_digest_query","processor":"literal_metadata_projection_v1","requestHash":identity(&r),"receiverWorkspaceId":"3".repeat(64),"manifests":[manifest],"rows":[row],"visitedRows":1,"matchingRows":1,"searchBytes":0,"offset":0,"limit":1,"hasMore":false,"completeForSelectedMetadata":true,"complete":false,"sourceAuthority":false,"modelCalls":0,"networkRequests":0,"sourceWritten":false,"recordsWritten":false,"limitations":[]});
        (r, v)
    }
    #[test]
    fn untrusted_response_cannot_change_scope_binding_row_identity_or_authority() {
        let (r, v) = fixture();
        validate_response(&v, &r).unwrap();
        for pointer in [
            "/requestHash",
            "/complete",
            "/sourceAuthority",
            "/recordsWritten",
            "/rows/0/sourceAuthority",
            "/rows/0/semanticIdentityEstablished",
            "/rows/0/receiptId",
            "/rows/0/id",
            "/manifests/0/scope/appId",
            "/rows/0/metadata/context/repository",
            "/matchingRows",
            "/hasMore",
        ] {
            let mut bad = v.clone();
            *bad.pointer_mut(pointer).unwrap() = json!("forged");
            assert!(validate_response(&bad, &r).is_err(), "{pointer}");
        }
        let mut missing = v.clone();
        missing.as_object_mut().unwrap().remove("limitations");
        assert!(validate_response(&missing, &r).is_err());
        let mut unknown = v.clone();
        unknown["automaticExecution"] = json!(true);
        assert!(validate_response(&unknown, &r).is_err());
        for pointer in [
            "/digest/payload/symbols/../0",
            "/digest/payload/symbols/00",
            "/digest/payload/commits/0",
            "/digest/payload/symbols/0/body",
        ] {
            let mut bad = v.clone();
            bad["rows"][0]["pointer"] = json!(pointer);
            bad["rows"][0]["id"] = json!(row_id(
                bad["rows"][0]["receiptId"].as_str().unwrap(),
                Kind::Symbol,
                pointer
            ));
            assert!(validate_response(&bad, &r).is_err());
        }
    }
    #[test]
    fn query_request_is_closed_unique_and_unicode_bounded() {
        let (r, _) = fixture();
        Request::parse(&serde_json::to_vec(&r).unwrap()).unwrap();
        for text in ["x".repeat(257), "\0".into(), "\n".into()] {
            let mut q = r.clone();
            q.text = text;
            assert!(q.validate().is_err());
        }
        let mut q = r.clone();
        q.text = "🦀".repeat(256);
        q.validate().unwrap();
        q.kinds.push(Kind::Symbol);
        assert!(q.validate().is_err());
        for limit in [0, 1001] {
            let mut q = r.clone();
            q.limit = limit;
            assert!(q.validate().is_err());
        }
        let raw = serde_json::to_vec(&r).unwrap();
        let mut duplicate = b"{\"schemaVersion\":1,".to_vec();
        duplicate.extend_from_slice(&raw[1..]);
        assert!(Request::parse(&duplicate).is_err());
    }
}
