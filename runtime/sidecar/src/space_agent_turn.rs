use std::collections::BTreeSet;

use anyhow::{bail, Context, Result};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

pub(crate) const HANDOFF_STATE: &str = "queued_for_runner";
pub(crate) const EXECUTOR_BOUNDARY: &str = "dedicated_space_agent_turn_broker_v1";
pub(crate) const RUNNER_IDENTITY: &str = "erhua-space-agent-runner-v1";
pub(crate) const RUNNER_CONTRACT_VERSION: u8 = 1;
pub(crate) const RUNTIME_READY_ENV: &str = "QINTOPIA_SPACE_AGENT_TURN_RUNTIME_READY";
pub(crate) const RUNTIME_APPROVAL_ENV: &str = "QINTOPIA_SPACE_AGENT_TURN_RUNTIME_APPROVAL";
pub(crate) const RUNTIME_APPROVAL_PHRASE: &str =
    "approved-production-space-agent-turn-runtime-readiness";

const MAX_CONTRACT_DEPTH: usize = 8;
const MAX_PROPERTIES: usize = 32;
const MAX_ENUM_ITEMS: usize = 32;
const MAX_ARRAY_ITEMS: usize = 64;
const MAX_STRING_CHARS: usize = 4_000;
const MAX_CONTRACT_BYTES: usize = 64 * 1024;
const MAX_OUTPUT_BYTES: usize = 64 * 1024;
const MAX_SAFE_JSON_INTEGER: i64 = 9_007_199_254_740_991;

pub(crate) fn runtime_readiness() -> Result<bool> {
    let ready = std::env::var(RUNTIME_READY_ENV).ok();
    let approval = std::env::var(RUNTIME_APPROVAL_ENV).ok();
    runtime_readiness_from_values(ready.as_deref(), approval.as_deref())
}

pub(crate) fn runtime_readiness_is_approved() -> bool {
    runtime_readiness().unwrap_or(false)
}

fn runtime_readiness_from_values(ready: Option<&str>, approval: Option<&str>) -> Result<bool> {
    match ready {
        None | Some("0") => Ok(false),
        Some("1") if approval == Some(RUNTIME_APPROVAL_PHRASE) => bail!(
            "agent_turn production runtime is not provisioned in this Release; use only the separately reviewed manual runner rehearsal"
        ),
        Some("1") => bail!("agent_turn runtime readiness requires explicit owner approval"),
        Some(_) => bail!("agent_turn runtime readiness flag must be 0 or 1"),
    }
}

pub(crate) fn validate_output_contract(contract: &Value) -> Result<()> {
    let encoded = serde_json::to_vec(contract).context("encode agent_turn output_contract")?;
    if encoded.len() > MAX_CONTRACT_BYTES {
        bail!("agent_turn output_contract exceeds the byte limit");
    }
    let object = contract
        .as_object()
        .context("agent_turn output_contract must be an object")?;
    if object.get("type").and_then(Value::as_str) != Some("object") {
        bail!("agent_turn output_contract root type must be object");
    }
    validate_schema_node(contract, 0, true)
}

pub(crate) fn validate_output(contract: &Value, output: &Value) -> Result<()> {
    validate_output_contract(contract)?;
    let encoded = serde_json::to_vec(output).context("encode agent_turn output")?;
    if encoded.len() > MAX_OUTPUT_BYTES {
        bail!("agent_turn output exceeds the byte limit");
    }
    validate_value_against_schema(contract, output, 0)
}

pub(crate) fn output_contract_digest(contract: &Value) -> Result<String> {
    validate_output_contract(contract)?;
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(contract).context("encode output contract")?)
    ))
}

