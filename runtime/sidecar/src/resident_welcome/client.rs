//! PMS GET-only compensation client. Endpoints cannot be supplied by events.
use super::{
    protocol::{reject, VerifiedEvent},
    store::Store,
};
use anyhow::{ensure, Result};
use serde::Deserialize;
use serde_json::value::RawValue;
use url::Url;

#[derive(Deserialize)]
pub struct FeedPage {
    pub schema_version: String,
    pub source_instance: String,
    pub property_id: String,
    pub events: Vec<Box<RawValue>>,
    pub next_cursor: String,
    pub has_more: bool,
    pub head_cursor: String,
    pub retention_floor_cursor: String,
}
impl FeedPage {
    pub fn verified(&self, source: &str, property: &str) -> Result<Vec<VerifiedEvent>> {
        ensure!(
            self.schema_version == "pms.events.v1"
                && self.source_instance == source
                && self.property_id == property,
            "feed_scope_or_schema"
        );
        ensure!(
            self.events.len() <= 100
                && !self.next_cursor.is_empty()
                && !self.head_cursor.is_empty()
                && !self.retention_floor_cursor.is_empty(),
            "invalid_feed_page"
        );
        ensure!(
            !self.has_more || !self.events.is_empty(),
            "empty_nonterminal_page"
        );
        self.events
            .iter()
            .map(|raw| {
                VerifiedEvent::from_feed(raw.get().as_bytes(), source, property).map_err(Into::into)
            })
            .collect()
    }
}

pub struct Client {
    root: Url,
    token: zeroize::Zeroizing<String>,
    source: String,
    property: String,
    http: crate::bounded_http::HttpClient,
}
impl Client {
    #[cfg(feature = "welcome-synthetic-driver")]
    pub(super) fn synthetic(
        root: &str,
        token: String,
        source: String,
        property: String,
        enabled: bool,
    ) -> Result<Self> {
        ensure!(
            enabled && source.starts_with("synthetic-"),
            "synthetic_read_disabled"
        );
        let parsed = Url::parse(root).map_err(|_| anyhow::anyhow!("invalid local read root"))?;
        ensure!(
            parsed.scheme() == "http"
                && matches!(parsed.host_str(), Some("127.0.0.1" | "[::1]"))
                && parsed.username().is_empty()
                && parsed.password().is_none()
                && parsed.query().is_none()
                && parsed.fragment().is_none()
                && parsed.path() == "/",
            "literal_loopback_root_required"
        );
        let mut client = Self::new("https://127.0.0.1/", token, source, property)?;
        client.root = parsed;
        client.http = crate::bounded_http::HttpClient::synthetic_loopback();
        Ok(client)
    }
    pub fn scan(&self, after: Option<&str>) -> Result<super::recovery::ScanPage> {
        let mut endpoint = self.root.join("api/v1/integrations/agent-os/orders")?;
        endpoint
            .query_pairs_mut()
            .append_pair("propertyId", &self.property)
            .append_pair("limit", "100");
        if let Some(after) = after {
            ensure!(super::protocol::reference(after), "invalid_scan_position");
            endpoint.query_pairs_mut().append_pair("afterId", after);
        }
        let response = self
            .http
            .request(
                "GET",
                &endpoint,
                &[("Authorization", format!("Bearer {}", self.token.as_str()))],
                &[],
                16 * 1024 * 1024,
            )
            .map_err(|_| anyhow::anyhow!("pms_scan_unavailable"))?;
        ensure!(response.status == 200, "pms_scan_failed_not_deletion");
        super::protocol::parse_unique_bounded(&response.body, 16 * 1024 * 1024)?;
        let page: super::recovery::ScanPage = serde_json::from_slice(&response.body)?;
        page.verified(&self.source, &self.property, after)?;
        Ok(page)
    }
    fn get_projection(&self, collection: &str, id: &str) -> Result<serde_json::Value> {
        ensure!(
            matches!(collection, "orders" | "members" | "inventory-units"),
            "projection_route_forbidden"
        );
        ensure!(super::protocol::reference(id), "invalid_entity_ref");
        let mut endpoint = self.root.clone();
        endpoint
            .path_segments_mut()
            .map_err(|_| anyhow::anyhow!("invalid PMS root"))?
            .extend(["api", "v1", "integrations", "agent-os", collection, id]);
        endpoint
            .query_pairs_mut()
            .append_pair("propertyId", &self.property);
        let response = self
            .http
            .request(
                "GET",
                &endpoint,
                &[("Authorization", format!("Bearer {}", self.token.as_str()))],
                &[],
                1024 * 1024,
            )
            .map_err(|_| anyhow::anyhow!("pms_projection_unavailable"))?;
        ensure!(
            response.status == 200,
            "projection_read_failed_not_a_tombstone"
        );
        let raw = super::protocol::parse_unique_bounded(&response.body, 1024 * 1024)?;
        match collection {
            "orders" => super::projection::order(&raw, &self.source, &self.property, Some(id)),
            "members" => {
                super::projection::entity(&raw, &self.source, &self.property, "member", id)
            }
            _ => {
                super::projection::entity(&raw, &self.source, &self.property, "inventory_unit", id)
            }
        }
    }

