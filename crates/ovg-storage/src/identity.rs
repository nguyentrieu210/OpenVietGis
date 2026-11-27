//! Browser sessions and self-scoped identity reads against the existing PostgreSQL schema.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{Duration, Utc};
use ovg_core::AppError;
use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Row, Transaction};
use subtle::ConstantTimeEq;
use uuid::Uuid;

use crate::DbPool;

#[derive(Serialize)]
pub struct UserProfile {
    pub id: Uuid,
    pub display_name: String,
    pub email_snapshot: Option<String>,
    pub status: String,
}

pub struct VerifiedSession {
    id: Uuid,
    user: UserProfile,
}

impl VerifiedSession {
    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn user_id(&self) -> Uuid {
        self.user.id
    }

    pub fn display_name(&self) -> &str {
        &self.user.display_name
    }

    pub fn into_user(self) -> UserProfile {
        self.user
    }
}

pub enum SessionAuthenticationError {
    Invalid,
    Revoked,
    Expired,
    Database(AppError),
}

pub async fn authenticate_session(
    pool: &DbPool,
    token: &str,
) -> Result<VerifiedSession, SessionAuthenticationError> {
    let (id, secret) = parse_session_token(token).ok_or(SessionAuthenticationError::Invalid)?;
    let row = sqlx::query(
        "SELECT s.secret_hash, s.revoked_at IS NOT NULL AS revoked,
                s.expires_at <= clock_timestamp() AS expired,
                u.id AS user_id, u.display_name, u.email_snapshot, u.status
         FROM identity.web_sessions AS s
         JOIN identity.users AS u ON u.id = s.user_id
         JOIN identity.identity_links AS l ON l.id = s.identity_link_id AND l.user_id = u.id
         WHERE s.id = $1",
    )
    .bind(id)
    .fetch_optional(pool.inner())
    .await
    .map_err(|err| SessionAuthenticationError::Database(db_error(err)))?
    .ok_or(SessionAuthenticationError::Invalid)?;

    let decoded = (|| -> Result<_, sqlx::Error> {
        Ok((
            row.try_get::<Vec<u8>, _>("secret_hash")?,
            row.try_get::<bool, _>("revoked")?,
            row.try_get::<bool, _>("expired")?,
            UserProfile {
                id: row.try_get("user_id")?,
                display_name: row.try_get("display_name")?,
                email_snapshot: row.try_get("email_snapshot")?,
                status: row.try_get("status")?,
            },
        ))
    })()
    .map_err(|err| SessionAuthenticationError::Database(db_error(err)))?;
    let (secret_hash, revoked, expired, user) = decoded;

    if revoked || user.status != "active" {
        return Err(SessionAuthenticationError::Revoked);
    }
    if expired {
        return Err(SessionAuthenticationError::Expired);
    }
    if !verify_digest(secret, &secret_hash) {
        return Err(SessionAuthenticationError::Invalid);
    }
    Ok(VerifiedSession { id, user })
}

fn parse_session_token(token: &str) -> Option<(Uuid, &str)> {
    let (id, secret) = token.strip_prefix("ovgs_")?.split_once('.')?;
    Some((Uuid::parse_str(id).ok()?, secret))
}

fn verify_digest(secret: &str, expected: &[u8]) -> bool {
    let digest: [u8; 32] = Sha256::digest(secret.as_bytes()).into();
    bool::from(digest.as_slice().ct_eq(expected))
}

#[derive(Serialize)]
pub struct OrganizationProfile {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub status: String,
}

#[derive(Serialize)]
pub struct OrganizationMembership {
    #[serde(flatten)]
    pub organization: OrganizationProfile,
    pub role: String,
}