fn validate_schema_node(schema: &Value, depth: usize, root: bool) -> Result<()> {
    if depth > MAX_CONTRACT_DEPTH {
        bail!("agent_turn output_contract exceeds the maximum depth");
    }
    let object = schema
        .as_object()
        .context("output_contract schema nodes must be objects")?;
    let schema_type = object
        .get("type")
        .and_then(Value::as_str)
        .context("output_contract schema node type is required")?;
    let common = ["type", "description", "enum", "const"];
    let type_fields: &[&str] = match schema_type {
        "object" => &["properties", "required", "additionalProperties"],
        "array" => &["items", "minItems", "maxItems"],
        "string" => &["minLength", "maxLength"],
        "integer" => &["minimum", "maximum"],
        "boolean" | "null" => &[],
        _ => bail!("output_contract contains an unsupported type"),
    };
    for key in object.keys() {
        if !common.contains(&key.as_str()) && !type_fields.contains(&key.as_str()) {
            bail!("output_contract contains an unsupported schema keyword");
        }
    }
    if let Some(description) = object.get("description") {
        let description = description
            .as_str()
            .context("output_contract description must be a string")?;
        if description.chars().count() > 300 || contains_unsafe_control(description) {
            bail!("output_contract description is invalid");
        }
    }
    match schema_type {
        "object" => validate_object_schema(object, depth, root)?,
        "array" => validate_array_schema(object, depth)?,
        "string" => validate_string_schema(object)?,
        "integer" => validate_integer_schema(object)?,
        "boolean" | "null" => {}
        _ => unreachable!(),
    }
    validate_enum_and_const(object, schema_type)
}

fn validate_object_schema(object: &Map<String, Value>, depth: usize, root: bool) -> Result<()> {
    let properties = object
        .get("properties")
        .and_then(Value::as_object)
        .context("object output_contract properties are required")?;
    if properties.len() > MAX_PROPERTIES || (root && properties.is_empty()) {
        bail!("object output_contract property count is invalid");
    }
    if object.get("additionalProperties") != Some(&Value::Bool(false)) {
        bail!("object output_contract must set additionalProperties to false");
    }
    for (name, schema) in properties {
        validate_property_name(name)?;
        validate_schema_node(schema, depth + 1, false)?;
    }
    let required = object
        .get("required")
        .and_then(Value::as_array)
        .context("object output_contract required must be an array")?;
    if required.len() > properties.len() {
        bail!("object output_contract required contains too many entries");
    }
    let mut seen = BTreeSet::new();
    for value in required {
        let name = value
            .as_str()
            .context("output_contract required entries must be strings")?;
        if !properties.contains_key(name) || !seen.insert(name) {
            bail!("output_contract required contains an unknown or duplicate property");
        }
    }
    Ok(())
}

fn validate_array_schema(object: &Map<String, Value>, depth: usize) -> Result<()> {
    let items = object
        .get("items")
        .context("array output_contract items are required")?;
    validate_schema_node(items, depth + 1, false)?;
    let min = bounded_usize(object.get("minItems"), "minItems", 0, MAX_ARRAY_ITEMS)?;
    let max = bounded_usize(
        object.get("maxItems"),
        "maxItems",
        MAX_ARRAY_ITEMS,
        MAX_ARRAY_ITEMS,
    )?;
    if min > max {
        bail!("array output_contract minItems exceeds maxItems");
    }
    Ok(())
}

fn validate_string_schema(object: &Map<String, Value>) -> Result<()> {
    let min = bounded_usize(object.get("minLength"), "minLength", 0, MAX_STRING_CHARS)?;
    let max = bounded_usize(
        object.get("maxLength"),
        "maxLength",
        MAX_STRING_CHARS,
        MAX_STRING_CHARS,
    )?;
    if min > max {
        bail!("string output_contract minLength exceeds maxLength");
    }
    Ok(())
}

fn validate_integer_schema(object: &Map<String, Value>) -> Result<()> {
    let minimum = optional_safe_integer(object.get("minimum"), "minimum")?;
    let maximum = optional_safe_integer(object.get("maximum"), "maximum")?;
    if minimum.zip(maximum).is_some_and(|(min, max)| min > max) {
        bail!("numeric output_contract minimum exceeds maximum");
    }
    Ok(())
}

