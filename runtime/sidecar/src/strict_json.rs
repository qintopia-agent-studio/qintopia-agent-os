use std::{collections::HashSet, fmt};

use anyhow::{bail, Context, Result};
use serde::{
    de::{self, MapAccess, SeqAccess, Visitor},
    Deserialize, Deserializer,
};
use serde_json::Value;

#[derive(Debug, Clone, Copy)]
pub(crate) struct JsonLimits {
    pub max_bytes: usize,
    pub max_depth: usize,
    pub max_nodes: usize,
    pub max_string_bytes: usize,
    pub max_key_bytes: usize,
}

pub(crate) const RAW_EVENT_ENVELOPE_LIMITS: JsonLimits = JsonLimits {
    max_bytes: 4 * 1024 * 1024,
    max_depth: 64,
    max_nodes: 65_536,
    max_string_bytes: 1024 * 1024,
    max_key_bytes: 1024,
};

pub(crate) const QIWE_STRING_DATA_LIMITS: JsonLimits = JsonLimits {
    max_bytes: 1024 * 1024,
    max_depth: 64,
    max_nodes: 65_536,
    max_string_bytes: 512 * 1024,
    max_key_bytes: 1024,
};

pub(crate) const fn registry_json_limits(max_bytes: usize) -> JsonLimits {
    JsonLimits {
        max_bytes,
        max_depth: 32,
        max_nodes: 5_000,
        max_string_bytes: 16 * 1024,
        max_key_bytes: 256,
    }
}

pub(crate) fn parse_strict_bounded_slice(bytes: &[u8], limits: JsonLimits) -> Result<Value> {
    if bytes.len() > limits.max_bytes {
        bail!("JSON byte limit exceeded");
    }
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let StrictJsonValue(value) =
        StrictJsonValue::deserialize(&mut deserializer).context("strict JSON parse failed")?;
    deserializer
        .end()
        .context("strict JSON parse contained trailing data")?;
    validate_bounded_value(&value, limits)?;
    Ok(value)
}

pub(crate) fn parse_strict_bounded_str(text: &str, limits: JsonLimits) -> Result<Value> {
    parse_strict_bounded_slice(text.as_bytes(), limits)
}

pub(crate) fn validate_bounded_value(value: &Value, limits: JsonLimits) -> Result<()> {
    let mut stack = vec![(value, 0usize)];
    let mut nodes = 0usize;
    while let Some((current, depth)) = stack.pop() {
        nodes = nodes.saturating_add(1);
        if nodes > limits.max_nodes {
            bail!("JSON node limit exceeded");
        }
        if depth > limits.max_depth {
            bail!("JSON depth limit exceeded");
        }
        match current {
            Value::String(text) => {
                if text.len() > limits.max_string_bytes {
                    bail!("JSON string limit exceeded");
                }
            }
            Value::Array(values) => {
                stack.extend(values.iter().map(|child| (child, depth + 1)));
            }
            Value::Object(values) => {
                for (key, child) in values {
                    if key.len() > limits.max_key_bytes {
                        bail!("JSON key limit exceeded");
                    }
                    stack.push((child, depth + 1));
                }
            }
            Value::Null | Value::Bool(_) | Value::Number(_) => {}
        }
    }
    Ok(())
}

struct StrictJsonValue(Value);

impl<'de> Deserialize<'de> for StrictJsonValue {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(StrictJsonVisitor)
    }
}

struct StrictJsonVisitor;

impl<'de> Visitor<'de> for StrictJsonVisitor {
    type Value = StrictJsonValue;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a bounded JSON value without duplicate object keys")
    }

    fn visit_unit<E>(self) -> std::result::Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(StrictJsonValue(Value::Null))
    }

    fn visit_bool<E>(self, value: bool) -> std::result::Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(StrictJsonValue(Value::Bool(value)))
    }

    fn visit_i64<E>(self, value: i64) -> std::result::Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(StrictJsonValue(Value::Number(value.into())))
    }

    fn visit_u64<E>(self, value: u64) -> std::result::Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(StrictJsonValue(Value::Number(value.into())))
    }

    fn visit_f64<E>(self, value: f64) -> std::result::Result<Self::Value, E>
    where
        E: de::Error,
    {
        serde_json::Number::from_f64(value)
            .map(Value::Number)
            .map(StrictJsonValue)
            .ok_or_else(|| E::custom("non-finite JSON number"))
    }

    fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(StrictJsonValue(Value::String(value.to_string())))
    }

    fn visit_string<E>(self, value: String) -> std::result::Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(StrictJsonValue(Value::String(value)))
    }

    fn visit_seq<A>(self, mut sequence: A) -> std::result::Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut values = Vec::new();
        while let Some(value) = sequence.next_element::<StrictJsonValue>()? {
            values.push(value.0);
        }
        Ok(StrictJsonValue(Value::Array(values)))
    }

    fn visit_map<A>(self, mut map: A) -> std::result::Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut values = serde_json::Map::new();
        let mut keys = HashSet::new();
        while let Some(key) = map.next_key::<String>()? {
            if !keys.insert(key.clone()) {
                return Err(de::Error::custom("duplicate JSON object key"));
            }
            let value = map.next_value::<StrictJsonValue>()?;
            values.insert(key, value.0);
        }
        Ok(StrictJsonValue(Value::Object(values)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_LIMITS: JsonLimits = JsonLimits {
        max_bytes: 1024,
        max_depth: 3,
        max_nodes: 8,
        max_string_bytes: 8,
        max_key_bytes: 8,
    };

    #[test]
    fn rejects_duplicate_keys_at_every_depth() {
        let error =
            parse_strict_bounded_str(r#"{"outer":{"id":"first","id":"second"}}"#, TEST_LIMITS)
                .expect_err("nested duplicate must fail");
        assert!(format!("{error:#}").contains("duplicate"));
    }

    #[test]
    fn rejects_depth_node_string_and_trailing_data_limits() {
        assert!(parse_strict_bounded_str(r#"[[[[null]]]]"#, TEST_LIMITS).is_err());
        assert!(parse_strict_bounded_str(r#"[0,1,2,3,4,5,6,7]"#, TEST_LIMITS).is_err());
        assert!(parse_strict_bounded_str(r#""123456789""#, TEST_LIMITS).is_err());
        assert!(parse_strict_bounded_str("{} {}", TEST_LIMITS).is_err());
    }
}
