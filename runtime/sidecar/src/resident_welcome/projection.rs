//! Wire projection validation and the shared contract's string-only JCS hash.
use super::{
    digest,
    protocol::{decimal, reference},
    state::{Occupant, Snapshot, StayState},
};
use anyhow::{ensure, Result};
use chrono::{DateTime, NaiveDate, Utc};
use serde_json::{Map, Value};

fn fields(v: &Value, keys: &[&str]) -> Result<Value> {
    let mut out = Map::new();
    for key in keys {
        out.insert(
            (*key).into(),
            v.get(*key)
                .ok_or_else(|| anyhow::anyhow!("projection_field_missing"))?
                .clone(),
        );
    }
    Ok(Value::Object(out))
}
fn text<'a>(v: &'a Value, k: &str) -> Result<&'a str> {
    v.get(k)
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("projection_string_required"))
}
fn enumeration(v: &Value, k: &str, values: &[&str]) -> Result<()> {
    ensure!(values.contains(&text(v, k)?), "projection_enum_invalid");
    Ok(())
}
fn date(v: &Value, k: &str) -> Result<NaiveDate> {
    Ok(NaiveDate::parse_from_str(text(v, k)?, "%Y-%m-%d")?)
}
fn array(v: &Value, k: &str) -> Result<Vec<Value>> {
    Ok(v.get(k)
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("projection_array_required"))?
        .clone())
}
fn sorted(mut rows: Vec<Value>, keys: &[&str]) -> Vec<Value> {
    rows.sort_by(|a, b| {
        for key in keys {
            let order = a[*key]
                .as_str()
                .unwrap_or("")
                .encode_utf16()
                .cmp(b[*key].as_str().unwrap_or("").encode_utf16());
            if !order.is_eq() {
                return order;
            }
        }
        std::cmp::Ordering::Equal
    });
    rows
}
pub fn canonical(v: &Value) -> Result<String> {
    Ok(match v {
        Value::Null => "null".into(),
        Value::Bool(b) => b.to_string(),
        Value::String(s) => serde_json::to_string(s)?,
        Value::Array(a) => format!(
            "[{}]",
            a.iter()
                .map(canonical)
                .collect::<Result<Vec<_>>>()?
                .join(",")
        ),
        Value::Object(m) => {
            let mut keys = m.keys().collect::<Vec<_>>();
            keys.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
            let entries = keys
                .into_iter()
                .map(|k| {
                    Ok(format!(
                        "{}:{}",
                        serde_json::to_string(k)?,
                        canonical(&m[k])?
                    ))
                })
                .collect::<Result<Vec<_>>>()?;
            format!("{{{}}}", entries.join(","))
        }
        Value::Number(_) => anyhow::bail!("numeric_projection_field_forbidden"),
    })
}
fn meta(raw: &Value, source: &str, property: &str) -> Result<()> {
    ensure!(
        text(raw, "schema_version")? == "pms.projections.v1"
            && text(raw, "source_instance")? == source
            && text(raw, "property_id")? == property,
        "projection_scope_mismatch"
    );
    let hash = text(raw, "projection_hash")?;
    ensure!(
        hash.len() == 64
            && hash
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "projection_hash_invalid"
    );
    DateTime::parse_from_rfc3339(text(raw, "observed_at")?)?;
    Ok(())
}
fn verify_hash(value: &Value, order: bool) -> Result<()> {
    let mut stable = value.clone();
    let m = stable.as_object_mut().unwrap();
    m.remove("projection_hash");
    m.remove("observed_at");
    if order {
        m.remove("read_context");
    }
    ensure!(
        digest(canonical(&stable)?.as_bytes()) == text(value, "projection_hash")?,
        "projection_hash_conflict"
    );
    Ok(())
}

