//! Entirely owned external hook settings; foreign arrays are never patched.
use super::*;
const SELECTOR: &str = "claude-session-hooks-v1";
pub(super) fn checked(bytes: &[u8], prior: &OwnedFragment, d: &Deadline) -> Result<()> {
    checked_for(bytes, prior, Format::ClaudeHookSettingsV1, SELECTOR, d)
}
pub(super) fn checked_source(bytes: &[u8], prior: &OwnedFragment, d: &Deadline) -> Result<()> {
    checked_for(
        bytes,
        prior,
        Format::ClaudeSourceHookSettingsV1,
        "claude-source-hooks-v1",
        d,
    )
}
fn checked_for(
    bytes: &[u8],
    prior: &OwnedFragment,
    format: Format,
    selector: &str,
    d: &Deadline,
) -> Result<()> {
    d.check()?;
    ensure!(
        prior.format == format
            && prior.selector == selector
            && prior.bytes == bytes
            && hash(bytes) == prior.hash,
        "Claude hook settings ownership differs"
    );
    ensure!(
        crate::json::parse(bytes, MAX_BODY)?.is_object(),
        "Claude hook settings must be an object"
    );
    Ok(())
}
pub fn patch(
    input: &[u8],
    body: Option<&[u8]>,
    prior: Option<&OwnedFragment>,
    d: &Deadline,
) -> Result<Patch> {
    patch_for(
        input,
        body,
        prior,
        Format::ClaudeHookSettingsV1,
        SELECTOR,
        d,
    )
}
pub fn patch_source(
    input: &[u8],
    body: Option<&[u8]>,
    prior: Option<&OwnedFragment>,
    d: &Deadline,
) -> Result<Patch> {
    patch_for(
        input,
        body,
        prior,
        Format::ClaudeSourceHookSettingsV1,
        "claude-source-hooks-v1",
        d,
    )
}
fn patch_for(
    input: &[u8],
    body: Option<&[u8]>,
    prior: Option<&OwnedFragment>,
    format: Format,
    selector: &str,
    d: &Deadline,
) -> Result<Patch> {
    text(input, MAX_BODY, d)?;
    if let Some(owned) = prior {
        checked_for(input, owned, format.clone(), selector, d)?;
    } else {
        ensure!(
            input.is_empty(),
            "Existing Claude hook settings cannot be adopted"
        );
    }
    let output = body.unwrap_or_default();
    let owned_fragment = body.map(|bytes| OwnedFragment {
        format: format.clone(),
        selector: selector.into(),
        bytes: bytes.into(),
        hash: hash(bytes),
    });
    if let Some(owned) = &owned_fragment {
        checked_for(output, owned, format, selector, d)?;
    }
    Ok(Patch {
        before_hash: hash(input),
        after_hash: hash(output),
        owned_fragment,
        edits: if input == output {
            vec![]
        } else {
            vec![ByteEdit {
                start_byte: 0,
                end_byte: input.len(),
                expected: input.into(),
                replacement: output.into(),
            }]
        },
    })
}