pub async fn active_memberships(
    pool: &DbPool,
    session: &VerifiedSession,
    organization_id: Option<Uuid>,
) -> Result<Vec<OrganizationMembership>, AppError> {
    let mut tx = begin_self_read(pool, session).await?;
    let rows = sqlx::query(
        "SELECT o.id, o.name, o.slug, m.role, o.status
         FROM org.organization_memberships AS m
         JOIN org.organizations AS o ON o.id = m.organization_id
         WHERE m.user_id = $1 AND m.status = 'active'
           AND ($2::uuid IS NULL OR o.id = $2)
         ORDER BY o.name, o.id",
    )
    .bind(session.user_id())
    .bind(organization_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(db_error)?;
    tx.commit().await.map_err(db_error)?;

    rows.into_iter()
        .map(|row| -> Result<_, sqlx::Error> {
            Ok(OrganizationMembership {
                organization: OrganizationProfile {
                    id: row.try_get("id")?,
                    name: row.try_get("name")?,
                    slug: row.try_get("slug")?,
                    status: row.try_get("status")?,
                },
                role: row.try_get("role")?,
            })
        })
        .collect::<Result<_, _>>()
        .map_err(db_error)
}

#[derive(Serialize)]
pub struct IdentityLink {
    pub id: Uuid,
    pub issuer: String,
    pub subject: String,
    pub email_snapshot: Option<String>,
    pub email_verified: bool,
}

pub async fn list_identity_links(
    pool: &DbPool,
    session: &VerifiedSession,
) -> Result<Vec<IdentityLink>, AppError> {
    let mut tx = begin_self_read(pool, session).await?;
    let rows = sqlx::query(
        "SELECT id, issuer, subject, email_snapshot, email_verified
         FROM identity.identity_links WHERE user_id = $1 ORDER BY created_at",
    )
    .bind(session.user_id())
    .fetch_all(&mut *tx)
    .await
    .map_err(db_error)?;
    tx.commit().await.map_err(db_error)?;

    rows.into_iter()
        .map(|row| -> Result<_, sqlx::Error> {
            Ok(IdentityLink {
                id: row.try_get("id")?,
                issuer: row.try_get("issuer")?,
                subject: row.try_get("subject")?,
                email_snapshot: row.try_get("email_snapshot")?,
                email_verified: row.try_get("email_verified")?,
            })
        })
        .collect::<Result<_, _>>()
        .map_err(db_error)
}

pub async fn get_avatar(
    pool: &DbPool,
    session: &VerifiedSession,
) -> Result<Option<String>, AppError> {
    let mut tx = begin_self_read(pool, session).await?;
    let avatar =
        sqlx::query_scalar("SELECT data_url FROM identity.user_avatars WHERE user_id = $1")
            .bind(session.user_id())
            .fetch_optional(&mut *tx)
            .await
            .map_err(db_error)?;
    tx.commit().await.map_err(db_error)?;
    Ok(avatar)
}

pub async fn installation_initialized(pool: &DbPool) -> Result<bool, AppError> {
    sqlx::query_scalar(
        "SELECT initialized_at IS NOT NULL FROM platform.installation_state
         WHERE singleton_key = 'installation'",
    )
    .fetch_one(pool.inner())
    .await
    .map_err(db_error)
}

#[derive(Clone, Debug)]
pub struct ExternalIdentityClaims {
    pub issuer: String,
    pub subject: String,
    pub display_name: String,
    pub email: Option<String>,
    pub email_verified: bool,
}

#[derive(Clone, Debug)]
pub struct LoginOutcome {
    pub user_id: Uuid,
    pub identity_link_id: Uuid,
    pub session_id: Uuid,
    pub session_token: String,
}

pub enum OidcIdentityError {
    Conflict(&'static str),
    LinkConflict,
    InvalidLinkSession(&'static str),
    Storage(AppError),
}

impl From<AppError> for OidcIdentityError {
    fn from(error: AppError) -> Self {
        Self::Storage(error)
    }
}

pub fn random_urlsafe_token(num_bytes: usize) -> Result<String, AppError> {
    let mut bytes = vec![0u8; num_bytes];
    getrandom::getrandom(&mut bytes)
        .map_err(|err| AppError::internal(format!("CSPRNG failed: {err}")))?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

fn issue_session_token(session_id: Uuid) -> Result<(String, [u8; 32]), AppError> {
    let secret = format!("ovg_s_{}", random_urlsafe_token(32)?);
    let token = format!("ovgs_{}.{}", session_id.simple(), secret);
    let digest: [u8; 32] = Sha256::digest(secret.as_bytes()).into();
    Ok((token, digest))
}

async fn lock_external_identity(
    tx: &mut Transaction<'_, Postgres>,
    issuer: &str,
    subject: &str,
) -> Result<(), AppError> {
    let key = format!("openvietgis.identity:{}:{}{}", issuer.chars().count(), issuer, subject);
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(&key)
        .execute(&mut **tx)
        .await
        .map_err(db_error)?;
    Ok(())
}

async fn find_external_identity_row(
    tx: &mut Transaction<'_, Postgres>,
    issuer: &str,
    subject: &str,
) -> Result<Option<(Uuid, String, Uuid)>, AppError> {
    let row = sqlx::query(
        "SELECT u.id AS user_id, u.status, l.id AS identity_link_id
         FROM identity.identity_links AS l
         JOIN identity.users AS u ON u.id = l.user_id
         WHERE l.issuer = $1 AND l.subject = $2",
    )
    .bind(issuer)
    .bind(subject)
    .fetch_optional(&mut **tx)
    .await
    .map_err(db_error)?;

    row.map(|row| -> Result<_, sqlx::Error> {
        Ok((
            row.try_get("user_id")?,
            row.try_get("status")?,
            row.try_get("identity_link_id")?,
        ))
    })
    .transpose()
    .map_err(db_error)
}

async fn insert_identity_link_audit(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    display_name: &str,
    resource_id: Option<Uuid>,
    outcome: &str,
    metadata: serde_json::Value,
    created_at: chrono::DateTime<Utc>,
) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO audit.audit_events (
             id, organization_id, actor_type, actor_id, actor_display_snapshot,
             action, resource_type, resource_id, outcome, metadata, created_at
         )
         VALUES ($1, NULL, 'human', $2, $3, 'identity.link', 'identity_link', $4, $5, $6, $7)",
    )
    .bind(Uuid::new_v4())
    .bind(user_id)
    .bind(display_name)
    .bind(resource_id)
    .bind(outcome)
    .bind(metadata)
    .bind(created_at)
    .execute(&mut **tx)
    .await
    .map_err(db_error)?;
    Ok(())
}

pub async fn complete_oidc_login(
    pool: &DbPool,
    claims: &ExternalIdentityClaims,
    session_ttl_seconds: i64,
) -> Result<LoginOutcome, OidcIdentityError> {
    let mut tx = pool.inner().begin().await.map_err(db_error)?;
    lock_external_identity(&mut tx, &claims.issuer, &claims.subject).await?;

    let now = Utc::now();
    let existing = find_external_identity_row(&mut tx, &claims.issuer, &claims.subject).await?;
    let (user_id, identity_link_id) = match existing {
        None => {
            let user_id = Uuid::new_v4();
            let link_id = Uuid::new_v4();
            sqlx::query(
                "INSERT INTO identity.users (
                     id, display_name, email_snapshot, status, created_at, updated_at
                 )
                 VALUES ($1, $2, $3, 'active', $4, $4)",
            )
            .bind(user_id)
            .bind(&claims.display_name)
            .bind(&claims.email)
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(db_error)?;

            sqlx::query(
                "INSERT INTO identity.identity_links (
                     id, user_id, issuer, subject, email_snapshot, email_verified,
                     display_name_snapshot, created_at, last_login_at
                 )
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $8)",
            )
            .bind(link_id)
            .bind(user_id)
            .bind(&claims.issuer)
            .bind(&claims.subject)
            .bind(&claims.email)
            .bind(claims.email_verified)
            .bind(&claims.display_name)
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(db_error)?;
            (user_id, link_id)
        }
        Some((user_id, status, link_id)) => {
            if status != "active" {
                return Err(OidcIdentityError::Conflict("user is disabled"));
            }
            sqlx::query(
                "UPDATE identity.identity_links
                 SET last_login_at = $1,
                     email_snapshot = $2,
                     email_verified = $3,
                     display_name_snapshot = $4
                 WHERE id = $5 AND user_id = $6",
            )
            .bind(now)
            .bind(&claims.email)
            .bind(claims.email_verified)
            .bind(&claims.display_name)
            .bind(link_id)
            .bind(user_id)
            .execute(&mut *tx)
            .await
            .map_err(db_error)?;
            (user_id, link_id)
        }
    };

    let session_id = Uuid::new_v4();
    let (session_token, secret_hash) = issue_session_token(session_id)?;
    let expires_at = now + Duration::seconds(session_ttl_seconds);
    sqlx::query(
        "INSERT INTO identity.web_sessions (
             id, user_id, identity_link_id, secret_hash,
             issued_at, expires_at, last_seen_at, revoked_at
         )
         VALUES ($1, $2, $3, $4, $5, $6, NULL, NULL)",
    )
    .bind(session_id)
    .bind(user_id)
    .bind(identity_link_id)
    .bind(secret_hash.as_slice())
    .bind(now)
    .bind(expires_at)
    .execute(&mut *tx)
    .await
    .map_err(db_error)?;

    tx.commit().await.map_err(db_error)?;
    Ok(LoginOutcome {
        user_id,
        identity_link_id,
        session_id,
        session_token,
    })
}