pub fn order(
    raw: &Value,
    source: &str,
    property: &str,
    expected_order: Option<&str>,
) -> Result<Value> {
    meta(raw, source, property)?;
    let mut v = fields(
        raw,
        &[
            "schema_version",
            "source_instance",
            "property_id",
            "projection_hash",
            "observed_at",
            "order_id",
            "order_revision",
            "stay_id",
            "stay_status",
            "fulfillment_state",
            "checked_in_at",
            "effective_arrangement",
            "occupants",
            "member_ref",
            "related_revisions",
            "read_context",
        ],
    )?;
    ensure!(
        reference(text(&v, "order_id")?)
            && reference(text(&v, "stay_id")?)
            && decimal(text(&v, "order_revision")?),
        "projection_reference_invalid"
    );
    ensure!(
        expected_order.is_none_or(|id| v["order_id"] == id),
        "order_scope_mismatch"
    );
    enumeration(
        &v,
        "stay_status",
        &[
            "PLANNED",
            "IN_HOUSE",
            "COMPLETED",
            "CANCELLED",
            "NO_SHOW",
            "CHECK_IN_REVOKED",
        ],
    )?;
    enumeration(
        &v,
        "fulfillment_state",
        &[
            "NOT_CHECKED_IN",
            "IN_HOUSE",
            "CHECKED_OUT",
            "CANCELLED",
            "NO_SHOW",
            "CHECK_IN_REVOKED",
        ],
    )?;
    let expected = match text(&v, "stay_status")? {
        "PLANNED" => "NOT_CHECKED_IN",
        "COMPLETED" => "CHECKED_OUT",
        other => other,
    };
    ensure!(
        text(&v, "fulfillment_state")? == expected,
        "contradictory_stay_state"
    );
    if !v["checked_in_at"].is_null() {
        DateTime::parse_from_rfc3339(text(&v, "checked_in_at")?)?;
    }
    let mut arrangement = fields(
        &v["effective_arrangement"],
        &[
            "presentation",
            "arrival_date",
            "departure_date",
            "intervals",
        ],
    )?;
    enumeration(
        &arrangement,
        "presentation",
        &[
            "CURRENT",
            "LAST",
            "BEFORE_CANCELLATION",
            "NO_SHOW_ORDER",
            "BEFORE_CHECK_IN_REVOCATION",
        ],
    )?;
    let arrival = date(&arrangement, "arrival_date")?;
    let departure = date(&arrangement, "departure_date")?;
    ensure!(arrival < departure, "invalid_arrangement_dates");
    let mut intervals = vec![];
    for interval in array(&arrangement, "intervals")? {
        let i = fields(
            &interval,
            &[
                "segment_id",
                "inventory_unit_id",
                "arrival_date",
                "departure_date",
                "inventory_kind",
                "room_id",
                "bed_id",
                "building_code",
                "inventory_active",
            ],
        )?;
        enumeration(&i, "inventory_kind", &["ROOM", "BED"])?;
        ensure!(
            i["inventory_active"].is_boolean()
                && (i["building_code"].is_null() || i["building_code"].is_string()),
            "invalid_inventory_state"
        );
        for k in ["segment_id", "inventory_unit_id", "room_id"] {
            ensure!(reference(text(&i, k)?), "invalid_interval_ref");
        }
        if i["inventory_kind"] == "ROOM" {
            ensure!(
                i["bed_id"].is_null() && i["room_id"] == i["inventory_unit_id"],
                "room_bed_mismatch"
            )
        } else {
            ensure!(
                i["bed_id"] == i["inventory_unit_id"] && i["room_id"] != i["inventory_unit_id"],
                "bed_parent_mismatch"
            )
        }
        ensure!(
            date(&i, "arrival_date")? >= arrival
                && date(&i, "departure_date")? <= departure
                && date(&i, "arrival_date")? < date(&i, "departure_date")?,
            "interval_dates_invalid"
        );
        intervals.push(i);
    }
    intervals = sorted(
        intervals,
        &[
            "arrival_date",
            "departure_date",
            "inventory_unit_id",
            "segment_id",
        ],
    );
    ensure!(
        !intervals.is_empty() && intervals.len() <= 1000,
        "invalid_interval_count"
    );
    ensure!(
        date(&intervals[0], "arrival_date")? == arrival
            && date(intervals.last().unwrap(), "departure_date")? == departure,
        "incomplete_arrangement"
    );
    for pair in intervals.windows(2) {
        ensure!(
            pair[0]["departure_date"] == pair[1]["arrival_date"],
            "arrangement_gap_or_overlap"
        );
    }
    arrangement["intervals"] = Value::Array(intervals.clone());
    v["effective_arrangement"] = arrangement;
    let mut occupants = vec![];
    let mut seen = std::collections::HashSet::new();
    for row in array(&v, "occupants")? {
        let o = fields(&row, &["occupant_id", "role", "registration_state"])?;
        enumeration(&o, "role", &["PRIMARY", "ADDITIONAL"])?;
        enumeration(&o, "registration_state", &["active", "removed"])?;
        ensure!(
            reference(text(&o, "occupant_id")?) && seen.insert(text(&o, "occupant_id")?.to_owned()),
            "duplicate_occupant"
        );
        occupants.push(o);
    }
    occupants.sort_by(|a, b| {
        (a["role"] != "PRIMARY")
            .cmp(&(b["role"] != "PRIMARY"))
            .then_with(|| {
                a["occupant_id"]
                    .as_str()
                    .unwrap()
                    .encode_utf16()
                    .cmp(b["occupant_id"].as_str().unwrap().encode_utf16())
            })
    });
    v["occupants"] = Value::Array(occupants);
    if !v["member_ref"].is_null() {
        v["member_ref"] = fields(&v["member_ref"], &["member_id"])?;
        ensure!(
            reference(text(&v["member_ref"], "member_id")?),
            "invalid_member_ref"
        );
    }
    let mut related = vec![];
    let mut references = std::collections::HashSet::new();
    for row in array(&v, "related_revisions")? {
        let r = fields(
            &row,
            &["aggregate_type", "aggregate_id", "aggregate_revision"],
        )?;
        enumeration(&r, "aggregate_type", &["member", "inventory_unit"])?;
        ensure!(
            reference(text(&r, "aggregate_id")?)
                && decimal(text(&r, "aggregate_revision")?)
                && references.insert((
                    text(&r, "aggregate_type")?.to_owned(),
                    text(&r, "aggregate_id")?.to_owned()
                )),
            "invalid_related_revision"
        );
        related.push(r);
    }
    for i in &intervals {
        for key in ["inventory_unit_id", "room_id"] {
            ensure!(
                references.contains(&("inventory_unit".into(), text(i, key)?.into())),
                "missing_inventory_revision"
            );
        }
    }
    if !v["member_ref"].is_null() {
        ensure!(
            references.contains(&("member".into(), text(&v["member_ref"], "member_id")?.into())),
            "missing_member_revision"
        );
    }
    v["related_revisions"] = Value::Array(sorted(related, &["aggregate_type", "aggregate_id"]));
    let mut context = fields(
        &v["read_context"],
        &[
            "property_timezone",
            "business_date",
            "temporal_state",
            "current_interval",
        ],
    )?;
    ensure!(
        !text(&context, "property_timezone")?.is_empty(),
        "timezone_required"
    );
    let today = date(&context, "business_date")?;
    let temporal = match text(&v, "stay_status")? {
        "PLANNED" => {
            if today < arrival {
                "NOT_STARTED"
            } else if today == arrival {
                "RESERVED_TODAY"
            } else {
                "OVERDUE_RESERVED"
            }
        }
        "IN_HOUSE" => {
            ensure!(today >= arrival, "in_house_before_arrival");
            if today < departure {
                "IN_HOUSE_TODAY"
            } else if today == departure {
                "DUE_OUT"
            } else {
                "OVERDUE_IN_HOUSE"
            }
        }
        _ => "TERMINAL",
    };
    ensure!(
        text(&context, "temporal_state")? == temporal,
        "temporal_state_conflict"
    );
    if matches!(temporal, "RESERVED_TODAY" | "IN_HOUSE_TODAY") {
        context["current_interval"] = fields(
            &context["current_interval"],
            &[
                "segment_id",
                "inventory_unit_id",
                "arrival_date",
                "departure_date",
            ],
        )?;
        let current = intervals
            .iter()
            .filter(|i| {
                date(i, "arrival_date").unwrap() <= today
                    && today < date(i, "departure_date").unwrap()
            })
            .collect::<Vec<_>>();
        ensure!(
            current.len() == 1
                && fields(
                    current[0],
                    &[
                        "segment_id",
                        "inventory_unit_id",
                        "arrival_date",
                        "departure_date"
                    ]
                )? == context["current_interval"],
            "current_interval_conflict"
        );
    } else {
        ensure!(
            context["current_interval"].is_null(),
            "unexpected_current_interval"
        );
    }
    v["read_context"] = context;
    verify_hash(&v, true)?;
    Ok(v)
}