    pub fn order(&self, id: &str) -> Result<serde_json::Value> {
        self.get_projection("orders", id)
    }
    pub fn member(&self, id: &str) -> Result<serde_json::Value> {
        self.get_projection("members", id)
    }
    pub fn inventory_unit(&self, id: &str) -> Result<serde_json::Value> {
        self.get_projection("inventory-units", id)
    }

    pub fn new(root: &str, token: String, source: String, property: String) -> Result<Self> {
        let root = Url::parse(root).map_err(|_| anyhow::anyhow!("invalid PMS root"))?;
        ensure!(
            root.scheme() == "https"
                && root.host_str().is_some()
                && root.username().is_empty()
                && root.password().is_none()
                && root.query().is_none()
                && root.fragment().is_none()
                && root.path() == "/",
            "fixed HTTPS root required"
        );
        ensure!(
            !token.is_empty() && !token.contains(['\r', '\n']),
            "invalid READ credential"
        );
        Ok(Self {
            root,
            token: zeroize::Zeroizing::new(token),
            source,
            property,
            http: crate::bounded_http::HttpClient::production(),
        })
    }

    /// Synchronous bounded client; workers call in a blocking task. The transport
    /// returns redirects without following them; no token crosses to another host.
    pub fn events(&self, cursor: Option<&str>) -> Result<FeedPage> {
        let mut endpoint = self.root.join("api/v1/integration-events")?;
        endpoint
            .query_pairs_mut()
            .append_pair("propertyId", &self.property)
            .append_pair("limit", "100");
        if let Some(cursor) = cursor {
            endpoint.query_pairs_mut().append_pair("cursor", cursor);
        }
        let response = self
            .http
            .request(
                "GET",
                &endpoint,
                &[("Authorization", format!("Bearer {}", self.token.as_str()))],
                &[],
                100 * 65536 + 65536,
            )
            .map_err(|_| anyhow::anyhow!("pms_transport_unavailable"))?;
        match response.status {
            200 => (),
            410 => return Err(reject(410, "CURSOR_EXPIRED").into()),
            401 | 403 => return Err(reject(403, "PMS_READ_FORBIDDEN").into()),
            400 => return Err(reject(400, "INVALID_CURSOR").into()),
            _ => return Err(reject(502, "PMS_FEED_UNAVAILABLE").into()),
        }
        // RawValue retains whitespace, escapes and object-key order for Inbox hash.
        super::protocol::parse_unique_bounded(&response.body, 100 * 65536 + 65536)?;
        let page: FeedPage =
            serde_json::from_slice(&response.body).map_err(|_| reject(422, "invalid_feed"))?;
        page.verified(&self.source, &self.property)?;
        Ok(page)
    }
}

pub async fn commit_feed(
    store: &Store,
    source: &str,
    property: &str,
    expected: Option<&str>,
    page: Result<FeedPage>,
) -> Result<bool> {
    let page = match page {
        Ok(page) => page,
        Err(error) => {
            if error
                .downcast_ref::<super::protocol::Rejection>()
                .is_some_and(|e| matches!(e.status, 400 | 403 | 410))
            {
                store.require_rebuild(source, property, None).await?;
            }
            return Err(error);
        }
    };
    let events = page.verified(source, property)?;
    ensure!(
        !page.has_more || Some(page.next_cursor.as_str()) != expected,
        "cursor_did_not_advance"
    );
    store
        .accept_page(source, property, expected, &page.next_cursor, &events)
        .await?;
    Ok(page.has_more)
}