pub async fn complete_oidc_link(
    pool: &DbPool,
    user_id: Uuid,
    session_id: Uuid,
    claims: &ExternalIdentityClaims,
) -> Result<Uuid, OidcIdentityError> {
    let mut tx = pool.inner().begin().await.map_err(db_error)?;

    let active_session = sqlx::query_scalar::<_, Uuid>(
        "SELECT id
         FROM identity.web_sessions
         WHERE id = $1
           AND user_id = $2
           AND revoked_at IS NULL
           AND expires_at > clock_timestamp()
         FOR UPDATE",
    )
    .bind(session_id)
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(db_error)?;
    if active_session.is_none() {
        return Err(OidcIdentityError::InvalidLinkSession(
            "link session no longer valid",
        ));
    }

    let user_row = sqlx::query("SELECT display_name, status FROM identity.users WHERE id = $1")
        .bind(user_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db_error)?;
    let Some(user_row) = user_row else {
        return Err(OidcIdentityError::Conflict("user not found"));
    };
    let display_name: String = user_row.try_get("display_name").map_err(db_error)?;
    let status: String = user_row.try_get("status").map_err(db_error)?;
    if status != "active" {
        return Err(OidcIdentityError::Conflict("user not found"));
    }

    let now = Utc::now();
    lock_external_identity(&mut tx, &claims.issuer, &claims.subject).await?;
    if let Some((existing_user_id, _, existing_link_id)) =
        find_external_identity_row(&mut tx, &claims.issuer, &claims.subject).await?
    {
        if existing_user_id == user_id {
            tx.commit().await.map_err(db_error)?;
            return Ok(existing_link_id);
        }
        insert_identity_link_audit(
            &mut tx,
            user_id,
            &display_name,
            None,
            "denied",
            json!({ "reason": "identity_conflict" }),
            now,
        )
        .await?;
        tx.commit().await.map_err(db_error)?;
        return Err(OidcIdentityError::LinkConflict);
    }

    let new_link_id = Uuid::new_v4();
    let inserted = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO identity.identity_links (
             id, user_id, issuer, subject, email_snapshot, email_verified,
             display_name_snapshot, created_at, last_login_at
         )
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $8)
         ON CONFLICT (issuer, subject) DO NOTHING
         RETURNING id",
    )
    .bind(new_link_id)
    .bind(user_id)
    .bind(&claims.issuer)
    .bind(&claims.subject)
    .bind(&claims.email)
    .bind(claims.email_verified)
    .bind(&claims.display_name)
    .bind(now)
    .fetch_optional(&mut *tx)
    .await
    .map_err(db_error)?;

    if inserted.is_none() {
        if let Some((existing_user_id, _, existing_link_id)) =
            find_external_identity_row(&mut tx, &claims.issuer, &claims.subject).await?
        {
            if existing_user_id == user_id {
                tx.commit().await.map_err(db_error)?;
                return Ok(existing_link_id);
            }
        }
        insert_identity_link_audit(
            &mut tx,
            user_id,
            &display_name,
            None,
            "denied",
            json!({ "reason": "identity_conflict" }),
            now,
        )
        .await?;
        tx.commit().await.map_err(db_error)?;
        return Err(OidcIdentityError::LinkConflict);
    }

    insert_identity_link_audit(
        &mut tx,
        user_id,
        &display_name,
        Some(new_link_id),
        "success",
        json!({ "issuer": claims.issuer }),
        now,
    )
    .await?;
    tx.commit().await.map_err(db_error)?;
    Ok(new_link_id)
}

