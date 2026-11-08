//! Closed internal error-chain labels; no source, path, protocol or caller data.
use anyhow::Error;

#[derive(Clone, Copy)]
pub(super) enum ProviderStage {
    SourceManifests,
    ProviderSelection,
    HostConfiguration,
    PassiveAdmission,
    ProfileBinding,
    RequestDispatch,
    SourceView,
    ProviderSetup,
    Materialize,
    TransportConnect,
    TransportSpawn,
    Initialize,
    TargetOpen,
    RequestReply,
    PublishedDiagnostics,
}

// Keep Display and typed causes exact. Only callers explicitly rendering the
// existing full chain see the added constant label; successful values are untouched.
pub(super) fn annotate(error: Error, stage: ProviderStage) -> Error {
    let original = error.to_string();
    let label = match stage {
        ProviderStage::SourceManifests => "provider_stage=source_manifests",
        ProviderStage::ProviderSelection => "provider_stage=provider_selection",
        ProviderStage::HostConfiguration => "provider_stage=host_configuration",
        ProviderStage::PassiveAdmission => "provider_stage=passive_admission",
        ProviderStage::ProfileBinding => "provider_stage=profile_binding",
        ProviderStage::RequestDispatch => "provider_stage=request_dispatch",
        ProviderStage::SourceView => "provider_stage=source_view",
        ProviderStage::ProviderSetup => "provider_stage=provider_setup",
        ProviderStage::Materialize => "provider_stage=materialize",
        ProviderStage::TransportConnect => "provider_stage=transport_connect",
        ProviderStage::TransportSpawn => "provider_stage=transport_spawn",
        ProviderStage::Initialize => "provider_stage=initialize",
        ProviderStage::TargetOpen => "provider_stage=target_open",
        ProviderStage::RequestReply => "provider_stage=request_reply",
        ProviderStage::PublishedDiagnostics => "provider_stage=published_diagnostics",
    };
    error.context(label).context(original)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capture::Deadline;
    use std::io::{Error as IoError, ErrorKind};

    #[test]
    fn phase_labels_preserve_display_typed_causes_deadlines_and_success() {
        for (stage, label) in [
            (
                ProviderStage::SourceManifests,
                "provider_stage=source_manifests",
            ),
            (
                ProviderStage::ProviderSelection,
                "provider_stage=provider_selection",
            ),
            (
                ProviderStage::HostConfiguration,
                "provider_stage=host_configuration",
            ),
            (
                ProviderStage::PassiveAdmission,
                "provider_stage=passive_admission",
            ),
            (
                ProviderStage::ProfileBinding,
                "provider_stage=profile_binding",
            ),
            (
                ProviderStage::RequestDispatch,
                "provider_stage=request_dispatch",
            ),
            (ProviderStage::SourceView, "provider_stage=source_view"),
            (
                ProviderStage::ProviderSetup,
                "provider_stage=provider_setup",
            ),
            (ProviderStage::Materialize, "provider_stage=materialize"),
            (
                ProviderStage::TransportConnect,
                "provider_stage=transport_connect",
            ),
            (
                ProviderStage::TransportSpawn,
                "provider_stage=transport_spawn",
            ),
            (ProviderStage::Initialize, "provider_stage=initialize"),
            (ProviderStage::TargetOpen, "provider_stage=target_open"),
            (ProviderStage::RequestReply, "provider_stage=request_reply"),
            (
                ProviderStage::PublishedDiagnostics,
                "provider_stage=published_diagnostics",
            ),
        ] {
            for kind in [
                ErrorKind::PermissionDenied,
                ErrorKind::Interrupted,
                ErrorKind::TimedOut,
                ErrorKind::WouldBlock,
            ] {
                let error = Error::new(IoError::new(kind, "owned original refusal"))
                    .context("existing outer refusal");
                let before = error.to_string();
                let nested = annotate(annotate(error, stage), ProviderStage::RequestDispatch);
                assert_eq!(nested.to_string(), before);
                let cause = nested.downcast_ref::<IoError>().unwrap();
                assert_eq!(cause.kind(), kind);
                assert_eq!(cause.to_string(), "owned original refusal");
                assert!(nested.chain().any(|cause| cause.to_string() == label));
                assert!(
                    nested
                        .chain()
                        .any(|cause| cause.to_string() == "provider_stage=request_dispatch")
                );
            }
            let expired = Deadline::new(0);
            let original = expired.check().unwrap_err();
            let before = original.to_string();
            assert_eq!(annotate(original, stage).to_string(), before);
            assert!(expired.check().is_err());
            let original = serde_json::json!({"status":"ok","source":"unchanged"});
            let result: anyhow::Result<_> = Ok(original.clone());
            assert_eq!(
                result.map_err(|error| annotate(error, stage)).unwrap(),
                original
            );
        }
    }
}
