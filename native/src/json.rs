//! Bounded JSON host input and output. Errors never echo field values.
use anyhow::{Result, ensure};
use serde::de::{DeserializeSeed, Error as _, MapAccess, SeqAccess, Visitor};
use serde_json::Value;
use std::fmt;

struct Seed<'a> {
    depth: usize,
    nodes: &'a mut usize,
}
impl<'de> DeserializeSeed<'de> for Seed<'_> {
    type Value = Value;
    fn deserialize<D: serde::Deserializer<'de>>(
        self,
        d: D,
    ) -> std::result::Result<Value, D::Error> {
        *self.nodes += 1;
        if self.depth > 64 || *self.nodes > 2_100_000 {
            return Err(D::Error::custom("JSON capacity exceeded"));
        }
        d.deserialize_any(self)
    }
}
impl<'de> Visitor<'de> for Seed<'_> {
    type Value = Value;
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("bounded JSON with unique object fields")
    }
    fn visit_bool<E: serde::de::Error>(self, v: bool) -> std::result::Result<Value, E> {
        Ok(Value::Bool(v))
    }
    fn visit_unit<E: serde::de::Error>(self) -> std::result::Result<Value, E> {
        Ok(Value::Null)
    }
    fn visit_none<E: serde::de::Error>(self) -> std::result::Result<Value, E> {
        Ok(Value::Null)
    }
    fn visit_i64<E: serde::de::Error>(self, v: i64) -> std::result::Result<Value, E> {
        Ok(Value::Number(v.into()))
    }
    fn visit_u64<E: serde::de::Error>(self, v: u64) -> std::result::Result<Value, E> {
        Ok(Value::Number(v.into()))
    }
    fn visit_f64<E: serde::de::Error>(self, v: f64) -> std::result::Result<Value, E> {
        serde_json::Number::from_f64(v)
            .map(Value::Number)
            .ok_or_else(|| E::custom("Nonfinite JSON number"))
    }
    fn visit_str<E: serde::de::Error>(self, v: &str) -> std::result::Result<Value, E> {
        Ok(Value::String(v.into()))
    }
    fn visit_string<E: serde::de::Error>(self, v: String) -> std::result::Result<Value, E> {
        Ok(Value::String(v))
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut values: A) -> std::result::Result<Value, A::Error> {
        let mut out = Vec::new();
        while let Some(v) = values.next_element_seed(Seed {
            depth: self.depth + 1,
            nodes: &mut *self.nodes,
        })? {
            out.push(v);
        }
        Ok(Value::Array(out))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut values: A) -> std::result::Result<Value, A::Error> {
        let mut out = serde_json::Map::new();
        while let Some(key) = values.next_key::<String>()? {
            if out.contains_key(&key) {
                return Err(A::Error::custom("Duplicate JSON key"));
            }
            let value = values.next_value_seed(Seed {
                depth: self.depth + 1,
                nodes: &mut *self.nodes,
            })?;
            out.insert(key, value);
        }
        Ok(Value::Object(out))
    }
}
pub fn parse(bytes: &[u8], max_bytes: usize) -> Result<Value> {
    ensure!(bytes.len() <= max_bytes, "JSON input exceeds byte capacity");
    let mut parser = serde_json::Deserializer::from_slice(bytes);
    let mut nodes = 0;
    let value = Seed {
        depth: 0,
        nodes: &mut nodes,
    }
    .deserialize(&mut parser)
    .map_err(|error| {
        if error.to_string().starts_with("Duplicate JSON key") {
            anyhow::anyhow!("Duplicate JSON key in host input")
        } else {
            anyhow::anyhow!("Invalid JSON or exceeded depth/node capacity")
        }
    })?;
    parser
        .end()
        .map_err(|_| anyhow::anyhow!("Trailing or malformed JSON input"))?;
    Ok(value)
}

/// Decode public host input without returning serde errors that quote its values.
pub fn decode<T: serde::de::DeserializeOwned>(value: Value) -> Result<T> {
    serde_json::from_value(value).map_err(|_| anyhow::anyhow!("Invalid host input schema"))
}

/// Serialize an exact bounded JSON result under the caller's absolute deadline.
pub fn encode_bounded<T: serde::Serialize + ?Sized>(
    value: &T,
    max_bytes: usize,
    deadline: &crate::capture::Deadline,
) -> Result<Vec<u8>> {
    use std::io::Write;
    struct BoundedWriter<'a> {
        deadline: &'a crate::capture::Deadline,
        limit: usize,
        size: usize,
        bytes: Option<Vec<u8>>,
    }
    impl Write for BoundedWriter<'_> {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.deadline.check().map_err(std::io::Error::other)?;
            if bytes.len() > self.limit.saturating_sub(self.size) {
                return Err(std::io::Error::other("JSON output exceeds byte capacity"));
            }
            if let Some(output) = &mut self.bytes {
                output.extend_from_slice(bytes);
            }
            self.size += bytes.len();
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            self.deadline.check().map_err(std::io::Error::other)
        }
    }
    deadline.check()?;
    let mut counter = BoundedWriter {
        deadline,
        limit: max_bytes,
        size: 0,
        bytes: None,
    };
    serde_json::to_writer(&mut counter, value)?;
    deadline.check()?;
    let expected = counter.size;
    let mut output = BoundedWriter {
        deadline,
        limit: expected,
        size: 0,
        bytes: Some(Vec::with_capacity(expected)),
    };
    serde_json::to_writer(&mut output, value)?;
    deadline.check()?;
    ensure!(output.size == expected, "JSON output size changed");
    Ok(output.bytes.expect("output writer always owns its buffer"))
}
