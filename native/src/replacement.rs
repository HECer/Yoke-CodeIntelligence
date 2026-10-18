//! Bounded regex capture interpolation shared by source and note edits.
use crate::capture::Deadline;
use anyhow::{Result, ensure};
fn append(output: &mut String, text: &str, max: usize) -> Result<()> {
    ensure!(
        output
            .len()
            .checked_add(text.len())
            .is_some_and(|length| length <= max),
        "Replacement byte budget exceeded"
    );
    output.push_str(text);
    Ok(())
}
/// Implements Rust regex capture-template syntax, checking before every append.
pub fn expand(
    captures: &regex::Captures<'_>,
    template: &str,
    max: usize,
    deadline: &Deadline,
) -> Result<String> {
    deadline.check()?;
    let mut output = String::new();
    let mut cursor = 0;
    // Cache the next brace: malformed repeated `${` must remain linear work.
    let mut brace = template.find('}');
    while cursor < template.len() {
        deadline.check()?;
        let Some(relative) = template[cursor..].find('$') else {
            append(&mut output, &template[cursor..], max)?;
            break;
        };
        let dollar = cursor + relative;
        append(&mut output, &template[cursor..dollar], max)?;
        let bytes = template.as_bytes();
        if bytes.get(dollar + 1) == Some(&b'$') {
            append(&mut output, "$", max)?;
            cursor = dollar + 2;
            continue;
        }
        let (reference, end) = if bytes.get(dollar + 1) == Some(&b'{') {
            let start = dollar + 2;
            if brace.is_some_and(|position| position < start) {
                brace = template[start..].find('}').map(|position| start + position);
            }
            if let Some(end) = brace {
                (Some(&template[start..end]), end + 1)
            } else {
                (None, dollar + 1)
            }
        } else {
            let mut end = dollar + 1;
            while bytes
                .get(end)
                .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
            {
                end += 1;
            }
            (
                if end > dollar + 1 {
                    Some(&template[dollar + 1..end])
                } else {
                    None
                },
                end,
            )
        };
        if let Some(reference) = reference {
            let captured = if let Ok(index) = reference.parse::<usize>() {
                captures.get(index)
            } else {
                captures.name(reference)
            };
            if let Some(captured) = captured {
                append(&mut output, captured.as_str(), max)?;
            }
        } else {
            append(&mut output, "$", max)?;
        }
        cursor = end;
    }
    deadline.check()?;
    Ok(output)
}
