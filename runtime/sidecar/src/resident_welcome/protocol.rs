use super::digest;
use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use serde::{
    de::{self, MapAccess, SeqAccess, Visitor},
    Deserialize, Deserializer, Serialize,
};
use serde_json::Value;
use sha2::Sha256;
use std::{
    collections::{BTreeMap, HashSet},
    fmt,
};

pub const PATH: &str = "/api/v1/ingress/pms/events";
pub const MAX_BODY: usize = 65536;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rejection {
    pub status: u16,
    pub code: &'static str,
}
impl fmt::Display for Rejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code)
    }
}
impl std::error::Error for Rejection {}
pub fn reject(status: u16, code: &'static str) -> Rejection {
    Rejection { status, code }
}

// Recursive duplicate-key rejection, including ignored extension fields. serde's
// ordinary Value parser silently replaces duplicate keys; it is not sufficient.
struct Unique(Value);
impl<'de> Deserialize<'de> for Unique {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Unique;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("unique JSON")
            }
            fn visit_map<M: MapAccess<'de>>(self, mut m: M) -> Result<Unique, M::Error> {
                let mut out = serde_json::Map::new();
                while let Some((k, Unique(v))) = m.next_entry::<String, Unique>()? {
                    if out.insert(k, v).is_some() {
                        return Err(de::Error::custom("duplicate key"));
                    }
                }
                Ok(Unique(Value::Object(out)))
            }
            fn visit_seq<S: SeqAccess<'de>>(self, mut s: S) -> Result<Unique, S::Error> {
                let mut out = Vec::new();
                while let Some(Unique(v)) = s.next_element()? {
                    out.push(v);
                }
                Ok(Unique(Value::Array(out)))
            }
            fn visit_bool<E: de::Error>(self, v: bool) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_f64<E: de::Error>(self, v: f64) -> Result<Unique, E> {
                serde_json::Number::from_f64(v)
                    .map(|n| Unique(n.into()))
                    .ok_or_else(|| de::Error::custom("number"))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_unit<E: de::Error>(self) -> Result<Unique, E> {
                Ok(Unique(Value::Null))
            }
        }
        d.deserialize_any(V)
    }
}

pub fn parse_unique(raw: &[u8]) -> Result<Value, Rejection> {
    parse_unique_bounded(raw, MAX_BODY)
}