pub async fn verify_password_action_identity(
    pool: &DbPool,
    user_id: Uuid,
    claims: &ExternalIdentityClaims,
) -> Result<bool, AppError> {
    let found = sqlx::query_scalar::<_, Uuid>(
        "SELECT u.id
         FROM identity.identity_links AS l
         JOIN identity.users AS u ON u.id = l.user_id
         WHERE l.issuer = $1 AND l.subject = $2",
    )
    .bind(&claims.issuer)
    .bind(&claims.subject)
    .fetch_optional(pool.inner())
    .await
    .map_err(db_error)?;
    Ok(found == Some(user_id))
}

async fn begin_self_read(
    pool: &DbPool,
    session: &VerifiedSession,
) -> Result<Transaction<'static, Postgres>, AppError> {
    let mut tx = pool.inner().begin().await.map_err(db_error)?;
    sqlx::query("SET TRANSACTION READ ONLY")
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
    // Self-membership discovery must carry the verified actor and no organization scope.
    sqlx::query(crate::rls::BIND_TENANT_GUCS_SQL)
        .bind("")
        .bind(session.user_id().to_string())
        .bind("")
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
    Ok(tx)
}

fn db_error(err: sqlx::Error) -> AppError {
    AppError::internal(format!("identity storage query failed: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_canonical_python_session_token_and_hashes_only_the_secret() {
        let raw = "ovgs_10000000000000000000000000000001.s_example";
        let (id, secret) = parse_session_token(raw).unwrap();
        assert_eq!(id, Uuid::from_u128(0x10000000000000000000000000000001));
        assert_eq!(secret, "s_example");
        let oracle = [
            0x8c, 0xf7, 0x87, 0xfc, 0x8e, 0x65, 0xef, 0x99, 0x53, 0x7f, 0x42, 0x62, 0x4a, 0xd7,
            0x65, 0xf0, 0x2f, 0xda, 0x90, 0xed, 0xca, 0xe3, 0xac, 0x80, 0x3f, 0xaa, 0x8a, 0xfc,
            0x44, 0x74, 0xbe, 0x0d,
        ];
        assert!(verify_digest(secret, &oracle));
        assert!(!verify_digest(raw, &oracle));
        assert!(!verify_digest("s_tampered", &oracle));
        assert!(!verify_digest(secret, &oracle[..31]));
        assert!(parse_session_token("other_10000000000000000000000000000001.s_example").is_none());
        assert!(parse_session_token("ovgs_not-a-uuid.s_example").is_none());
        assert!(parse_session_token("ovgs_10000000000000000000000000000001").is_none());
    }

    #[test]
    fn issued_session_token_roundtrips_through_parser_and_digest() {
        let session_id = Uuid::from_u128(0x20000000000000000000000000000002);
        let (token, secret_hash) = issue_session_token(session_id).unwrap();
        let (parsed_id, secret) = parse_session_token(&token).unwrap();
        assert_eq!(parsed_id, session_id);
        assert!(secret.starts_with("ovg_s_"));
        assert!(verify_digest(secret, &secret_hash));
        assert!(!verify_digest("tampered", &secret_hash));
    }
}
