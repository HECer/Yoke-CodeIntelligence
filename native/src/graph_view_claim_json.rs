//! Local bounded JSON token retention. No global serde feature or Value reinterpretation.
use crate::capture::Deadline;
use anyhow::{Context, Result, bail, ensure};
use std::collections::BTreeMap;

pub(crate) struct RawValue(String);
impl RawValue {
    pub(crate) fn get(&self) -> &str {
        &self.0
    }
}
pub(crate) struct Object(pub BTreeMap<String, RawValue>);
struct Parser<'a> {
    text: &'a str,
    at: usize,
    nodes: usize,
    deadline: &'a Deadline,
}
impl<'a> Parser<'a> {
    fn new(text: &'a str, deadline: &'a Deadline) -> Result<Self> {
        deadline.check()?;
        ensure!(
            text.len() <= 1_000_000,
            "Declared JSON byte capacity exceeded"
        );
        Ok(Self {
            text,
            at: 0,
            nodes: 0,
            deadline,
        })
    }
    fn peek(&self) -> Option<u8> {
        self.text.as_bytes().get(self.at).copied()
    }
    fn step(&mut self) -> Result<()> {
        self.at = self
            .at
            .checked_add(1)
            .context("Declared JSON cursor overflow")?;
        if self.at.is_multiple_of(4096) {
            self.deadline.check()?;
        }
        Ok(())
    }
    fn whitespace(&mut self) -> Result<()> {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\r' | b'\n')) {
            self.step()?;
        }
        Ok(())
    }
    fn take(&mut self, byte: u8) -> Result<()> {
        self.whitespace()?;
        ensure!(self.peek() == Some(byte), "Invalid declared JSON grammar");
        self.step()
    }
    fn end(&mut self) -> Result<()> {
        self.whitespace()?;
        ensure!(self.at == self.text.len(), "Trailing declared JSON input");
        self.deadline.check()
    }
    fn charge(&mut self, depth: usize) -> Result<()> {
        self.deadline.check()?;
        self.nodes = self
            .nodes
            .checked_add(1)
            .context("Declared JSON node overflow")?;
        ensure!(
            depth <= 64 && self.nodes <= 2_100_000,
            "Declared JSON depth/node capacity exceeded"
        );
        Ok(())
    }
    fn string(&mut self) -> Result<String> {
        self.whitespace()?;
        let start = self.at;
        self.take(b'"')?;
        loop {
            match self.peek() {
                Some(b'"') => {
                    self.step()?;
                    break;
                }
                Some(b'\\') => {
                    self.step()?;
                    ensure!(self.peek().is_some(), "Truncated declared JSON escape");
                    self.step()?;
                }
                Some(_) => self.step()?,
                None => bail!("Unterminated declared JSON string"),
            }
        }
        // Only a string is deserialized here. Reserved object keys cannot reinterpret values.
        serde_json::from_str(&self.text[start..self.at])
            .map_err(|_| anyhow::anyhow!("Invalid declared JSON string"))
    }
    fn digits(&mut self) -> Result<()> {
        let start = self.at;
        while self.peek().is_some_and(|b| b.is_ascii_digit()) {
            self.step()?;
        }
        ensure!(self.at > start, "Declared JSON number requires digits");
        Ok(())
    }
    fn number(&mut self) -> Result<()> {
        if self.peek() == Some(b'-') {
            self.step()?;
        }
        match self.peek() {
            Some(b'0') => self.step()?,
            Some(b'1'..=b'9') => self.digits()?,
            _ => bail!("Invalid declared JSON integer"),
        }
        if self.peek() == Some(b'.') {
            self.step()?;
            self.digits()?;
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.step()?;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.step()?;
            }
            self.digits()?;
        }
        Ok(())
    }
    fn object(&mut self, depth: usize) -> Result<Object> {
        self.take(b'{')?;
        self.whitespace()?;
        let mut out = BTreeMap::new();
        if self.peek() == Some(b'}') {
            self.step()?;
            return Ok(Object(out));
        }
        loop {
            let key = self.string()?;
            ensure!(!out.contains_key(&key), "Duplicate declared JSON field");
            self.take(b':')?;
            let value = self.value(depth + 1)?;
            out.insert(key, value);
            self.whitespace()?;
            if self.peek() == Some(b'}') {
                self.step()?;
                break;
            }
            self.take(b',')?;
        }
        Ok(Object(out))
    }
    fn array(&mut self, depth: usize) -> Result<Vec<RawValue>> {
        self.take(b'[')?;
        self.whitespace()?;
        let mut out = Vec::new();
        if self.peek() == Some(b']') {
            self.step()?;
            return Ok(out);
        }
        loop {
            out.push(self.value(depth + 1)?);
            self.whitespace()?;
            if self.peek() == Some(b']') {
                self.step()?;
                break;
            }
            self.take(b',')?;
        }
        Ok(out)
    }
    fn value(&mut self, depth: usize) -> Result<RawValue> {
        self.charge(depth)?;
        self.whitespace()?;
        let start = self.at;
        match self.peek() {
            Some(b'{') => {
                self.object(depth)?;
            }
            Some(b'[') => {
                self.array(depth)?;
            }
            Some(b'"') => {
                self.string()?;
            }
            Some(b'-' | b'0'..=b'9') => self.number()?,
            Some(b't' | b'f' | b'n') => {
                let token = match self.peek() {
                    Some(b't') => "true",
                    Some(b'f') => "false",
                    _ => "null",
                };
                ensure!(
                    self.text[self.at..].starts_with(token),
                    "Invalid declared JSON literal"
                );
                self.at += token.len();
            }
            _ => bail!("Invalid declared JSON value"),
        }
        Ok(RawValue(self.text[start..self.at].to_owned()))
    }
}
#[cfg(test)]
pub(crate) fn raw_value(text: &str, deadline: &Deadline) -> Result<RawValue> {
    let mut parser = Parser::new(text, deadline)?;
    let value = parser.value(0)?;
    parser.end()?;
    Ok(value)
}
pub(crate) fn object(text: &str, deadline: &Deadline) -> Result<Object> {
    let mut parser = Parser::new(text, deadline)?;
    parser.charge(0)?;
    let value = parser.object(0)?;
    parser.end()?;
    Ok(value)
}
pub(crate) fn array(text: &str, deadline: &Deadline) -> Result<Vec<RawValue>> {
    let mut parser = Parser::new(text, deadline)?;
    parser.charge(0)?;
    let value = parser.array(0)?;
    parser.end()?;
    Ok(value)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn raw_tokens_do_not_reinterpret_reserved_keys_or_invalid_numeric_grammar() {
        let d = Deadline::new(30000);
        let source = r#"{"n":18446744073709551617,"overflow":1e999,"$serde_json::private::RawValue":"null","unicode":"π\u0301"}"#;
        let root = raw_value(source, &d).unwrap();
        assert_eq!(root.get(), source);
        let object = object(source, &d).unwrap();
        assert_eq!(object.0["n"].get(), "18446744073709551617");
        assert_eq!(object.0["$serde_json::private::RawValue"].get(), "\"null\"");
        for bad in [
            "01",
            "-01",
            "1.",
            "1e",
            "1e+",
            "+1",
            "[1,]",
            "{\"x\":1,}",
            "true false",
            "NaN",
            r#"{"a":1,"\u0061":2}"#,
            r#"{"copy":{"x":1,"x":2}}"#,
        ] {
            assert!(raw_value(bad, &d).is_err(), "{bad}");
        }
        assert!(raw_value(&format!("{}0{}", "[".repeat(65), "]".repeat(65)), &d).is_err());
        let mut parser = Parser::new("0", &d).unwrap();
        parser.nodes = 2_100_000;
        assert!(parser.value(0).is_err());
    }
}
