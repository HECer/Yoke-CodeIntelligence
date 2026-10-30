//! Trusted local template with escaped JSON data and a matching strict policy.
use super::*;
use base64::{Engine as _, prelude::BASE64_STANDARD};
use sha2::{Digest, Sha256};

const TEMPLATE: &str = include_str!("github_app_page.html");
fn policy(template: &str) -> Result<String> {
    let contents = |open, close| {
        template
            .split_once(open)
            .and_then(|(_, tail)| tail.split_once(close))
            .map(|(body, _)| body)
            .ok_or_else(|| anyhow::anyhow!(INVALID))
    };
    let style = BASE64_STANDARD.encode(Sha256::digest(contents("<style>", "</style>")?.as_bytes()));
    let script = BASE64_STANDARD.encode(Sha256::digest(
        contents("<script>", "</script>")?.as_bytes(),
    ));
    Ok(format!(
        "default-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'; object-src 'none'; script-src 'sha256-{script}'; style-src 'sha256-{style}'"
    ))
}
pub(super) fn html(display: &Value, limit: usize, d: &Deadline) -> Result<String> {
    template_html(TEMPLATE, display, limit, d)
}
fn template_html(template: &str, display: &Value, limit: usize, d: &Deadline) -> Result<String> {
    d.check()?;
    // HTML tokenization normalizes line endings. Use the same bytes for both
    // the rendered template and its CSP hashes, including Windows checkouts.
    let normalized = template
        .contains('\r')
        .then(|| template.replace("\r\n", "\n").replace('\r', "\n"));
    let template = normalized.as_deref().unwrap_or(template);
    let packet = bounded(display, limit, d)?;
    let packet = std::str::from_utf8(&packet).map_err(|_| anyhow::anyhow!(INVALID))?;
    let mut escaped = String::new();
    for (index, ch) in packet.chars().enumerate() {
        if index % 1024 == 0 {
            d.check()?;
        }
        let replacement = match ch {
            '&' => Some("\\u0026"),
            '<' => Some("\\u003c"),
            '>' => Some("\\u003e"),
            '\u{2028}' => Some("\\u2028"),
            '\u{2029}' => Some("\\u2029"),
            _ => None,
        };
        let size = replacement.map_or(ch.len_utf8(), str::len);
        ensure!(size <= limit.saturating_sub(escaped.len()), CAPACITY);
        if let Some(text) = replacement {
            escaped.push_str(text);
        } else {
            escaped.push(ch);
        }
    }
    let (prefix, tail) = template
        .split_once("__YOKE_PAGE_POLICY__")
        .ok_or_else(|| anyhow::anyhow!(INVALID))?;
    let (middle, suffix) = tail
        .split_once("__YOKE_PAGE_DATA__")
        .ok_or_else(|| anyhow::anyhow!(INVALID))?;
    let policy = policy(template)?;
    let size = [
        prefix.len(),
        policy.len(),
        middle.len(),
        escaped.len(),
        suffix.len(),
    ]
    .into_iter()
    .try_fold(0usize, |sum, n| sum.checked_add(n))
    .ok_or_else(|| anyhow::anyhow!(CAPACITY))?;
    ensure!(size <= limit, CAPACITY);
    d.check()?;
    let mut out = String::with_capacity(size);
    for part in [prefix, &policy, middle, &escaped, suffix] {
        out.push_str(part);
    }
    d.check()?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hostile_data_round_trips_without_becoming_markup_or_script_syntax() {
        let d = Deadline::new(1000);
        let hostile = "</script><img src=x onerror=alert(1)>&\u{2028}\u{2029}";
        let packet = json!({"name":hostile,"other":"__YOKE_PAGE_DATA__"});
        let output = html(&packet, 64_000, &d).unwrap();
        assert!(!output.contains(hostile));
        assert!(!output.contains("<img"));
        assert!(!output.contains('\u{2028}'));
        assert!(!output.contains('\u{2029}'));
        let data = output
            .split_once("<script type=\"application/json\" id=\"data\">")
            .unwrap()
            .1
            .split_once("</script>")
            .unwrap()
            .0;
        assert_eq!(serde_json::from_str::<Value>(data).unwrap(), packet);
        assert_eq!(output.matches("<script>").count(), 1);
    }

    #[test]
    fn escaped_expansion_and_complete_template_obey_the_same_exact_html_budget() {
        let d = Deadline::new(1000);
        let packet = json!({"label":"&".repeat(1000)});
        let html = html(&packet, 64_000, &d).unwrap();
        assert_eq!(super::html(&packet, html.len(), &d).unwrap(), html);
        assert_eq!(
            super::html(&packet, html.len() - 1, &d)
                .unwrap_err()
                .to_string(),
            CAPACITY
        );
        assert_eq!(
            super::html(&packet, 1100, &d).unwrap_err().to_string(),
            CAPACITY
        );
    }

    #[test]
    fn windows_checkout_line_endings_match_the_browser_inline_csp_hash() {
        let d = Deadline::new(1000);
        let windows_template = TEMPLATE.replace("\r\n", "\n").replace('\n', "\r\n");
        let packet = json!({"label":"owned fixture"});
        let output = template_html(&windows_template, &packet, 64_000, &d).unwrap();
        let csp = output
            .split_once("http-equiv=\"Content-Security-Policy\" content=\"")
            .unwrap()
            .1
            .split_once('"')
            .unwrap()
            .0;
        // HTML normalizes CRLF before exposing inline block text to CSP.
        for (open, close) in [("<script>", "</script>"), ("<style>", "</style>")] {
            let text = output
                .split_once(open)
                .unwrap()
                .1
                .split_once(close)
                .unwrap()
                .0
                .replace("\r\n", "\n")
                .replace('\r', "\n");
            let expected = BASE64_STANDARD.encode(Sha256::digest(text.as_bytes()));
            assert!(
                csp.contains(&format!("'sha256-{expected}'")),
                "trusted inline block would be blocked after a CRLF checkout"
            );
        }
        let unix_template = TEMPLATE.replace("\r\n", "\n").replace('\r', "\n");
        assert_eq!(
            output,
            template_html(&unix_template, &packet, 64_000, &d).unwrap()
        );
    }
}
