//! Authentication is independent of business grants. All decisions share the tenant lock.
use super::*;
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, SaltString},
    Argon2, PasswordHasher, PasswordVerifier,
};
use zeroize::Zeroize;

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Credentials {
    pub username: String,
    pub password: String,
}
impl Drop for Credentials {
    fn drop(&mut self) {
        self.password.zeroize();
    }
}

#[derive(serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum AccountCommand {
    Create {
        person: Uuid,
        username: String,
        password: String,
    },
    Reset {
        account: Uuid,
        password: String,
    },
    Disable {
        account: Uuid,
    },
}
impl Drop for AccountCommand {
    fn drop(&mut self) {
        match self {
            Self::Create { password, .. } | Self::Reset { password, .. } => password.zeroize(),
            Self::Disable { .. } => {}
        }
    }
}

fn username(value: &str) -> Result<String> {
    let value = value.trim().to_ascii_lowercase();
    ensure!(
        (3..=64).contains(&value.len())
            && value.as_bytes()[0].is_ascii_alphanumeric()
            && value
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_.-".contains(&b)),
        "invalid_username"
    );
    Ok(value)
}
fn hash_password(password: &str) -> Result<String> {
    ensure!(
        (12..=128).contains(&password.chars().count()) && password.len() <= 512,
        "password_length"
    );
    Argon2::default()
        .hash_password(password.as_bytes(), &SaltString::generate(&mut OsRng))
        .map(|h| h.to_string())
        .map_err(|_| anyhow::anyhow!("password_hash_failed"))
}
fn matches_password(password: &str, hash: &str) -> bool {
    PasswordHash::new(hash).is_ok_and(|h| {
        Argon2::default()
            .verify_password(password.as_bytes(), &h)
            .is_ok()
    })
}

impl Store {
    fn account_admin(p: &Policy, actor: &Actor) -> bool {
        p.scopes.iter().filter(|s| s.parent.is_none()).any(|s| {
            p.manager(actor.person, s.id, "default", "organization", "identity")
                .is_some()
        })
    }

    async fn auth_audit(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        actor: Uuid,
        account: Uuid,
        action: &str,
    ) -> Result<()> {
        sqlx::query("INSERT INTO qintopia_agent_os.tool_invocation_audit(profile_id,tool_name,purpose,input_summary,output_summary,risk_level) VALUES('collaboration-local','collaboration.account','account_lifecycle',$1,$2,'high')")
            .bind(json!({"actor_ref":actor,"account_ref":account,"action":action}))
            .bind(json!({"tenant_hash":digest(self.tenant.as_bytes())})).execute(&mut **tx).await?;
        Ok(())
    }

