//! Exact excerpts from sources already admitted by the parent producer.
use super::{Budget, CheckedSource, DetailOptions, capacity};
use crate::{capture::Deadline, paths};
use anyhow::{Context, Result, ensure};
use serde_json::{Map, Value, json};
use std::collections::HashMap;

const SCAN_BYTES: usize = 16_384;

struct LineCheckpoint {
    byte: usize,
    line_breaks: usize,
}

struct SourceFacts {
    length: usize,
    safe: bool,
    checkpoints: Vec<LineCheckpoint>,
}

impl SourceFacts {
    fn inspect(text: &str, deadline: &Deadline) -> Result<Self> {
        deadline.check()?;
        let mut checkpoints = Vec::new();
        let mut cursor = 0usize;
        let mut line_breaks = 0usize;
        let mut unsafe_control = false;
        loop {
            deadline.check()?;
            checkpoints.push(LineCheckpoint {
                byte: cursor,
                line_breaks,
            });
            if cursor == text.len() {
                break;
            }
            let remaining = text.len().checked_sub(cursor).ok_or_else(capacity)?;
            let mut end = cursor
                .checked_add(remaining.min(SCAN_BYTES))
                .ok_or_else(capacity)?;
            while !text.is_char_boundary(end) {
                end = end.checked_sub(1).ok_or_else(capacity)?;
            }
            for character in text[cursor..end].chars() {
                if character == '\n' {
                    line_breaks = line_breaks.checked_add(1).ok_or_else(capacity)?;
                }
                if character == '\0'
                    || (character.is_control() && !matches!(character, '\r' | '\n' | '\t'))
                {
                    unsafe_control = true;
                }
            }
            cursor = end;
        }
        // Keep the released rule authoritative rather than maintaining a second
        // token detector. Its synchronous scan has cooperative checks around it.
        deadline.check()?;
        let obvious_secret = paths::obvious_secret(text);
        deadline.check()?;
        Ok(Self {
            length: text.len(),
            safe: !unsafe_control && !obvious_secret,
            checkpoints,
        })
    }

    fn line(&self, text: &str, byte: usize, deadline: &Deadline) -> Result<usize> {
        deadline.check()?;
        ensure!(
            text.len() == self.length && byte <= self.length,
            "Invalid checked Git quote source"
        );
        let index = self
            .checkpoints
            .partition_point(|checkpoint| checkpoint.byte <= byte)
            .checked_sub(1)
            .ok_or_else(capacity)?;
        let checkpoint = &self.checkpoints[index];
        let mut line_breaks = checkpoint.line_breaks;
        for &character in &text.as_bytes()[checkpoint.byte..byte] {
            if character == b'\n' {
                line_breaks = line_breaks.checked_add(1).ok_or_else(capacity)?;
            }
        }
        deadline.check()?;
        line_breaks.checked_add(1).ok_or_else(capacity)
    }

    fn range(&self, text: &str, start: usize, end: usize, deadline: &Deadline) -> Result<Value> {
        let start_line = self.line(text, start, deadline)?;
        let end_line = if start == end {
            start_line
        } else {
            self.line(text, end.checked_sub(1).ok_or_else(capacity)?, deadline)?
        };
        Ok(json!({
            "startByte": start,
            "endByte": end,
            "startLine": start_line,
            "endLine": end_line,
        }))
    }
}

pub(super) struct Quoter {
    max_quote_bytes: usize,
    max_total_quote_bytes: usize,
    requested_quotes: usize,
    complete_quotes: usize,
    excerpted_quotes: usize,
    omitted_quotes: usize,
    emitted_quote_bytes: usize,
    source_facts: HashMap<String, SourceFacts>,
}

impl Quoter {
    pub(super) fn new(options: &DetailOptions) -> Self {
        Self {
            max_quote_bytes: options.max_quote_bytes,
            max_total_quote_bytes: options.max_total_quote_bytes,
            requested_quotes: 0,
            complete_quotes: 0,
            excerpted_quotes: 0,
            omitted_quotes: 0,
            emitted_quote_bytes: 0,
            source_facts: HashMap::new(),
        }
    }