pub fn entity(raw: &Value, source: &str, property: &str, kind: &str, id: &str) -> Result<Value> {
    meta(raw, source, property)?;
    let (id_field, revision_field) = match kind {
        "member" => ("member_id", "member_revision"),
        "inventory_unit" => ("inventory_unit_id", "inventory_revision"),
        _ => anyhow::bail!("unsupported_projection_kind"),
    };
    let mut keys = vec![
        "schema_version",
        "source_instance",
        "property_id",
        "projection_hash",
        "observed_at",
        id_field,
        revision_field,
        "resource_state",
    ];
    enumeration(raw, "resource_state", &["active", "tombstone"])?;
    if raw["resource_state"] == "tombstone" {
        keys.extend([
            "invalidation_kind",
            "invalidated_recorded_at",
            "source_fact_ref",
        ]);
    } else if kind == "member" {
        keys.push("external_references");
    } else {
        keys.extend(["kind", "parent_room_id", "building_code"]);
    }
    let mut v = fields(raw, &keys)?;
    ensure!(
        text(&v, id_field)? == id && decimal(text(&v, revision_field)?),
        "entity_scope_or_revision"
    );
    if v["resource_state"] == "tombstone" {
        enumeration(
            &v,
            "invalidation_kind",
            &[if kind == "member" {
                "BUSINESS_DELETED"
            } else {
                "INACTIVE"
            }],
        )?;
        ensure!(
            reference(text(&v, "source_fact_ref")?),
            "invalid_tombstone_ref"
        );
        if kind == "member" || !v["invalidated_recorded_at"].is_null() {
            DateTime::parse_from_rfc3339(text(&v, "invalidated_recorded_at")?)?;
        }
    } else if kind == "member" {
        let mut refs = vec![];
        for r in array(&v, "external_references")? {
            let r = fields(
                &r,
                &[
                    "reference_id",
                    "provider",
                    "source_container_id",
                    "source_table_id",
                    "external_record_id",
                ],
            )?;
            enumeration(&r, "provider", &["FEISHU_BASE"])?;
            for key in [
                "reference_id",
                "source_container_id",
                "source_table_id",
                "external_record_id",
            ] {
                ensure!(reference(text(&r, key)?), "invalid_application_ref");
            }
            refs.push(r);
        }
        v["external_references"] = Value::Array(sorted(
            refs,
            &[
                "provider",
                "source_container_id",
                "source_table_id",
                "external_record_id",
                "reference_id",
            ],
        ));
    } else {
        enumeration(&v, "kind", &["ROOM", "BED"])?;
        ensure!(
            (v["building_code"].is_null() || v["building_code"].is_string())
                && if v["kind"] == "ROOM" {
                    v["parent_room_id"].is_null()
                } else {
                    v["parent_room_id"].is_string()
                },
            "invalid_inventory_parent"
        );
    }
    verify_hash(&v, false)?;
    Ok(v)
}

