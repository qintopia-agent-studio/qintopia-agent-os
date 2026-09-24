//! Welcome-only subjects and explicit effects. Not PMS authorization DTOs.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(
    tag = "kind",
    content = "id",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub(crate) enum SubjectRef {
    Person(Uuid),
    WorkAccount(Uuid),
}
impl SubjectRef {
    pub(crate) fn kind(&self) -> &'static str {
        match self {
            Self::Person(_) => "person",
            Self::WorkAccount(_) => "work_account",
        }
    }
    pub(crate) fn id(&self) -> Uuid {
        match self {
            Self::Person(id) | Self::WorkAccount(id) => *id,
        }
    }
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReviewGrant {
    pub subject: SubjectRef,
    pub effects: Vec<String>,
    pub valid_until: DateTime<Utc>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReviewSettings {
    pub scope: Uuid,
    pub conversation: Uuid,
    pub expected_version: i64,
    pub reviewers: Vec<ReviewGrant>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReviewOpen {
    pub work_item: Uuid,
    pub scope: Uuid,
    pub case_ref: Uuid,
    pub application: Uuid,
    pub artifacts: Vec<Uuid>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReviewDecision {
    pub operation_id: Uuid,
    pub work_item: Uuid,
    pub expected_version: i64,
    /// Explicitly selected candidate; never used as the acting subject.
    pub person: Option<Uuid>,
    pub channel: Option<Uuid>,
    pub confirm_application_stay: bool,
    pub confirm_channel_person: bool,
    pub confirm_content: bool,
    /// confirm, reject, or revoke; no generic "agree" operation.
    pub decision: String,
}