    pub(super) async fn verify_session(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        hash: &str,
        person: Uuid,
    ) -> Result<()> {
        let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.collaboration_sessions s JOIN qintopia_agent_os.collaboration_accounts a ON a.id=s.account_id AND a.tenant_key=s.tenant_key JOIN qintopia_identity.source_identity_links l ON l.id=a.identity_link_id WHERE s.token_hash=$1 AND s.tenant_key=$2 AND a.person_id=$3 AND a.status='active' AND s.account_version=a.version AND s.identity_version=l.version AND l.person_id=a.person_id AND l.namespace=$4 AND s.revoked_at IS NULL AND s.expires_at>clock_timestamp())")
            .bind(hash).bind(&self.tenant).bind(person).bind(&self.identity_namespace).fetch_one(&mut **tx).await?;
        ensure!(valid, "authentication_required");
        Ok(())
    }

    // A durable rule request survives logout/expiry, not account reset or disable.
    pub(super) async fn verify_rule_request_account(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        hash: &str,
        person: Uuid,
    ) -> Result<()> {
        let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.collaboration_sessions s JOIN qintopia_agent_os.collaboration_accounts a ON a.id=s.account_id AND a.tenant_key=s.tenant_key JOIN qintopia_identity.source_identity_links l ON l.id=a.identity_link_id WHERE s.token_hash=$1 AND s.tenant_key=$2 AND a.person_id=$3 AND a.status='active' AND s.account_version=a.version AND s.identity_version=l.version AND l.person_id=a.person_id AND l.namespace=$4)")
            .bind(hash).bind(&self.tenant).bind(person).bind(&self.identity_namespace).fetch_one(&mut **tx).await?;
        ensure!(valid, "request_account_changed_or_disabled");
        Ok(())
    }

    pub(crate) async fn session_actor(&self, token: &str) -> Result<Actor> {
        ensure!(
            token.len() == 64 && token.bytes().all(|b| b.is_ascii_hexdigit()),
            "authentication_required"
        );
        let hash = digest(token.as_bytes());
        let (mut tx, _, _) = self.begin().await?;
        let row=sqlx::query("SELECT a.person_id,a.identity_link_id,s.identity_version FROM qintopia_agent_os.collaboration_sessions s JOIN qintopia_agent_os.collaboration_accounts a ON a.id=s.account_id AND a.tenant_key=s.tenant_key WHERE s.token_hash=$1 AND s.tenant_key=$2")
            .bind(&hash).bind(&self.tenant).fetch_optional(&mut *tx).await?.ok_or_else(||anyhow::anyhow!("authentication_required"))?;
        let actor = Actor {
            link: row.get("identity_link_id"),
            person: row.get("person_id"),
            work_account: None,
            identity_version: row.get("identity_version"),
            identity_namespace: self.identity_namespace.clone(),
            gateway: None,
            tenant: self.tenant.clone(),
            session_hash: Some(hash),
        };
        self.verify(&mut tx, &actor).await?;
        Ok(actor)
    }

    pub(crate) async fn login(&self, credentials: &Credentials) -> Result<String> {
        ensure!(credentials.password.len() <= 512, "invalid_credentials");
        let name =
            username(&credentials.username).map_err(|_| anyhow::anyhow!("invalid_credentials"))?;
        let (mut tx, _, now) = self.begin().await?;
        // Commit attempts even when credentials are wrong; bound username buckets to this window.
        sqlx::query("DELETE FROM qintopia_agent_os.collaboration_login_limits WHERE tenant_key=$1 AND window_start<$2-interval '15 minutes'")
            .bind(&self.tenant).bind(now).execute(&mut *tx).await?;
        for (bucket, limit) in [
            ("global".to_string(), 60),
            (format!("user:{}", digest(name.as_bytes())), 5),
        ] {
            let n:i32=sqlx::query_scalar("INSERT INTO qintopia_agent_os.collaboration_login_limits(tenant_key,bucket,window_start,attempts) VALUES($1,$2,$3,1) ON CONFLICT(tenant_key,bucket) DO UPDATE SET attempts=collaboration_login_limits.attempts+1 RETURNING attempts")
                .bind(&self.tenant).bind(bucket).bind(now).fetch_one(&mut *tx).await?;
            if n > limit {
                tx.commit().await?;
                bail!("login_rate_limited");
            }
        }
        let row=sqlx::query("SELECT a.*,l.version AS identity_version FROM qintopia_agent_os.collaboration_accounts a JOIN qintopia_identity.source_identity_links l ON l.id=a.identity_link_id JOIN qintopia_identity.persons p ON p.id=a.person_id WHERE a.tenant_key=$1 AND a.username=$2 AND a.status='active' AND l.namespace=$3 AND l.person_id=a.person_id AND l.status='confirmed' AND l.evidence_ref IS NOT NULL AND l.confirmed_by IS NOT NULL AND p.status='active' AND NOT EXISTS(SELECT 1 FROM qintopia_agent_os.collaboration_ledger x WHERE x.tenant_key=$1 AND x.kind='person' AND x.object_ref=p.id::text AND x.status<>'active')")
            .bind(&self.tenant).bind(&name).bind(&self.identity_namespace).fetch_optional(&mut *tx).await?;
        let valid = if let Some(r) = &row {
            matches_password(&credentials.password, &r.get::<String, _>("password_hash"))
        } else {
            let _ = hash_password("dummy-password-work-factor");
            false
        };
        if !valid {
            tx.commit().await?;
            bail!("invalid_credentials");
        }
        let row = row.unwrap();
        let token = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
        sqlx::query("INSERT INTO qintopia_agent_os.collaboration_sessions(token_hash,tenant_key,account_id,account_version,identity_version,expires_at) VALUES($1,$2,$3,$4,$5,clock_timestamp()+interval '8 hours')")
            .bind(digest(token.as_bytes())).bind(&self.tenant).bind(row.get::<Uuid,_>("id"))
            .bind(row.get::<i64,_>("version")).bind(row.get::<i64,_>("identity_version")).execute(&mut *tx).await?;
        self.auth_audit(&mut tx, row.get("person_id"), row.get("id"), "login")
            .await?;
        tx.commit().await?;
        Ok(token)
    }

    pub(crate) async fn me(&self, actor: &Actor) -> Result<Value> {
        let (mut tx, _, now) = self.begin().await?;
        self.verify(&mut tx, actor).await?;
        let p = self.policy(&mut tx, now).await?;
        let name:String=sqlx::query_scalar("SELECT coalesce(preferred_name,display_name) FROM qintopia_identity.persons WHERE id=$1")
            .bind(actor.person).fetch_one(&mut *tx).await?;
        Ok(
            json!({"person":actor.person,"label":name,"account_admin":Self::account_admin(&p,actor)}),
        )
    }

    pub(crate) async fn logout(&self, actor: &Actor) -> Result<()> {
        let (mut tx, _, _) = self.begin().await?;
        self.verify(&mut tx, actor).await?;
        sqlx::query("UPDATE qintopia_agent_os.collaboration_sessions SET revoked_at=clock_timestamp() WHERE tenant_key=$1 AND token_hash=$2")
            .bind(&self.tenant).bind(&actor.session_hash).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }

    pub(crate) async fn change_password(&self, actor: &Actor, old: &str, new: &str) -> Result<()> {
        ensure!(old.len() <= 512, "invalid_credentials");
        let (mut tx, _, _) = self.begin().await?;
        self.verify(&mut tx, actor).await?;
        let row=sqlx::query("SELECT id,password_hash FROM qintopia_agent_os.collaboration_accounts WHERE tenant_key=$1 AND person_id=$2 AND status='active'")
            .bind(&self.tenant).bind(actor.person).fetch_one(&mut *tx).await?;
        // Same persistent bucket mechanism bounds wrong current-password attempts.
        let n:i32=sqlx::query_scalar("INSERT INTO qintopia_agent_os.collaboration_login_limits(tenant_key,bucket,window_start,attempts) VALUES($1,$2,clock_timestamp(),1) ON CONFLICT(tenant_key,bucket) DO UPDATE SET attempts=CASE WHEN collaboration_login_limits.window_start<clock_timestamp()-interval '15 minutes' THEN 1 ELSE collaboration_login_limits.attempts+1 END,window_start=CASE WHEN collaboration_login_limits.window_start<clock_timestamp()-interval '15 minutes' THEN clock_timestamp() ELSE collaboration_login_limits.window_start END RETURNING attempts")
            .bind(&self.tenant).bind(format!("change:{}",actor.person)).fetch_one(&mut *tx).await?;
        if n > 5 {
            tx.commit().await?;
            bail!("login_rate_limited");
        }
        if !matches_password(old, &row.get::<String, _>("password_hash")) {
            tx.commit().await?;
            bail!("invalid_credentials");
        }
        let hash = hash_password(new)?;
        let id: Uuid = row.get("id");
        self.replace_password(&mut tx, id, &hash).await?;
        self.auth_audit(&mut tx, actor.person, id, "change_password")
            .await?;
        tx.commit().await?;
        Ok(())
    }

    async fn replace_password(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        id: Uuid,
        hash: &str,
    ) -> Result<()> {
        let n=sqlx::query("UPDATE qintopia_agent_os.collaboration_accounts SET password_hash=$3,version=version+1 WHERE tenant_key=$1 AND id=$2 AND status='active'")
            .bind(&self.tenant).bind(id).bind(hash).execute(&mut **tx).await?.rows_affected();
        ensure!(n == 1, "account_not_active");
        self.revoke_sessions(tx, id).await
    }
    async fn revoke_sessions(&self, tx: &mut Transaction<'_, Postgres>, id: Uuid) -> Result<()> {
        sqlx::query("UPDATE qintopia_agent_os.collaboration_sessions SET revoked_at=clock_timestamp() WHERE tenant_key=$1 AND account_id=$2 AND revoked_at IS NULL")
            .bind(&self.tenant).bind(id).execute(&mut **tx).await?;
        Ok(())
    }

    pub(crate) async fn accounts(&self, actor: &Actor) -> Result<Value> {
        let (mut tx, _, now) = self.begin().await?;
        self.verify(&mut tx, actor).await?;
        ensure!(
            Self::account_admin(&self.policy(&mut tx, now).await?, actor),
            "account_management_denied"
        );
        // Availability mirrors login's current identity and ledger predicates.
        // It is a read-only explanation, never a login ticket or account mutation.
        let accounts:Value=sqlx::query_scalar("WITH current_accounts AS (SELECT a.id,a.person_id AS person,a.username,a.status,coalesce(p.preferred_name,p.display_name) AS label,CASE WHEN a.status<>'active' THEN 'disabled' WHEN l.id IS NULL OR l.namespace<>$2 OR l.person_id IS DISTINCT FROM a.person_id OR l.status<>'confirmed' OR l.evidence_ref IS NULL OR l.confirmed_by IS NULL THEN 'identity_invalid' WHEN p.status<>'active' THEN 'person_inactive' WHEN EXISTS(SELECT 1 FROM qintopia_agent_os.collaboration_ledger x WHERE x.tenant_key=a.tenant_key AND x.kind='person' AND x.object_ref=p.id::text AND x.status<>'active') THEN 'person_unavailable' ELSE 'ready' END AS login_state FROM qintopia_agent_os.collaboration_accounts a JOIN qintopia_identity.persons p ON p.id=a.person_id LEFT JOIN qintopia_identity.source_identity_links l ON l.id=a.identity_link_id WHERE a.tenant_key=$1) SELECT coalesce(jsonb_agg(to_jsonb(a)||jsonb_build_object('login_available',a.login_state='ready') ORDER BY a.username),'[]') FROM current_accounts a")
            .bind(&self.tenant).bind(&self.identity_namespace).fetch_one(&mut *tx).await?;
        let people:Value=sqlx::query_scalar("SELECT coalesce(jsonb_agg(x ORDER BY x->>'label'),'[]') FROM (SELECT DISTINCT jsonb_build_object('id',p.id,'label',coalesce(p.preferred_name,p.display_name)) x FROM qintopia_identity.persons p JOIN qintopia_identity.source_identity_links l ON l.person_id=p.id WHERE l.namespace=$2 AND l.status='confirmed' AND l.evidence_ref IS NOT NULL AND l.confirmed_by IS NOT NULL AND p.status='active' AND NOT EXISTS(SELECT 1 FROM qintopia_agent_os.collaboration_accounts a WHERE a.tenant_key=$1 AND a.person_id=p.id) AND NOT EXISTS(SELECT 1 FROM qintopia_agent_os.collaboration_ledger x WHERE x.tenant_key=$1 AND x.kind='person' AND x.object_ref=p.id::text AND x.status<>'active')) s")
            .bind(&self.tenant).bind(&self.identity_namespace).fetch_one(&mut *tx).await?;
        Ok(json!({"accounts":accounts,"people":people}))
    }

    async fn create_account(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        person: Uuid,
        name: &str,
        password: &str,
    ) -> Result<Uuid> {
        self.known_person(tx, person).await?;
        let link:Uuid=sqlx::query_scalar("SELECT id FROM qintopia_identity.source_identity_links WHERE namespace=$1 AND person_id=$2 AND status='confirmed' AND evidence_ref IS NOT NULL AND confirmed_by IS NOT NULL ORDER BY id LIMIT 1")
            .bind(&self.identity_namespace).bind(person).fetch_one(&mut **tx).await?;
        let id=sqlx::query_scalar("INSERT INTO qintopia_agent_os.collaboration_accounts(tenant_key,person_id,identity_link_id,username,password_hash) VALUES($1,$2,$3,$4,$5) RETURNING id")
            .bind(&self.tenant).bind(person).bind(link).bind(username(name)?).bind(hash_password(password)?)
            .fetch_one(&mut **tx).await.map_err(|_|anyhow::anyhow!("account_conflict"))?;
        Ok(id)
    }

    pub(crate) async fn account_command(
        &self,
        actor: &Actor,
        command: &AccountCommand,
    ) -> Result<Value> {
        let (mut tx, _, now) = self.begin().await?;
        self.verify(&mut tx, actor).await?;
        ensure!(
            Self::account_admin(&self.policy(&mut tx, now).await?, actor),
            "account_management_denied"
        );
        let (id, action) = match command {
            AccountCommand::Create {
                person,
                username,
                password,
            } => (
                self.create_account(&mut tx, *person, username, password)
                    .await?,
                "create",
            ),
            AccountCommand::Reset { account, password } => {
                self.replace_password(&mut tx, *account, &hash_password(password)?)
                    .await?;
                (*account, "reset")
            }
            AccountCommand::Disable { account } => {
                let n=sqlx::query("UPDATE qintopia_agent_os.collaboration_accounts SET status='disabled',version=version+1 WHERE tenant_key=$1 AND id=$2 AND status='active'")
                    .bind(&self.tenant).bind(account).execute(&mut *tx).await?.rows_affected();
                ensure!(n == 1, "account_not_active");
                self.revoke_sessions(&mut tx, *account).await?;
                (*account, "disable")
            }
        };
        self.auth_audit(&mut tx, actor.person, id, action).await?;
        tx.commit().await?;
        Ok(json!({"id":id,"action":action,"business_permissions_added":false}))
    }

    /// Process-owner CLI only: consumes existing authority, never creates grants.
    pub(crate) async fn bootstrap_account(
        &self,
        person: Uuid,
        name: &str,
        password: &str,
    ) -> Result<()> {
        let (mut tx, _, now) = self.begin().await?;
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM qintopia_agent_os.collaboration_accounts WHERE tenant_key=$1",
        )
        .bind(&self.tenant)
        .fetch_one(&mut *tx)
        .await?;
        ensure!(count == 0, "accounts_already_initialized");
        let row=sqlx::query("SELECT id,version FROM qintopia_identity.source_identity_links WHERE namespace=$1 AND person_id=$2 AND status='confirmed' AND evidence_ref IS NOT NULL AND confirmed_by IS NOT NULL ORDER BY id LIMIT 1")
            .bind(&self.identity_namespace).bind(person).fetch_one(&mut *tx).await?;
        let actor = Actor {
            link: row.get("id"),
            person,
            work_account: None,
            identity_version: row.get("version"),
            identity_namespace: self.identity_namespace.clone(),
            gateway: None,
            tenant: self.tenant.clone(),
            session_hash: None,
        };
        self.verify(&mut tx, &actor).await?;
        ensure!(
            Self::account_admin(&self.policy(&mut tx, now).await?, &actor),
            "account_management_denied"
        );
        let id = self.create_account(&mut tx, person, name, password).await?;
        self.auth_audit(&mut tx, person, id, "bootstrap").await?;
        tx.commit().await?;
        Ok(())
    }
}
