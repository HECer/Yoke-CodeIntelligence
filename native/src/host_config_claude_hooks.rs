//! Entirely owned external hook settings; foreign arrays are never patched.
use super::*;
const SELECTOR: &str = "claude-session-hooks-v1";
pub(super) fn checked(bytes: &[u8], prior: &OwnedFragment, d: &Deadline) -> Result<()> {
    d.check()?;
    ensure!(
        prior.format == Format::ClaudeHookSettingsV1
            && prior.selector == SELECTOR
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
    text(input, MAX_BODY, d)?;
    if let Some(owned) = prior {
        checked(input, owned, d)?;
    } else {
        ensure!(
            input.is_empty(),
            "Existing Claude hook settings cannot be adopted"
        );
    }
    let output = body.unwrap_or_default();
    let owned_fragment = body.map(|bytes| OwnedFragment {
        format: Format::ClaudeHookSettingsV1,
        selector: SELECTOR.into(),
        bytes: bytes.into(),
        hash: hash(bytes),
    });
    if let Some(owned) = &owned_fragment {
        checked(output, owned, d)?;
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
