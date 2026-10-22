//! Strict bounded YAML values; no aliases, tags, merge keys or implicit I/O.
use crate::capture::Deadline;
use anyhow::{Result, ensure};
use serde_json::{Map, Value, json};
use yaml_rust2::{
    Yaml,
    parser::{Event, Parser},
    scanner::TScalarStyle,
};

const MAX_EVENTS: usize = 8192;
struct Reader<'a> {
    parser: Parser<std::str::Chars<'a>>,
    deadline: &'a Deadline,
    events: usize,
}
impl Reader<'_> {
    fn next(&mut self) -> Result<Event> {
        self.deadline.check()?;
        self.events += 1;
        ensure!(
            self.events <= MAX_EVENTS,
            "session_config_yaml_capacity_exceeded"
        );
        self.parser
            .next_token()
            .map(|v| v.0)
            .map_err(|_| anyhow::anyhow!("session_config_yaml_invalid"))
    }
    fn value(&mut self, event: Event, depth: usize) -> Result<Value> {
        ensure!(depth <= 16, "session_config_yaml_depth_exceeded");
        self.deadline.check()?;
        match event {
            Event::Scalar(text, style, anchor, tag) => {
                ensure!(
                    anchor == 0 && tag.is_none(),
                    "session_config_yaml_feature_unsupported"
                );
                ensure!(!text.contains('\0'), "session_config_yaml_invalid");
                if style != TScalarStyle::Plain {
                    return Ok(Value::String(text));
                }
                Ok(match Yaml::from_str(&text) {
                    Yaml::String(s) => Value::String(s),
                    Yaml::Integer(n) => json!(n),
                    Yaml::Boolean(b) => json!(b),
                    Yaml::Null => Value::Null,
                    _ => anyhow::bail!("session_config_yaml_scalar_unsupported"),
                })
            }
            Event::SequenceStart(anchor, tag) => {
                ensure!(
                    anchor == 0 && tag.is_none(),
                    "session_config_yaml_feature_unsupported"
                );
                let mut values = Vec::new();
                loop {
                    let event = self.next()?;
                    if event == Event::SequenceEnd {
                        break;
                    }
                    values.push(self.value(event, depth + 1)?);
                }
                Ok(Value::Array(values))
            }
            Event::MappingStart(anchor, tag) => {
                ensure!(
                    anchor == 0 && tag.is_none(),
                    "session_config_yaml_feature_unsupported"
                );
                let mut values = Map::new();
                loop {
                    let event = self.next()?;
                    if event == Event::MappingEnd {
                        break;
                    }
                    let Value::String(key) = self.value(event, depth + 1)? else {
                        anyhow::bail!("session_config_yaml_string_key_required")
                    };
                    ensure!(
                        key != "<<" && !values.contains_key(&key),
                        "session_config_yaml_duplicate_or_merge_key"
                    );
                    let event = self.next()?;
                    let value = self.value(event, depth + 1)?;
                    values.insert(key, value);
                }
                Ok(Value::Object(values))
            }
            _ => anyhow::bail!("session_config_yaml_feature_unsupported"),
        }
    }
}
pub(super) fn decode(bytes: &[u8], deadline: &Deadline) -> Result<Map<String, Value>> {
    deadline.check()?;
    ensure!(
        bytes.len() <= 65536 && !bytes.contains(&0),
        "session_config_yaml_capacity_or_encoding_invalid"
    );
    let text = std::str::from_utf8(bytes)
        .map_err(|_| anyhow::anyhow!("session_config_yaml_encoding_invalid"))?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut reader = Reader {
        parser: Parser::new_from_str(text),
        deadline,
        events: 0,
    };
    ensure!(
        reader.next()? == Event::StreamStart,
        "session_config_yaml_invalid"
    );
    let event = reader.next()?;
    if event == Event::StreamEnd {
        return Ok(Map::new());
    }
    ensure!(event == Event::DocumentStart, "session_config_yaml_invalid");
    let event = reader.next()?;
    let value = reader.value(event, 0)?;
    ensure!(
        reader.next()? == Event::DocumentEnd && reader.next()? == Event::StreamEnd,
        "session_config_yaml_single_document_required"
    );
    match value {
        Value::Object(v) => Ok(v),
        Value::Null => Ok(Map::new()),
        _ => anyhow::bail!("session_config_yaml_mapping_required"),
    }
}
