//! Strict hash-bound prompt catalog data; no filesystem or rendering authority.
use super::{MAX_FILE_BYTES, yaml};
use crate::{capture::Deadline, model};
use anyhow::{Result, ensure};
use serde_json::Value;
use std::collections::BTreeMap;
pub const MAX_CATALOG_FILES: usize = 32;
pub const MAX_EFFECTIVE_PROMPTS: usize = 256;
pub const MAX_PROMPT_DECLARATIONS: usize = 512;
pub fn prompt_name(name: &str) -> Result<()> {
    ensure!(
        !name.is_empty()
            && name != "[native prompt]"
            && name.len() <= 200
            && name
                .split('/')
                .all(|p| !p.is_empty() && p != "." && p != "..")
            && !name
                .chars()
                .any(|c| c.is_control() || matches!(c, '\\' | ':')),
        "session_config_prompt_name_invalid"
    );
    Ok(())
}
pub fn decode_prompt_catalog(
    bytes: &[u8],
    expected_sha256: &str,
    d: &Deadline,
) -> Result<BTreeMap<String, String>> {
    d.check()?;
    ensure!(
        bytes.len() <= MAX_FILE_BYTES,
        "session_config_catalog_file_capacity"
    );
    ensure!(
        model::valid_id(expected_sha256) && model::hash(bytes) == expected_sha256,
        "session_config_catalog_source_changed"
    );
    let mut root = yaml::decode(bytes, d)?;
    ensure!(
        root.len() == 1 && root.contains_key("prompts"),
        "session_config_prompt_catalog_schema_invalid"
    );
    let Value::Object(prompts) = root.remove("prompts").unwrap() else {
        anyhow::bail!("session_config_prompt_catalog_schema_invalid")
    };
    ensure!(
        prompts.len() <= MAX_PROMPT_DECLARATIONS,
        "session_config_prompt_declaration_capacity"
    );
    let mut result = BTreeMap::new();
    for (name, value) in prompts {
        d.check()?;
        prompt_name(&name)?;
        let text = value
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("session_config_prompt_text_required"))?;
        ensure!(
            text.len() <= MAX_FILE_BYTES && !text.contains('\0'),
            "session_config_prompt_text_invalid"
        );
        result.insert(name, text.into());
    }
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalog_parser_binds_raw_bom_crlf_and_decodes_only_prompt_strings() {
        let bytes="\u{feff}prompts:\r\n  system_prompt: |\r\n    First\r\n    e\u{301} \u{1f980}\r\n  base: '{{ value }}'\r\n".as_bytes();
        let parsed =
            decode_prompt_catalog(bytes, &model::hash(bytes), &Deadline::new(10000)).unwrap();
        assert_eq!(parsed["system_prompt"], "First\ne\u{301} \u{1f980}\n");
        assert_eq!(parsed["base"], "{{ value }}");
        assert!(decode_prompt_catalog(bytes, &"0".repeat(64), &Deadline::new(10000)).is_err());
    }
    #[test]
    fn catalog_parser_refuses_aliases_duplicates_authority_keys_types_and_unsafe_names() {
        for bytes in [
            "prompts: {a: first, a: second}",
            "prompts: &a {x: yes}",
            "prompts: {x: 3}",
            "prompts: {x: null}",
            "prompts: {x: [value]}",
            "prompts: {'../x': value}",
            "prompts: {'C:/x': value}",
            "prompts: {'[native prompt]': value}",
            "prompts: {x: yes}\nactivation_command: unsafe",
            "prompts: {}\n---\nprompts: {}",
        ] {
            assert!(
                decode_prompt_catalog(bytes.as_bytes(), &model::hash(bytes), &Deadline::new(10000))
                    .is_err(),
                "{bytes}"
            );
        }
        assert!(
            decode_prompt_catalog(
                b"prompts: {}",
                &model::hash(b"prompts: {}"),
                &Deadline::new(10000)
            )
            .unwrap()
            .is_empty()
        );
    }
}
