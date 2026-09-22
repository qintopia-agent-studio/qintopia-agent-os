//! Typed collaboration commands and current authorization evaluation.
use anyhow::{bail, ensure, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionMode {
    Autonomous,
    Confirmation,
    Denied,
}

impl PermissionMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Autonomous => "autonomous",
            Self::Confirmation => "confirmation",
            Self::Denied => "denied",
        }
    }
}

impl std::str::FromStr for PermissionMode {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        match value {
            "autonomous" => Ok(Self::Autonomous),
            "confirmation" => Ok(Self::Confirmation),
            "denied" => Ok(Self::Denied),
            _ => bail!("invalid_permission_mode"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PermissionSetting {
    pub action: String,
    pub mode: PermissionMode,
    pub reviewer: Option<Uuid>,
}

pub const ACTIONS: &[&str] = &[
    "confirm_knowledge",
    "train",
    "change_rules",
    "review",
    "publish",
    "designate",
    "identity",
    "manage",
    "technical_support",
];
pub const DOMAINS: &[&str] = &[
    "community_service",
    "activity_operations",
    "hospitality",
    "technical_support",
    "organization",
];

// This is the versioned registry, not another Agent directory or a runtime-ready claim.
pub fn agents() -> Vec<&'static str> {
    include_str!("../../../../registry/agents.yaml")
        .lines()
        .filter_map(|s| s.strip_prefix("  - id: agents/"))
        .map(str::trim)
        .collect()
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Delegation {
    pub agents: Vec<String>,
    pub domains: Vec<String>,
    pub actions: Vec<String>,
    pub depth: i32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Assignment {
    pub collaboration: Option<Uuid>,
    pub person: Uuid,
    pub role: Uuid,
    #[serde(default)]
    pub duty: Option<Uuid>,
    pub scope: Uuid,
    pub agent: String,
    pub domain: String,
    pub responsibility: String,
    #[serde(default)]
    pub valid_from: Option<DateTime<Utc>>,
    pub valid_until: Option<DateTime<Utc>>,
    pub proxy_for: Option<Uuid>,
    #[serde(default)]
    pub actions: Vec<String>,
    #[serde(default)]
    pub permissions: Vec<PermissionSetting>,
    pub delegation: Option<Delegation>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Change {
    ConfigureWork {
        assignment: Box<Assignment>,
        audience: Audience,
    },
    SavePosition {
        id: Option<Uuid>,
        role: Uuid,
        scope: Uuid,
        parent: Option<Uuid>,
        label: String,
        description: String,
        draft: bool,
    },
    SaveLedger {
        id: Option<Uuid>,
        object: String,
        reference: Option<String>,
        label: String,
        nickname: String,
        description: String,
        scope: Option<Uuid>,
        owner: Option<Uuid>,
        draft: bool,
    },
    Lifecycle {
        object: String,
        id: Uuid,
        operation: String,
    },
    SetAudience {
        collaboration: Uuid,
        audience: Audience,
    },
    Assign(Box<Assignment>),
    RevokeGrant {
        grant: Uuid,
    },
    EndAppointment {
        appointment: Uuid,
    },
    CreateScope {
        parent: Uuid,
        label: String,
        scope_kind: String,
    },
    CreateRole {
        label: String,
        available_actions: Vec<String>,
    },
    SetGroups {
        scope: Uuid,
        conversations: Vec<Uuid>,
    },
    SaveDuty {
        id: Option<Uuid>,
        label: String,
        description: String,
        domain: String,
        available_actions: Vec<String>,
    },
    SaveRole {
        id: Option<Uuid>,
        label: String,
        description: String,
        duty_ids: Vec<Uuid>,
    },
    RetireCatalog {
        object: String,
        id: Uuid,
    },
    RestoreCatalog {
        object: String,
        id: Uuid,
    },
    UpdateScope {
        id: Uuid,
        label: String,
    },
    EndCollaboration {
        collaboration: Uuid,
    },
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Audience {
    #[serde(default)]
    pub open_reception: bool,
    pub groups: Vec<Uuid>,
    pub people: Vec<Uuid>,
    /// Within this connection's scope only; actual membership is resolved by PMS in C.
    pub residents: String,
    pub reply: PermissionMode,
    pub proactive: PermissionMode,
    pub reviewer: Option<Uuid>,
    pub topics: String,
    pub visibility: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Command {
    pub operation_id: Uuid,
    pub expected_version: i64,
    pub change: Change,
}

pub fn label(value: &str, max: usize) -> Result<()> {
    ensure!(
        !value.trim().is_empty()
            && value.chars().count() <= max
            && !value.chars().any(char::is_control),
        "invalid_label"
    );
    Ok(())
}

pub fn plain_text(value: &str, max: usize) -> Result<()> {
    ensure!(
        !value.trim().is_empty()
            && value.chars().count() <= max
            && !value
                .chars()
                .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t')),
        "invalid_text"
    );
    Ok(())
}

pub(super) fn valid_list(values: &[String], allowed: &[&str]) -> bool {
    !values.is_empty()
        && values.len() <= allowed.len()
        && values.iter().all(|s| allowed.contains(&s.as_str()))
        && values.iter().collect::<BTreeSet<_>>().len() == values.len()
}

impl Assignment {
    /// Compatibility for trusted local fixtures only. Public callers submit explicit settings.
    pub fn settings(&self) -> Vec<PermissionSetting> {
        if self.permissions.is_empty() {
            self.actions
                .iter()
                .map(|action| PermissionSetting {
                    action: action.clone(),
                    mode: PermissionMode::Autonomous,
                    reviewer: None,
                })
                .collect()
        } else {
            self.permissions.clone()
        }
    }

    pub fn has_autonomous(&self, action: &str) -> bool {
        self.settings()
            .iter()
            .any(|p| p.action == action && p.mode == PermissionMode::Autonomous)
    }

    pub fn validate(&self, now: DateTime<Utc>) -> Result<()> {
        plain_text(&self.responsibility, 2000)?;
        ensure!(
            agents().contains(&self.agent.as_str()) && DOMAINS.contains(&self.domain.as_str()),
            "unknown_agent_or_domain"
        );
        ensure!(
            self.permissions.is_empty() || self.actions.is_empty(),
            "mixed_permission_formats"
        );
        let settings = self.settings();
        if settings.is_empty() {
            ensure!(self.duty.is_some(), "invalid_actions");
        } else {
            ensure!(
                valid_list(
                    &settings
                        .iter()
                        .map(|p| p.action.clone())
                        .collect::<Vec<_>>(),
                    ACTIONS
                ),
                "invalid_actions"
            );
        }
        for setting in &settings {
            ensure!(
                (setting.mode == PermissionMode::Confirmation) == setting.reviewer.is_some(),
                "reviewer_mode_mismatch"
            );
            ensure!(
                setting.reviewer != Some(self.person),
                "self_confirmation_forbidden"
            );
        }
        ensure!(self.valid_until.is_none_or(|t| t > now), "expired_term");
        ensure!(
            self.valid_until
                .is_none_or(|t| t > self.valid_from.unwrap_or(now)),
            "invalid_term_range"
        );
        ensure!(
            self.proxy_for.is_none() || self.valid_until.is_some(),
            "proxy_requires_expiry"
        );
        ensure!(
            self.has_autonomous("manage") == self.delegation.is_some(),
            "delegation_envelope_required"
        );
        if let Some(d) = &self.delegation {
            ensure!(
                valid_list(&d.agents, &agents())
                    && valid_list(&d.domains, DOMAINS)
                    && valid_list(&d.actions, ACTIONS)
                    && (0..=7).contains(&d.depth),
                "invalid_delegation"
            );
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct Scope {
    pub id: Uuid,
    pub parent: Option<Uuid>,
    pub active: bool,
}

#[derive(Clone)]
pub struct Grant {
    pub id: Uuid,
    pub collaboration: Uuid,
    pub person: Uuid,
    pub scope: Uuid,
    pub agent: String,
    pub domain: String,
    pub duty: Option<Uuid>,
    pub action: String,
    pub mode: PermissionMode,
    pub reviewer: Option<Uuid>,
    pub parent: Option<Uuid>,
    pub descendants: bool,
    pub active: bool,
    pub delegation: Delegation,
}

pub struct Policy {
    pub scopes: Vec<Scope>,
    pub grants: Vec<Grant>,
}

impl Policy {
    pub fn in_scope(&self, child: Uuid, ancestor: Uuid, descendants: bool) -> bool {
        let mut next = Some(child);
        let mut seen = BTreeSet::new();
        while let Some(id) = next {
            if !seen.insert(id) {
                return false;
            }
            let Some(s) = self.scopes.iter().find(|s| s.id == id && s.active) else {
                return false;
            };
            if id == ancestor {
                return true;
            }
            if !descendants {
                return false;
            }
            next = s.parent;
        }
        false
    }

    pub fn effective(&self, grant: &Grant) -> bool {
        let mut g = grant;
        let mut seen = BTreeSet::new();
        loop {
            if !g.active
                || g.mode == PermissionMode::Denied
                || (g.mode == PermissionMode::Confirmation) != g.reviewer.is_some()
                || g.reviewer == Some(g.person)
                || !seen.insert(g.id)
                || !self.in_scope(g.scope, g.scope, false)
            {
                return false;
            }
            let Some(parent_id) = g.parent else {
                return g.action == "manage"
                    && g.mode == PermissionMode::Autonomous
                    && g.reviewer.is_none();
            };
            let Some(p) = self.grants.iter().find(|p| p.id == parent_id) else {
                return false;
            };
            if !self.covers(p, g.scope, &g.agent, &g.domain, &g.action) {
                return false;
            }
            if g.descendants && !p.descendants {
                return false;
            }
            if g.action == "manage"
                && (p.delegation.depth <= g.delegation.depth
                    || !subset(&g.delegation.agents, &p.delegation.agents)
                    || !subset(&g.delegation.domains, &p.delegation.domains)
                    || !subset(&g.delegation.actions, &p.delegation.actions))
            {
                return false;
            }
            g = p;
        }
    }

    fn covers(&self, g: &Grant, scope: Uuid, agent: &str, domain: &str, action: &str) -> bool {
        g.action == "manage"
            && g.mode == PermissionMode::Autonomous
            && g.reviewer.is_none()
            && self.in_scope(scope, g.scope, g.descendants)
            && g.delegation.agents.iter().any(|a| a == agent)
            && g.delegation.domains.iter().any(|a| a == domain)
            && g.delegation.actions.iter().any(|a| a == action)
    }

    pub fn manager(
        &self,
        person: Uuid,
        scope: Uuid,
        agent: &str,
        domain: &str,
        action: &str,
    ) -> Option<&Grant> {
        self.grants.iter().find(|g| {
            g.person == person && self.effective(g) && self.covers(g, scope, agent, domain, action)
        })
    }

    pub fn allowed(
        &self,
        person: Uuid,
        scope: Uuid,
        agent: &str,
        domain: &str,
        action: &str,
    ) -> bool {
        // A broad query does not identify the responsibility being exercised. It must not
        // select a convenient autonomous grant from conflicting duties or review settings.
        let matching: Vec<_> = self
            .grants
            .iter()
            .filter(|g| {
                g.person == person
                    && g.active
                    && g.agent == agent
                    && g.domain == domain
                    && g.action == action
                    && self.in_scope(scope, g.scope, g.descendants)
            })
            .collect();
        !matching.is_empty()
            && matching
                .iter()
                .filter_map(|g| g.duty)
                .collect::<BTreeSet<_>>()
                .len()
                == 1
            && matching.iter().all(|g| {
                g.duty.is_some() && self.decision(g.collaboration, action)["status"] == "autonomous"
            })
    }

    /// Explain current authorization for one complete person–duty–agent–scope connection.
    /// A confirmation result describes a required next step; it is never an approval token.
    pub fn decision(&self, collaboration: Uuid, action: &str) -> serde_json::Value {
        let denied =
            |reason| serde_json::json!({"status":"denied","reason":reason,"reviewer":null});
        let matching: Vec<_> = self
            .grants
            .iter()
            .filter(|g| g.collaboration == collaboration && g.action == action && g.active)
            .collect();
        let [grant] = matching.as_slice() else {
            return denied(if matching.is_empty() {
                "permission_not_granted"
            } else {
                "conflicting_permission_settings"
            });
        };
        if grant.duty.is_none() {
            return denied("duty_required");
        }
        if grant.mode == PermissionMode::Denied {
            return denied("permission_not_granted");
        }
        if grant.mode == PermissionMode::Confirmation
            && (grant.reviewer.is_none() || grant.reviewer == Some(grant.person))
        {
            return denied("eligible_reviewer_required");
        }
        if grant.mode == PermissionMode::Autonomous && grant.reviewer.is_some() {
            return denied("reviewer_mode_mismatch");
        }
        if !self.effective(grant) {
            return denied("authorization_chain_inactive");
        }
        if grant.mode == PermissionMode::Autonomous {
            return if grant.reviewer.is_none() {
                serde_json::json!({"status":"autonomous","reason":"within_granted_boundary","reviewer":null})
            } else {
                denied("reviewer_mode_mismatch")
            };
        }
        let Some(reviewer) = grant.reviewer.filter(|person| *person != grant.person) else {
            return denied("eligible_reviewer_required");
        };
        let eligible = self.grants.iter().any(|candidate| {
            candidate.person == reviewer
                && candidate.duty == grant.duty
                && candidate.scope == grant.scope
                && candidate.agent == grant.agent
                && candidate.domain == grant.domain
                && candidate.action == grant.action
                && candidate.mode == PermissionMode::Autonomous
                && candidate.reviewer.is_none()
                && self.effective(candidate)
                && !self.grants.iter().any(|other| {
                    other.active
                        && other.id != candidate.id
                        && other.collaboration == candidate.collaboration
                        && other.action == candidate.action
                })
        });
        if !eligible {
            return denied("reviewer_authority_inactive");
        }
        serde_json::json!({"status":"confirmation_required","reason":"designated_person_must_confirm","reviewer":reviewer})
    }

    pub fn can_inspect(&self, person: Uuid, scope: Uuid, agent: &str, domain: &str) -> bool {
        self.grants.iter().any(|g| {
            g.person == person
                && g.action == "manage"
                && g.mode == PermissionMode::Autonomous
                && self.effective(g)
                && self.in_scope(scope, g.scope, g.descendants)
                && g.delegation.agents.iter().any(|a| a == agent)
                && g.delegation.domains.iter().any(|d| d == domain)
        })
    }
}

pub fn subset(child: &[String], parent: &[String]) -> bool {
    child.iter().all(|v| parent.contains(v))
}
