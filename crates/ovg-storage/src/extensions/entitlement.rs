use super::{db_error, OrgExtensionScope};
use crate::catalog::ProjectRead;
use base64::{engine::general_purpose::STANDARD, Engine};
use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, VerifyingKey};
use ovg_core::AppError;
use ovg_module_sdk::entitlement::{
    verify_grant, CommercialPolicy, GrantKind, LicenseGrant, LicenseState, VerifiedLicense,
};
use serde_json::{json, Value};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;
fn denied() -> AppError {
    AppError::forbidden("commercial license unavailable")
}
fn conflict() -> AppError {
    AppError::conflict(
        "STALE_LICENSE",
        "license sequence already used or superseded",
    )
}
async fn latest(
    tx: &mut Transaction<'_, Postgres>,
    org: Uuid,
    digest: &str,
    key: &str,
    policy: &CommercialPolicy,
    authority: &VerifyingKey,
    now: DateTime<Utc>,
) -> Result<LicenseState, AppError> {
    let row=sqlx::query("SELECT certificate,signature,grant_digest,sequence,extension_key,grant_id FROM extensions.plugin_license_grants WHERE organization_id=$1 AND release_digest=$2 ORDER BY sequence DESC LIMIT 1")
  .bind(org).bind(digest).fetch_optional(&mut **tx).await.map_err(db_error)?;
    let Some(row) = row else {
        return Ok(LicenseState::Missing);
    };
    let g: LicenseGrant = serde_json::from_value(row.try_get("certificate").map_err(db_error)?)
        .map_err(|_| denied())?;
    if g.extension_key != key
        || g.release_digest != digest
        || g.sequence as i64 != row.try_get::<i64, _>("sequence").map_err(db_error)?
        || g.grant_id != row.try_get::<Uuid, _>("grant_id").map_err(db_error)?
        || g.extension_key
            != row
                .try_get::<String, _>("extension_key")
                .map_err(db_error)?
    {
        return Err(denied());
    }
    // Rotation requires reissue; do not accept an old authority's certificate.
    if g.authority_key_id != policy.authority_key_id {
        return Ok(LicenseState::Missing);
    }
    let sig: String = row.try_get("signature").map_err(db_error)?;
    let sig = Signature::from_slice(&STANDARD.decode(sig).map_err(|_| denied())?)
        .map_err(|_| denied())?;
    let proof = verify_grant(g, &sig, authority, policy, org, now)?;
    if proof.digest() != row.try_get::<String, _>("grant_digest").map_err(db_error)? {
        return Err(denied());
    }
    if policy.revokes(proof.grant()) {
        return Ok(LicenseState::Revoked);
    }
    Ok(proof.grant().state(now))
}
impl OrgExtensionScope<'_> {
    pub async fn import_license(
        &mut self,
        proof: &VerifiedLicense,
        signature: &Signature,
    ) -> Result<Value, AppError> {
        self.sandbox_admin().await?;
        let g = proof.grant();
        if g.organization_id != self.organization_id {
            return Err(denied());
        }
        let prior=sqlx::query("SELECT grant_id,sequence,grant_digest,signature FROM extensions.plugin_license_grants WHERE organization_id=$1 AND release_digest=$2 ORDER BY sequence DESC LIMIT 1")
    .bind(self.organization_id).bind(&g.release_digest).fetch_optional(&mut *self.tx).await.map_err(db_error)?;
        let sig = STANDARD.encode(signature.to_bytes());
        if let Some(row) = prior {
            let seq: i64 = row.try_get("sequence").map_err(db_error)?;
            if g.sequence as i64 <= seq {
                if g.sequence as i64 == seq
                    && row.try_get::<Uuid, _>("grant_id").map_err(db_error)? == g.grant_id
                    && row.try_get::<String, _>("grant_digest").map_err(db_error)? == proof.digest()
                    && row.try_get::<String, _>("signature").map_err(db_error)? == sig
                {
                    return Ok(
                        json!({"grant_id":g.grant_id,"sequence":g.sequence,"state":g.state(Utc::now()),"replayed":true}),
                    );
                }
                return Err(conflict());
            }
            if g.kind == GrantKind::Trial {
                return Err(conflict());
            }
        }
        if g.kind == GrantKind::Trial {
            let used:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM extensions.plugin_license_grants WHERE organization_id=$1 AND extension_key=$2)")
      .bind(self.organization_id).bind(&g.extension_key).fetch_one(&mut *self.tx).await.map_err(db_error)?;
            if used {
                return Err(AppError::conflict(
                    "TRIAL_ALREADY_USED",
                    "trial unavailable after prior license",
                ));
            }
        }
        let kind = match g.kind {
            GrantKind::Trial => "trial",
            GrantKind::Subscription => "subscription",
            GrantKind::Revoked => "revoked",
        };
        let certificate = serde_json::to_value(g).map_err(|_| denied())?;
        let inserted=sqlx::query("INSERT INTO extensions.plugin_license_grants(organization_id,grant_id,actor_id,extension_key,release_digest,sequence,kind,grant_digest,certificate,signature) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) ON CONFLICT DO NOTHING")
    .bind(self.organization_id).bind(g.grant_id).bind(self.actor_id).bind(&g.extension_key).bind(&g.release_digest).bind(g.sequence as i64).bind(kind).bind(proof.digest()).bind(certificate).bind(sig)
    .execute(&mut *self.tx).await.map_err(db_error)?;
        if inserted.rows_affected() != 1 {
            return Err(conflict());
        }
        Ok(
            json!({"grant_id":g.grant_id,"sequence":g.sequence,"state":g.state(Utc::now()),"replayed":false}),
        )
    }
    pub async fn license_status(
        &mut self,
        digest: &str,
        key: &str,
        policy: &CommercialPolicy,
        authority: &VerifyingKey,
    ) -> Result<LicenseState, AppError> {
        self.sandbox_admin().await?;
        latest(
            &mut self.tx,
            self.organization_id,
            digest,
            key,
            policy,
            authority,
            Utc::now(),
        )
        .await
    }
}
pub async fn project_license(
    read: &mut ProjectRead<'_>,
    digest: &str,
    key: &str,
    policy: &CommercialPolicy,
    authority: &VerifyingKey,
) -> Result<LicenseState, AppError> {
    if read.is_gis_scoped() || read.actor_type() != "human" {
        return Err(denied());
    }
    let org = read.organization_id();
    latest(
        read.transaction(),
        org,
        digest,
        key,
        policy,
        authority,
        Utc::now(),
    )
    .await
}