    pub(super) fn quote(
        &mut self,
        source: &CheckedSource,
        anchor: &Value,
        budget: &mut Budget,
        deadline: &Deadline,
    ) -> Result<Value> {
        deadline.check()?;
        ensure!(
            anchor.get("snapshotId").and_then(Value::as_str) == Some(source.snapshot_id.as_str())
                && anchor.get("path").and_then(Value::as_str) == Some(source.path.as_str())
                && anchor.get("contentHash").and_then(Value::as_str) == Some(source.hash.as_str()),
            "Invalid Git impact quote binding"
        );
        let start = usize::try_from(
            anchor
                .get("startByte")
                .and_then(Value::as_u64)
                .context("Invalid Git impact quote start")?,
        )
        .map_err(|_| capacity())?;
        let end = usize::try_from(
            anchor
                .get("endByte")
                .and_then(Value::as_u64)
                .context("Invalid Git impact quote end")?,
        )
        .map_err(|_| capacity())?;
        let text: &str = &source.text;
        ensure!(
            start <= end
                && end <= text.len()
                && text.is_char_boundary(start)
                && text.is_char_boundary(end),
            "Invalid Git impact quote UTF-8 range"
        );
        if !self.source_facts.contains_key(&source.hash) {
            let facts = SourceFacts::inspect(text, deadline)?;
            self.source_facts.insert(source.hash.clone(), facts);
        }
        let facts = self
            .source_facts
            .get(&source.hash)
            .context("Checked Git quote facts unavailable")?;
        let requested = facts.range(text, start, end, deadline)?;
        self.requested_quotes = self.requested_quotes.checked_add(1).ok_or_else(capacity)?;

        // Safety covers the complete source, including bytes outside the anchor,
        // and takes precedence even for a valid empty requested range.
        let (status, range, quote_text, reason) = if !facts.safe {
            self.omitted_quotes = self.omitted_quotes.checked_add(1).ok_or_else(capacity)?;
            (
                "omitted",
                Value::Null,
                Value::Null,
                Some("unsafe_source_excerpt"),
            )
        } else {
            let remaining = self
                .max_total_quote_bytes
                .checked_sub(self.emitted_quote_bytes)
                .ok_or_else(capacity)?;
            let requested_bytes = end.checked_sub(start).ok_or_else(capacity)?;
            let allowance = requested_bytes.min(self.max_quote_bytes).min(remaining);
            let mut actual_end = start.checked_add(allowance).ok_or_else(capacity)?;
            while !text.is_char_boundary(actual_end) {
                deadline.check()?;
                actual_end = actual_end.checked_sub(1).ok_or_else(capacity)?;
            }
            let emitted = actual_end.checked_sub(start).ok_or_else(capacity)?;
            let complete = actual_end == end;
            let reason = if complete {
                None
            } else if self.max_quote_bytes <= remaining {
                Some("per_quote_byte_limit")
            } else {
                Some("aggregate_quote_byte_limit")
            };
            if complete || emitted != 0 {
                let slice = &text[start..actual_end];
                // Reserve the exact serialized string fragment before allocating
                // its owned output. The parent counts the final wrapper once.
                budget.reserve_serialized(&slice)?;
                deadline.check()?;
                let quote_text = Value::String(slice.to_owned());
                let range = facts.range(text, start, actual_end, deadline)?;
                self.emitted_quote_bytes = self
                    .emitted_quote_bytes
                    .checked_add(emitted)
                    .ok_or_else(capacity)?;
                let status = if complete {
                    self.complete_quotes =
                        self.complete_quotes.checked_add(1).ok_or_else(capacity)?;
                    "complete"
                } else {
                    self.excerpted_quotes =
                        self.excerpted_quotes.checked_add(1).ok_or_else(capacity)?;
                    "excerpt"
                };
                (status, range, quote_text, reason)
            } else {
                self.omitted_quotes = self.omitted_quotes.checked_add(1).ok_or_else(capacity)?;
                ("omitted", Value::Null, Value::Null, reason)
            }
        };
        deadline.check()?;
        let mut output = Map::new();
        output.insert(
            "snapshotId".to_owned(),
            Value::String(source.snapshot_id.clone()),
        );
        output.insert("path".to_owned(), Value::String(source.path.clone()));
        output.insert("contentHash".to_owned(), Value::String(source.hash.clone()));
        output.insert("requested".to_owned(), requested);
        output.insert("status".to_owned(), Value::String(status.to_owned()));
        output.insert("range".to_owned(), range);
        output.insert("text".to_owned(), quote_text);
        output.insert(
            "reason".to_owned(),
            reason.map_or(Value::Null, |value| Value::String(value.to_owned())),
        );
        deadline.check()?;
        Ok(Value::Object(output))
    }

    pub(super) fn coverage(&self) -> Value {
        json!({
            "requestedQuotes": self.requested_quotes,
            "completeQuotes": self.complete_quotes,
            "excerptedQuotes": self.excerpted_quotes,
            "omittedQuotes": self.omitted_quotes,
            "emittedQuoteBytes": self.emitted_quote_bytes,
            "quotesComplete": self.complete_quotes == self.requested_quotes,
        })
    }
}
