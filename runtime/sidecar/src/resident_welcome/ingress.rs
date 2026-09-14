use super::{
    protocol::{self, SigningKey},
    store::Store,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub struct Response {
    pub status: u16,
    pub body: Value,
}

/// Mountable receiver core: ACK only after the database transaction commits. This
/// module is not mounted on a production listener in this local development slice.
pub async fn receive(
    store: &Store,
    method: &str,
    path: &str,
    headers: &BTreeMap<String, String>,
    raw: &[u8],
    keys: &[SigningKey],
    now: i64,
) -> Response {
    if method != "POST" || path != protocol::PATH {
        return Response {
            status: 404,
            body: json!({"code":"not_found"}),
        };
    }
    let event = match protocol::verify(raw, headers, keys, now) {
        Ok(e) => e,
        Err(e) => {
            return Response {
                status: e.status,
                body: json!({"code":e.code}),
            }
        }
    };
    match store.accept(&event).await {
        Ok(ack) => Response {
            status: if ack.status == "accepted" { 202 } else { 200 },
            body: serde_json::to_value(ack).unwrap(),
        },
        Err(e) => {
            if let Some(r) = e.downcast_ref::<protocol::Rejection>() {
                Response {
                    status: r.status,
                    body: json!({"code":r.code}),
                }
            } else {
                Response {
                    status: 503,
                    body: json!({"code":"inbox_unavailable"}),
                }
            }
        }
    }
}
