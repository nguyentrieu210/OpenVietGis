//! Browser sessions and self-scoped identity reads against the existing PostgreSQL schema.

use ovg_core::AppError;
use serde::Serialize;
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
}