fn validate_enum_and_const(object: &Map<String, Value>, schema_type: &str) -> Result<()> {
    if object.contains_key("enum") && object.contains_key("const") {
        bail!("output_contract node cannot contain both enum and const");
    }
    if let Some(values) = object.get("enum") {
        let values = values
            .as_array()
            .context("output_contract enum must be an array")?;
        if values.is_empty() || values.len() > MAX_ENUM_ITEMS {
            bail!("output_contract enum item count is invalid");
        }
        let mut seen = BTreeSet::new();
        for value in values {
            validate_literal_type(value, schema_type)?;
            let encoded = serde_json::to_string(value).context("encode output_contract enum")?;
            if !seen.insert(encoded) {
                bail!("output_contract enum contains duplicate values");
            }
        }
    }
    if let Some(value) = object.get("const") {
        validate_literal_type(value, schema_type)?;
    }
    Ok(())
}

fn validate_literal_type(value: &Value, schema_type: &str) -> Result<()> {
    let matches = match schema_type {
        "string" => value.is_string(),
        "integer" => safe_integer(value).is_some(),
        "boolean" => value.is_boolean(),
        "null" => value.is_null(),
        "object" | "array" => false,
        _ => false,
    };
    if !matches {
        bail!("output_contract literal does not match its declared type");
    }
    if value.as_str().is_some_and(|value| {
        value.chars().count() > MAX_STRING_CHARS || contains_unsafe_control(value)
    }) {
        bail!("output_contract string literal is outside bounded limits");
    }
    Ok(())
}

fn validate_value_against_schema(schema: &Value, value: &Value, depth: usize) -> Result<()> {
    if depth > MAX_CONTRACT_DEPTH {
        bail!("agent_turn output exceeds the maximum depth");
    }
    let object = schema
        .as_object()
        .context("validated schema is not an object")?;
    let schema_type = object
        .get("type")
        .and_then(Value::as_str)
        .context("validated schema type is missing")?;
    if let Some(expected) = object.get("const") {
        if value != expected {
            bail!("agent_turn output does not match const");
        }
    }
    if let Some(values) = object.get("enum").and_then(Value::as_array) {
        if !values.contains(value) {
            bail!("agent_turn output is outside enum");
        }
    }
    match schema_type {
        "object" => validate_output_object(object, value, depth),
        "array" => validate_output_array(object, value, depth),
        "string" => validate_output_string(object, value),
        "integer" => {
            let number = safe_integer(value).context("agent_turn output must be an integer")?;
            validate_output_integer(object, number)
        }
        "boolean" if value.is_boolean() => Ok(()),
        "null" if value.is_null() => Ok(()),
        "boolean" | "null" => bail!("agent_turn output type does not match contract"),
        _ => bail!("agent_turn output contract type is unsupported"),
    }
}

fn validate_output_object(schema: &Map<String, Value>, value: &Value, depth: usize) -> Result<()> {
    let value = value
        .as_object()
        .context("agent_turn output must be an object")?;
    let properties = schema
        .get("properties")
        .and_then(Value::as_object)
        .context("validated object schema properties are missing")?;
    if value.keys().any(|key| !properties.contains_key(key)) {
        bail!("agent_turn output contains an additional property");
    }
    for required in schema
        .get("required")
        .and_then(Value::as_array)
        .context("validated object schema required is missing")?
    {
        let name = required
            .as_str()
            .context("validated required entry is invalid")?;
        if !value.contains_key(name) {
            bail!("agent_turn output is missing a required property");
        }
    }
    for (key, child) in value {
        validate_value_against_schema(
            properties
                .get(key)
                .context("validated output property schema is missing")?,
            child,
            depth + 1,
        )?;
    }
    Ok(())
}