pub(crate) fn parse_unique_bounded(raw: &[u8], max: usize) -> Result<Value, Rejection> {
    if raw.len() > max {
        return Err(reject(413, "body_too_large"));
    }
    let value: Unique = serde_json::from_slice(raw).map_err(|_| reject(400, "invalid_json"))?;
    fn bounded(v: &Value, depth: usize) -> bool {
        depth <= 16
            && match v {
                Value::Object(m) => m.values().all(|v| bounded(v, depth + 1)),
                Value::Array(a) => a.iter().all(|v| bounded(v, depth + 1)),
                _ => true,
            }
    }
    if !bounded(&value.0, 0) {
        return Err(reject(400, "json_too_deep"));
    }
    Ok(value.0)
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Envelope {
    pub schema_version: String,
    pub event_id: String,
    pub source_instance: String,
    pub property_id: String,
    pub event_type: String,
    pub aggregate_type: String,
    pub aggregate_id: String,
    pub aggregate_revision: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain_version: Option<String>,
    pub recorded_at: DateTime<Utc>,
    pub effective_at: Option<DateTime<Utc>>,
    pub source_fact_ref: String,
    pub publish_seq: String,
    pub refs: BTreeMap<String, String>,
    pub origin: String,
}

pub fn decimal(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 78
        && value.bytes().all(|b| b.is_ascii_digit())
        && (value == "0" || !value.starts_with('0'))
}
pub fn reference(value: &str) -> bool {
    !value.is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
}

pub fn decode(raw: &[u8]) -> Result<Envelope, Rejection> {
    let value = parse_unique(raw)?;
    if value.get("effective_at").is_none() {
        return Err(reject(422, "missing_effective_at"));
    }
    let e: Envelope = serde_json::from_value(value).map_err(|_| reject(422, "invalid_envelope"))?;
    let expected = match e.event_type.as_str() {
        "pms.order.created"
        | "pms.order.context_changed"
        | "pms.stay.checked_in"
        | "pms.stay.arrangement_changed"
        | "pms.stay.checked_out"
        | "pms.stay.cancelled"
        | "pms.stay.no_show"
        | "pms.stay.check_in_revoked"
        | "pms.stay.check_out_revoked"
        | "pms.order.occupants_changed" => "order",
        "pms.member.context_changed" => "member",
        "pms.inventory_unit.context_changed" => "inventory_unit",
        "pms.entity.invalidated"
            if matches!(
                e.aggregate_type.as_str(),
                "order" | "member" | "inventory_unit"
            ) =>
        {
            &e.aggregate_type
        }
        _ => return Err(reject(422, "unsupported_event_type")),
    };
    if e.schema_version != "pms.events.v1"
        || e.aggregate_type != expected
        || !matches!(
            e.origin.as_str(),
            "live" | "historical_correction" | "baseline"
        )
        || !decimal(&e.aggregate_revision)
        || !decimal(&e.publish_seq)
        || e.domain_version.as_ref().is_some_and(|v| !decimal(v))
        || [
            &e.event_id,
            &e.source_instance,
            &e.property_id,
            &e.aggregate_id,
            &e.source_fact_ref,
        ]
        .iter()
        .any(|v| !reference(v))
        || e.refs.iter().any(|(k, v)| {
            !matches!(
                k.as_str(),
                "order_id" | "stay_id" | "member_id" | "inventory_unit_id" | "occupant_id"
            ) || !reference(v)
        })
    {
        return Err(reject(422, "invalid_contract"));
    }
    Ok(e)
}

pub struct SigningKey {
    pub key_id: String,
    pub secret: zeroize::Zeroizing<Vec<u8>>,
    pub source_instance: String,
    pub properties: HashSet<String>,
}

pub struct VerifiedEvent {
    pub(crate) envelope: Envelope,
    pub(crate) body_hash: String,
}
impl VerifiedEvent {
    pub fn envelope(&self) -> &Envelope {
        &self.envelope
    }
    pub fn from_feed(raw: &[u8], source: &str, property: &str) -> Result<Self, Rejection> {
        let envelope = decode(raw)?;
        if envelope.source_instance != source || envelope.property_id != property {
            return Err(reject(403, "source_scope_mismatch"));
        }
        Ok(Self {
            envelope,
            body_hash: digest(raw),
        })
    }
}

pub fn verify(
    raw: &[u8],
    headers: &BTreeMap<String, String>,
    keys: &[SigningKey],
    now: i64,
) -> Result<VerifiedEvent, Rejection> {
    if raw.len() > MAX_BODY {
        return Err(reject(413, "body_too_large"));
    }
    if headers.contains_key("content-encoding")
        || headers
            .get("content-type")
            .is_none_or(|s| s != "application/json")
    {
        return Err(reject(400, "unsupported_encoding"));
    }
    let get = |k: &str| {
        headers
            .get(k)
            .map(String::as_str)
            .ok_or_else(|| reject(401, "missing_signature"))
    };
    let key = keys
        .iter()
        .find(|k| get("x-qt-key-id").ok() == Some(k.key_id.as_str()))
        .ok_or_else(|| reject(401, "unknown_key"))?;
    let sent = get("x-qt-sent-at")?;
    let timestamp: i64 = sent.parse().map_err(|_| reject(401, "invalid_timestamp"))?;
    if !decimal(sent) || timestamp.abs_diff(now) > 300 {
        return Err(reject(401, "clock_window"));
    }
    let delivery = get("x-qt-delivery-id")?;
    if !reference(delivery) {
        return Err(reject(401, "invalid_delivery_id"));
    }
    let signature = get("x-qt-signature")?;
    if signature.len() != 64
        || !signature
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(reject(401, "invalid_signature"));
    }
    let bytes: Vec<u8> = (0..64)
        .step_by(2)
        .map(|i| u8::from_str_radix(&signature[i..i + 2], 16).unwrap())
        .collect();
    let message = format!("POST\n{PATH}\n{sent}\n{delivery}\n{}", digest(raw));
    let mut mac =
        Hmac::<Sha256>::new_from_slice(&key.secret).map_err(|_| reject(401, "invalid_key"))?;
    mac.update(message.as_bytes());
    mac.verify_slice(&bytes)
        .map_err(|_| reject(401, "invalid_signature"))?;
    VerifiedEvent::from_feed(raw, &key.source_instance, decode(raw)?.property_id.as_str()).and_then(
        |e| {
            if key.properties.contains(&e.envelope.property_id) {
                Ok(e)
            } else {
                Err(reject(403, "property_forbidden"))
            }
        },
    )
}