pub fn normalize(v: &Value) -> Result<Snapshot> {
    let context = &v["read_context"];
    let arrangement = &v["effective_arrangement"];
    let intervals = array(arrangement, "intervals")?;
    let current = if v["stay_status"] == "PLANNED" && context["temporal_state"] == "NOT_STARTED" {
        intervals.first()
    } else {
        intervals.iter().find(|i| {
            fields(
                i,
                &[
                    "segment_id",
                    "inventory_unit_id",
                    "arrival_date",
                    "departure_date",
                ],
            )
            .ok()
            .as_ref()
                == Some(&context["current_interval"])
        })
    };
    Ok(Snapshot {
        source_hash: Some(text(v, "projection_hash")?.into()),
        source: text(v, "source_instance")?.into(),
        property: text(v, "property_id")?.into(),
        order: text(v, "order_id")?.into(),
        revision: text(v, "order_revision")?.into(),
        stay: text(v, "stay_id")?.into(),
        state: match text(v, "stay_status")? {
            "PLANNED" => StayState::Reserved,
            "IN_HOUSE" => StayState::InHouse,
            _ => StayState::Terminated,
        },
        inventory_reserved: arrangement["presentation"] == "CURRENT"
            && intervals.iter().all(|i| i["inventory_active"] == true),
        current_arrangement: arrangement["presentation"] == "CURRENT" && current.is_some(),
        building: current
            .and_then(|i| i["building_code"].as_str())
            .unwrap_or("")
            .into(),
        occupants: array(v, "occupants")?
            .iter()
            .map(|o| {
                Ok(Occupant {
                    id: text(o, "occupant_id")?.into(),
                    active: o["registration_state"] == "active",
                })
            })
            .collect::<Result<_>>()?,
        related_revisions: array(v, "related_revisions")?
            .iter()
            .map(|r| {
                Ok((
                    format!(
                        "{}/{}",
                        text(r, "aggregate_type")?,
                        text(r, "aggregate_id")?
                    ),
                    text(r, "aggregate_revision")?.into(),
                ))
            })
            .collect::<Result<_>>()?,
        observed_at: DateTime::parse_from_rfc3339(text(v, "observed_at")?)?.with_timezone(&Utc),
        business_date: date(context, "business_date")?,
    })
}