fn validate_output_array(schema: &Map<String, Value>, value: &Value, depth: usize) -> Result<()> {
    let values = value
        .as_array()
        .context("agent_turn output must be an array")?;
    let min = schema.get("minItems").and_then(Value::as_u64).unwrap_or(0) as usize;
    let max = schema
        .get("maxItems")
        .and_then(Value::as_u64)
        .unwrap_or(MAX_ARRAY_ITEMS as u64) as usize;
    if values.len() < min || values.len() > max {
        bail!("agent_turn output array length is outside contract");
    }
    let item_schema = schema
        .get("items")
        .context("validated array schema items are missing")?;
    for child in values {
        validate_value_against_schema(item_schema, child, depth + 1)?;
    }
    Ok(())
}

fn validate_output_string(schema: &Map<String, Value>, value: &Value) -> Result<()> {
    let value = value
        .as_str()
        .context("agent_turn output must be a string")?;
    if contains_unsafe_control(value) {
        bail!("agent_turn output string contains unsafe control characters");
    }
    let length = value.chars().count();
    let min = schema.get("minLength").and_then(Value::as_u64).unwrap_or(0) as usize;
    let max = schema
        .get("maxLength")
        .and_then(Value::as_u64)
        .unwrap_or(MAX_STRING_CHARS as u64) as usize;
    if length < min || length > max {
        bail!("agent_turn output string length is outside contract");
    }
    Ok(())
}

fn validate_output_integer(schema: &Map<String, Value>, value: i64) -> Result<()> {
    if schema
        .get("minimum")
        .and_then(safe_integer)
        .is_some_and(|minimum| value < minimum)
        || schema
            .get("maximum")
            .and_then(safe_integer)
            .is_some_and(|maximum| value > maximum)
    {
        bail!("agent_turn output integer is outside contract");
    }
    Ok(())
}

fn validate_property_name(name: &str) -> Result<()> {
    let valid_shape = !name.is_empty()
        && name.len() <= 80
        && name.is_ascii()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_');
    let normalized = name.to_ascii_lowercase();
    let forbidden = [
        "authorization",
        "credential",
        "destination",
        "password",
        "room_id",
        "chat_id",
        "space_id",
        "secret",
        "target",
        "token",
        "uri",
        "url",
    ];
    if !valid_shape || forbidden.iter().any(|term| normalized.contains(term)) {
        bail!("output_contract property name is invalid or routing-sensitive");
    }
    Ok(())
}

fn bounded_usize(
    value: Option<&Value>,
    name: &str,
    default: usize,
    maximum: usize,
) -> Result<usize> {
    let Some(value) = value else {
        return Ok(default);
    };
    let value = value
        .as_u64()
        .with_context(|| format!("output_contract {name} must be an unsigned integer"))?;
    let value = usize::try_from(value).context("output_contract bound does not fit usize")?;
    if value > maximum {
        bail!("output_contract {name} exceeds the supported limit");
    }
    Ok(value)
}

fn optional_safe_integer(value: Option<&Value>, name: &str) -> Result<Option<i64>> {
    value
        .map(|value| {
            safe_integer(value)
                .with_context(|| format!("output_contract {name} must be a precise JSON integer"))
        })
        .transpose()
}

fn safe_integer(value: &Value) -> Option<i64> {
    if let Some(value) = value.as_i64() {
        return (-MAX_SAFE_JSON_INTEGER..=MAX_SAFE_JSON_INTEGER)
            .contains(&value)
            .then_some(value);
    }
    value
        .as_u64()
        .filter(|value| *value <= MAX_SAFE_JSON_INTEGER as u64)
        .map(|value| value as i64)
}

