//! One-time, exact-target invitation issuance and acceptance.
use super::*;
use serde::Deserialize;
use sqlx::postgres::PgRow;
use subtle::ConstantTimeEq;

#[derive(Deserialize)]
#[serde(deny_unknown_fields, tag = "kind", rename_all = "snake_case")]
enum Target {
    Email { email: String },
    User { user_id: Uuid },
    ExternalIdentity { issuer: String, subject: String },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Resource {
    scope_kind: String,
    workspace_id: Option<Uuid>,
    project_id: Option<Uuid>,
    role: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InvitationInput {
    target: Target,
    organization_role: String,
    resource_grant: Option<Resource>,
    expires_at: DateTime<Utc>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InvitationProof {
    organization_id: Uuid,
    invitation_id: Uuid,
    secret: String,
}

pub enum InvitationResult {
    Accepted(Value),
    VerifiedExpired,
}

fn payload_error() -> AppError {
    AppError::validation("INVALID_REQUEST", "invalid request payload")
}
fn invalid() -> AppError {
    AppError::bad_request("invalid invitation")
}

impl InvitationInput {
    pub fn parse(bytes: &[u8]) -> Result<Self, AppError> {
        let value: Self = serde_json::from_slice(bytes).map_err(|_| payload_error())?;
        if !matches!(
            value.organization_role.as_str(),
            "owner" | "admin" | "member" | "guest"
        ) {
            return Err(payload_error());
        }
        match &value.target {
            Target::Email { email } => {
                let mut parts = email.split('@');
                if !(3..=254).contains(&email.chars().count())
                    || email.chars().any(char::is_whitespace)
                    || parts.next().is_none_or(str::is_empty)
                    || parts.next().is_none_or(str::is_empty)
                    || parts.next().is_some()
                {
                    return Err(payload_error());
                }
            }
            Target::ExternalIdentity { issuer, subject } => {
                if !(1..=2048).contains(&issuer.chars().count())
                    || !(1..=1024).contains(&subject.chars().count())
                {
                    return Err(payload_error());
                }
            }
            Target::User { .. } => {}
        }
        if let Some(resource) = &value.resource_grant {
            if !matches!(
                resource.scope_kind.as_str(),
                "organization" | "workspace" | "project"
            ) || resource_role_permissions(&resource.role).is_empty()
            {
                return Err(payload_error());
            }
        }
        Ok(value)
    }
}

impl InvitationProof {
    pub fn parse(bytes: &[u8]) -> Result<Self, AppError> {
        let value: Self = serde_json::from_slice(bytes).map_err(|_| payload_error())?;
        if !(32..=512).contains(&value.secret.chars().count()) {
            return Err(payload_error());
        }
        Ok(value)
    }
}

impl TenantTx {
    pub async fn issue_invitation(
        &mut self,
        org: Uuid,
        input: InvitationInput,
    ) -> Result<Value, AppError> {
        let (_, caller) = self.member(org, true, false).await?;
        self.require(org, None, None, INVITATION_MANAGE).await?;
        if input.organization_role == "owner" && caller.role != "owner" {
            return Err(AppError::forbidden(
                "Owner authority is required to invite an Owner",
            ));
        }
        if input.expires_at <= Utc::now() {
            return Err(AppError::bad_request(
                "invitation expiry must be in the future",
            ));
        }
        let (kind, email, user, issuer, subject) = match &input.target {
            Target::Email { email } => {
                ("email", Some(email.trim().to_lowercase()), None, None, None)
            }
            Target::User { user_id } => ("user", None, Some(*user_id), None, None),
            Target::ExternalIdentity { issuer, subject } => (
                "external_identity",
                None,
                None,
                Some(issuer.as_str()),
                Some(subject.as_str()),
            ),
        };
        let (scope, workspace, project, role) = if let Some(resource) = &input.resource_grant {
            match (
                resource.scope_kind.as_str(),
                resource.workspace_id,
                resource.project_id,
            ) {
                ("organization", None, None) => {}
                ("organization", _, _) => {
                    return Err(AppError::bad_request(
                        "organization grant cannot include resource id",
                    ))
                }
                ("workspace", Some(id), None) => {
                    if self.workspace(org, id).await?.is_none() {
                        return Err(AppError::bad_request(
                            "workspace does not belong to organization",
                        ));
                    }
                }
                ("workspace", _, _) => {
                    return Err(AppError::bad_request(
                        "workspace grant requires workspace_id",
                    ))
                }
                ("project", None, Some(id)) => {
                    if self.project(org, id).await?.is_none() {
                        return Err(AppError::bad_request(
                            "project does not belong to organization",
                        ));
                    }
                }
                _ => return Err(AppError::bad_request("project grant requires project_id")),
            }
            (
                Some(resource.scope_kind.as_str()),
                resource.workspace_id,
                resource.project_id,
                Some(resource.role.as_str()),
            )
        } else {
            (None, None, None, None)
        };
        let mut entropy = [0u8; 32];
        getrandom::getrandom(&mut entropy)
            .map_err(|_| AppError::internal("OS invitation randomness unavailable"))?;
        let secret = format!("inv_{}", URL_SAFE_NO_PAD.encode(entropy));
        let hash = Sha256::digest(secret.as_bytes()).to_vec();
        let id = Uuid::new_v4();
        sqlx::query("INSERT INTO org.invitations(id,organization_id,status,target_kind,target_email_normalized,target_user_id,target_issuer,target_subject,organization_role,resource_scope_kind,resource_workspace_id,resource_project_id,resource_role,token_hash,token_prefix,expires_at,created_at,created_by_actor_type,created_by_actor_id)
                     VALUES($1,$2,'pending',$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,now(),'human',$16)")
            .bind(id).bind(org).bind(kind).bind(email).bind(user).bind(issuer).bind(subject)
            .bind(&input.organization_role).bind(scope).bind(workspace).bind(project).bind(role)
            .bind(hash).bind(&secret[..12]).bind(input.expires_at).bind(self.actor_id)
            .execute(&mut *self.tx).await.map_err(db_error)?;
        self.audit(
            org,
            "invitation.create",
            "invitation",
            Some(id),
            "success",
            json!({"target_kind":kind,"organization_role":input.organization_role}),
        )
        .await?;
        Ok(json!({"id":id,"organization_id":org,"status":"pending","secret":secret}))
    }

    pub async fn list_invitations(&mut self, org: Uuid) -> Result<Vec<Value>, AppError> {
        self.member(org, false, false).await?;
        self.require(org, None, None, INVITATION_MANAGE).await?;
        let rows=sqlx::query("SELECT id,target_email_normalized AS email,organization_role,resource_project_id AS project_id,resource_role,
            CASE WHEN status='pending' AND expires_at<=now() THEN 'expired' ELSE status END AS status,expires_at,created_at
            FROM org.invitations WHERE organization_id=$1 AND target_kind='email' ORDER BY created_at DESC LIMIT 100")
            .bind(org).fetch_all(&mut *self.tx).await.map_err(db_error)?;
        rows.iter().map(|row|->Result<Value,sqlx::Error>{
            let expiry:DateTime<Utc>=row.try_get("expires_at")?;
            let created:DateTime<Utc>=row.try_get("created_at")?;
            Ok(json!({"id":row.try_get::<Uuid,_>("id")?,"email":row.try_get::<String,_>("email")?,
              "organization_role":row.try_get::<String,_>("organization_role")?,"project_id":row.try_get::<Option<Uuid>,_>("project_id")?,
              "resource_role":row.try_get::<Option<String>,_>("resource_role")?,"status":row.try_get::<String,_>("status")?,
              "expires_at":python_timestamp(expiry),"created_at":python_timestamp(created)}))
        }).collect::<Result<_,_>>().map_err(db_error)
    }

    pub async fn revoke_invitation(&mut self, org: Uuid, id: Uuid) -> Result<(), AppError> {
        let (_, caller) = self.member(org, true, false).await?;
        let (role,status):(String,String)=sqlx::query_as("SELECT organization_role,status FROM org.invitations WHERE organization_id=$1 AND id=$2 FOR UPDATE")
            .bind(org).bind(id).fetch_optional(&mut *self.tx).await.map_err(db_error)?.ok_or_else(||AppError::not_found("invitation not found"))?;
        self.require(org, None, None, INVITATION_MANAGE).await?;
        if role == "owner" && caller.role != "owner" {
            return Err(AppError::forbidden(
                "Owner required to revoke an Owner invitation",
            ));
        }
        if status != "pending" {
            return Err(AppError::bad_request(
                "only pending invitation can be revoked",
            ));
        }
        sqlx::query("UPDATE org.invitations SET status='revoked',revoked_at=now() WHERE organization_id=$1 AND id=$2")
            .bind(org).bind(id).execute(&mut *self.tx).await.map_err(db_error)?;
        self.audit(
            org,
            "invitation.revoke",
            "invitation",
            Some(id),
            "success",
            json!({}),
        )
        .await
    }

    // The pre-membership exception is private to these exact-ID operations.
    async fn invitation_row(
        &mut self,
        proof: &InvitationProof,
        lock: bool,
    ) -> Result<Option<PgRow>, AppError> {
        self.bind(Some(proof.organization_id)).await?;
        let suffix = if lock { " FOR UPDATE" } else { "" };
        sqlx::query(&format!("SELECT status,target_kind,target_user_id,target_email_normalized,target_issuer,target_subject,organization_role,
            resource_scope_kind,resource_workspace_id,resource_project_id,resource_role,token_hash,expires_at,created_by_actor_type,created_by_actor_id
            FROM org.invitations WHERE organization_id=$1 AND id=$2{suffix}"))
            .bind(proof.organization_id).bind(proof.invitation_id).fetch_optional(&mut *self.tx).await.map_err(db_error)
    }

    async fn target_matches(&mut self, row: &PgRow, session_id: Uuid) -> Result<bool, AppError> {
        let kind: String = row.try_get("target_kind").map_err(db_error)?;
        let link:Option<(String,String,Option<String>,bool)>=sqlx::query_as("SELECT l.issuer,l.subject,l.email_snapshot,l.email_verified FROM identity.web_sessions s JOIN identity.identity_links l ON l.id=s.identity_link_id AND l.user_id=s.user_id JOIN identity.users u ON u.id=s.user_id
            WHERE s.id=$1 AND s.user_id=$2 AND s.revoked_at IS NULL AND s.expires_at>clock_timestamp() AND u.status='active'")
            .bind(session_id).bind(self.actor_id).fetch_optional(&mut *self.tx).await.map_err(db_error)?;
        let Some((issuer, subject, email, verified)) = link else {
            return Ok(false);
        };
        Ok(match kind.as_str() {
            "user" => {
                row.try_get::<Option<Uuid>, _>("target_user_id")
                    .map_err(db_error)?
                    == Some(self.actor_id)
            }
            "email" => {
                verified
                    && email.map(|email| email.trim().to_lowercase())
                        == row
                            .try_get::<Option<String>, _>("target_email_normalized")
                            .map_err(db_error)?
            }
            "external_identity" => {
                Some(issuer)
                    == row
                        .try_get::<Option<String>, _>("target_issuer")
                        .map_err(db_error)?
                    && Some(subject)
                        == row
                            .try_get::<Option<String>, _>("target_subject")
                            .map_err(db_error)?
            }
            _ => false,
        })
    }

    pub async fn check_invitation(
        &mut self,
        proof: &InvitationProof,
        session_id: Uuid,
    ) -> Result<&'static str, AppError> {
        let Some(row) = self.invitation_row(proof, false).await? else {
            return Ok("invalid");
        };
        if row.try_get::<String, _>("status").map_err(db_error)? != "pending"
            || row
                .try_get::<DateTime<Utc>, _>("expires_at")
                .map_err(db_error)?
                <= Utc::now()
            || !valid_secret(&row, proof)?
        {
            return Ok("invalid");
        }
        Ok(if self.target_matches(&row, session_id).await? {
            "eligible"
        } else {
            "mismatch"
        })
    }

    pub async fn accept_invitation(
        &mut self,
        proof: &InvitationProof,
        session_id: Uuid,
    ) -> Result<InvitationResult, AppError> {
        let org = proof.organization_id;
        self.bind(Some(org)).await?;
        if self
            .organization(org, true)
            .await?
            .is_none_or(|o| o.status != "active")
        {
            return Err(invalid());
        }
        let row = self
            .invitation_row(proof, true)
            .await?
            .ok_or_else(invalid)?;
        if row.try_get::<String, _>("status").map_err(db_error)? != "pending"
            || !valid_secret(&row, proof)?
            || !self.target_matches(&row, session_id).await?
        {
            return Err(invalid());
        }
        let now = Utc::now();
        if row
            .try_get::<DateTime<Utc>, _>("expires_at")
            .map_err(db_error)?
            <= now
        {
            sqlx::query(
                "UPDATE org.invitations SET status='expired' WHERE organization_id=$1 AND id=$2",
            )
            .bind(org)
            .bind(proof.invitation_id)
            .execute(&mut *self.tx)
            .await
            .map_err(db_error)?;
            return Ok(InvitationResult::VerifiedExpired);
        }
        let role: String = row.try_get("organization_role").map_err(db_error)?;
        if role == "owner" {
            let inviter: Option<Uuid> = row.try_get("created_by_actor_id").map_err(db_error)?;
            let inviter_kind: String = row.try_get("created_by_actor_type").map_err(db_error)?;
            let valid:Option<Uuid>=sqlx::query_scalar("SELECT id FROM org.organization_memberships WHERE organization_id=$1 AND user_id=$2 AND role='owner' AND status='active' FOR UPDATE")
                .bind(org).bind(inviter).fetch_optional(&mut *self.tx).await.map_err(db_error)?;
            if inviter_kind != "human" || valid.is_none() {
                return Err(invalid());
            }
        }
        let existing = self.own_membership_for_update(org).await?;
        if existing
            .as_ref()
            .is_some_and(|member| member.role == "owner" && role != "owner")
        {
            return Err(invalid());
        }
        let mid = existing
            .as_ref()
            .map(|member| member.id)
            .unwrap_or_else(Uuid::new_v4);
        if existing.is_some() {
            sqlx::query("UPDATE org.organization_memberships SET role=$1,status='active',activated_at=$2,suspended_at=NULL,revoked_at=NULL WHERE organization_id=$3 AND id=$4 AND user_id=$5")
                .bind(&role).bind(now).bind(org).bind(mid).bind(self.actor_id).execute(&mut *self.tx).await.map_err(db_error)?;
        } else {
            sqlx::query("INSERT INTO org.organization_memberships(id,organization_id,user_id,role,status,created_at,activated_at) VALUES($1,$2,$3,$4,'active',$5,$5)")
                .bind(mid).bind(org).bind(self.actor_id).bind(&role).bind(now).execute(&mut *self.tx).await.map_err(db_error)?;
        }
        let mut grant = None;
        let mut metadata = json!({"membership_id":mid.to_string()});
        let scope: Option<String> = row.try_get("resource_scope_kind").map_err(db_error)?;
        if let Some(scope) = scope {
            let workspace: Option<Uuid> = row.try_get("resource_workspace_id").map_err(db_error)?;
            let project: Option<Uuid> = row.try_get("resource_project_id").map_err(db_error)?;
            let resource_role: String = row.try_get("resource_role").map_err(db_error)?;
            let existing:Option<(Uuid,DateTime<Utc>)>=sqlx::query_as("SELECT id,created_at FROM org.resource_grants WHERE organization_id=$1 AND subject_kind='membership' AND membership_id=$2 AND scope_kind=$3 AND workspace_id IS NOT DISTINCT FROM $4 AND project_id IS NOT DISTINCT FROM $5 AND role=$6 AND revoked_at IS NULL")
                .bind(org).bind(mid).bind(&scope).bind(workspace).bind(project).bind(&resource_role).fetch_optional(&mut *self.tx).await.map_err(db_error)?;
            let (gid, created) = if let Some(existing) = existing {
                existing
            } else {
                let gid = Uuid::new_v4();
                sqlx::query("INSERT INTO org.resource_grants(id,organization_id,subject_kind,membership_id,scope_kind,workspace_id,project_id,role,created_at,created_by_actor_type,created_by_actor_id)
                             VALUES($1,$2,'membership',$3,$4,$5,$6,$7,$8,'human',$9)")
                    .bind(gid).bind(org).bind(mid).bind(&scope).bind(workspace).bind(project).bind(&resource_role).bind(now).bind(self.actor_id)
                    .execute(&mut *self.tx).await.map_err(db_error)?;
                self.audit(org,"authorization.grant.create","resource_grant",Some(gid),"success",json!({"scope_kind":scope,"role":resource_role,"membership_id":mid.to_string(),"invitation_id":proof.invitation_id.to_string()})).await?;
                (gid, now)
            };
            grant = Some(
                json!({"id":gid,"organization_id":org,"subject_kind":"membership","subject_id":mid,"scope_kind":scope,"workspace_id":workspace,"project_id":project,"role":resource_role,"created_at":python_timestamp(created),"revoked_at":null}),
            );
            metadata["grant_id"] = json!(gid.to_string());
        }
        sqlx::query("UPDATE org.invitations SET status='accepted',accepted_at=$1,accepted_by_user_id=$2 WHERE organization_id=$3 AND id=$4")
            .bind(now).bind(self.actor_id).bind(org).bind(proof.invitation_id).execute(&mut *self.tx).await.map_err(db_error)?;
        self.audit(
            org,
            "invitation.accept",
            "invitation",
            Some(proof.invitation_id),
            "success",
            metadata,
        )
        .await?;
        Ok(InvitationResult::Accepted(
            json!({"membership":{"id":mid,"organization_id":org,"user_id":self.actor_id,"role":role,"status":"active"},"grant":grant}),
        ))
    }
}

fn valid_secret(row: &PgRow, proof: &InvitationProof) -> Result<bool, AppError> {
    let expected: Vec<u8> = row.try_get("token_hash").map_err(db_error)?;
    let digest: [u8; 32] = Sha256::digest(proof.secret.as_bytes()).into();
    Ok(bool::from(digest.as_slice().ct_eq(expected.as_slice())))
}
