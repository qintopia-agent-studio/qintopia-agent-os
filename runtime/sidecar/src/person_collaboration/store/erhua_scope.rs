//! Route Erhua turns using current trusted bindings, retaining the identity gateway realm.
use super::{Actor, Store};
use anyhow::{ensure, Result};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub(super) enum TurnScope {
    Direct(Uuid),
    Group {
        scope: Uuid,
        binding: Uuid,
        version: i64,
        conversation: Uuid,
    },
}
impl TurnScope {
    pub(super) fn scope(&self) -> Uuid {
        match self {
            Self::Direct(scope) | Self::Group { scope, .. } => *scope,
        }
    }
}
impl Store {
    pub(crate) async fn preflight_erhua_gateway(&self, gateway: &str) -> Result<()> {
        ensure!(self.is_live(), "live_gateway_required");
        let (mut tx, _, _) = self.begin().await?;
        let valid: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_identity.person_identity_gateways g JOIN qintopia_agent_os.collaboration_scopes s ON s.id=g.scope_id AND s.tenant_key=g.tenant_key WHERE g.tenant_key=$1 AND g.gateway_key=$2 AND g.active AND g.account_kind<>'shared' AND g.subject_type IN ('qiwe_sender','wecom_external','wecom_internal') AND s.status='active' AND NOT EXISTS(SELECT 1 FROM qintopia_identity.person_identity_gateways x WHERE x.tenant_key<>g.tenant_key AND x.namespace=g.namespace AND x.subject_type=g.subject_type AND x.active) AND NOT EXISTS(SELECT 1 FROM qintopia_agent_os.collaboration_tenants t WHERE t.tenant_key<>g.tenant_key AND t.identity_namespace=g.namespace))")
            .bind(&self.tenant).bind(gateway).fetch_one(&mut *tx).await?;
        ensure!(valid, "live_gateway_unavailable");
        Ok(())
    }

    async fn direct_erhua_scope(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        actor: &Actor,
    ) -> Result<Uuid> {
        let now = sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&mut **tx)
            .await?;
        let policy = self.policy(tx, now).await?;
        let realm = actor
            .gateway
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("trusted_gateway_required"))?
            .2;
        let scopes: std::collections::BTreeSet<_> = policy
            .grants
            .iter()
            .filter(|g| {
                g.person == actor.person
                    && g.agent == "erhua"
                    && g.domain == "community_service"
                    && policy.effective(g)
                    && policy.in_scope(g.scope, realm, true)
            })
            .map(|g| g.scope)
            .collect();
        ensure!(scopes.len() <= 1, "private_scope_ambiguous");
        // A resident with no work assignment may still use their personal reply context.
        Ok(scopes.into_iter().next().unwrap_or(realm))
    }

    pub(crate) async fn route_erhua_actor(
        &self,
        mut actor: Actor,
        chat_type: &str,
        chat: &str,
    ) -> Result<Actor> {
        let (mut tx, _, now) = self.begin().await?;
        self.verify(&mut tx, &actor).await?;
        ensure!(actor.work_account.is_none(), "agent_tool_denied");
        let realm = actor
            .gateway
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("trusted_gateway_required"))?
            .2;
        actor.foundation_turn_scope = Some(if chat_type == "group" {
            let rows = sqlx::query("SELECT b.id,b.version,b.scope_id,b.conversation_id FROM qintopia_agent_os.collaboration_scope_bindings b JOIN qintopia_messages.conversations c ON c.id=b.conversation_id AND c.tenant_id=b.tenant_key WHERE b.tenant_key=$1 AND b.revoked_at IS NULL AND c.platform='qiwe' AND c.chat_type='group' AND c.chat_id=$2 AND c.status='active'")
                .bind(&self.tenant).bind(chat).fetch_all(&mut *tx).await?;
            ensure!(rows.len() == 1, "gateway_scope_mismatch");
            let row = &rows[0];
            let scope = row.get("scope_id");
            let policy = self.policy(&mut tx, now).await?;
            ensure!(
                policy.in_scope(scope, realm, true),
                "gateway_scope_mismatch"
            );
            TurnScope::Group {
                scope,
                binding: row.get("id"),
                version: row.get("version"),
                conversation: row.get("conversation_id"),
            }
        } else {
            ensure!(chat_type == "direct", "trusted_context_unavailable");
            TurnScope::Direct(self.direct_erhua_scope(&mut tx, &actor).await?)
        });
        tx.commit().await?;
        Ok(actor)
    }

    pub(super) async fn verify_foundation_turn_scope(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        actor: &Actor,
    ) -> Result<()> {
        match &actor.foundation_turn_scope {
            None => {}
            Some(TurnScope::Direct(scope)) => ensure!(
                *scope == self.direct_erhua_scope(tx, actor).await?,
                "gateway_scope_mismatch"
            ),
            Some(TurnScope::Group {
                scope,
                binding,
                version,
                conversation,
            }) => {
                let valid: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.collaboration_scope_bindings b JOIN qintopia_messages.conversations c ON c.id=b.conversation_id AND c.tenant_id=b.tenant_key JOIN qintopia_agent_os.collaboration_scopes s ON s.id=b.scope_id AND s.tenant_key=b.tenant_key WHERE b.tenant_key=$1 AND b.id=$2 AND b.version=$3 AND b.scope_id=$4 AND b.conversation_id=$5 AND b.revoked_at IS NULL AND c.platform='qiwe' AND c.chat_type='group' AND c.status='active' AND s.status='active')")
                    .bind(&self.tenant).bind(binding).bind(version).bind(scope).bind(conversation).fetch_one(&mut **tx).await?;
                ensure!(valid, "gateway_scope_mismatch");
                let realm = actor
                    .gateway
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("trusted_gateway_required"))?
                    .2;
                let now = sqlx::query_scalar("SELECT clock_timestamp()")
                    .fetch_one(&mut **tx)
                    .await?;
                ensure!(
                    self.policy(tx, now).await?.in_scope(*scope, realm, true),
                    "gateway_scope_mismatch"
                );
            }
        }
        Ok(())
    }
}