fn contains_unsafe_control(value: &str) -> bool {
    value.chars().any(|character| {
        (character.is_control() && !matches!(character, '\n' | '\t'))
            || matches!(
                character,
                '\u{061c}'
                    | '\u{200e}'
                    | '\u{200f}'
                    | '\u{202a}'..='\u{202e}'
                    | '\u{2066}'..='\u{2069}'
            )
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn contract() -> Value {
        json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["summary", "items"],
            "properties": {
                "summary": {"type": "string", "minLength": 1, "maxLength": 200},
                "items": {
                    "type": "array",
                    "minItems": 0,
                    "maxItems": 8,
                    "items": {
                        "type": "object",
                        "additionalProperties": false,
                        "required": ["label", "done"],
                        "properties": {
                            "label": {"type": "string", "maxLength": 100},
                            "done": {"type": "boolean"}
                        }
                    }
                }
            }
        })
    }

    #[test]
    fn runtime_readiness_stays_disabled_until_a_provisioning_release() {
        assert!(!runtime_readiness_from_values(None, None).unwrap());
        assert!(!runtime_readiness_from_values(Some("0"), None).unwrap());
        let unavailable = runtime_readiness_from_values(Some("1"), Some(RUNTIME_APPROVAL_PHRASE))
            .expect_err("approval alone cannot prove broker and runner readiness");
        assert!(unavailable
            .to_string()
            .contains("not provisioned in this Release"));
        assert!(runtime_readiness_from_values(Some("1"), None).is_err());
        assert!(runtime_readiness_from_values(Some("yes"), None).is_err());
    }

    #[test]
    fn business_owned_contract_accepts_only_matching_output() {
        let contract = contract();
        validate_output_contract(&contract).expect("bounded contract");
        validate_output(
            &contract,
            &json!({"summary": "today", "items": [{"label": "call", "done": false}]}),
        )
        .expect("matching output");
        assert!(validate_output(&contract, &json!({"summary": "today"})).is_err());
        assert!(validate_output(
            &contract,
            &json!({"summary": "today", "items": [], "target_group_id": "forged"})
        )
        .is_err());
    }

    #[test]
    fn contract_rejects_open_objects_and_routing_fields() {
        assert!(validate_output_contract(&json!({
            "type": "object",
            "properties": {"summary": {"type": "string"}},
            "required": ["summary"]
        }))
        .is_err());
        assert!(validate_output_contract(&json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {"target_url": {"type": "string"}},
            "required": ["target_url"]
        }))
        .is_err());
        assert!(validate_output_contract(&json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "summary": {"type": "string", "const": "x".repeat(MAX_STRING_CHARS + 1)}
            },
            "required": ["summary"]
        }))
        .is_err());
    }

    #[test]
    fn contract_digest_is_stable_for_the_exact_definition() {
        assert_eq!(
            output_contract_digest(&contract()).expect("contract digest"),
            output_contract_digest(&contract()).expect("same contract digest")
        );
    }

    #[test]
    fn numeric_contracts_reject_values_outside_the_precise_json_range() {
        let integer_contract = json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {"count": {"type": "integer"}},
            "required": ["count"]
        });
        validate_output(&integer_contract, &json!({"count": MAX_SAFE_JSON_INTEGER}))
            .expect("largest precise integer");
        assert!(validate_output(
            &integer_contract,
            &json!({"count": MAX_SAFE_JSON_INTEGER + 1})
        )
        .is_err());
        assert!(validate_output_contract(&json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "count": {"type": "integer", "maximum": MAX_SAFE_JSON_INTEGER + 1}
            },
            "required": ["count"]
        }))
        .is_err());
        assert!(validate_output_contract(&json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {"score": {"type": "number"}},
            "required": ["score"]
        }))
        .is_err());
        assert!(validate_output_contract(&json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {"count": {"type": "integer", "minimum": 0.5}},
            "required": ["count"]
        }))
        .is_err());
    }

    #[test]
    fn strings_reject_carriage_returns_and_bidirectional_controls() {
        let contract = contract();
        for summary in ["line one\rline two", "trusted\u{202e}txt.exe"] {
            assert!(validate_output(&contract, &json!({"summary": summary, "items": []})).is_err());
        }
        validate_output(
            &contract,
            &json!({"summary": "line one\nline two\tindented", "items": []}),
        )
        .expect("bounded newline and tab remain valid");
    }
}
