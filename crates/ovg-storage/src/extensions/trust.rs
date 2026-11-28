//! Global monotonic checkpoint for verified offline trust policies.
//! Every configured V2 catalog admission must pass this gate.
use super::db_error;
use crate::DbPool;
use ovg_core::AppError;
use ovg_module_sdk::trust::{TrustPolicy, VerifiedTrust};
use sqlx::Row;
pub async fn admit_policy(pool: &DbPool, verified: &VerifiedTrust) -> Result<(), AppError> {
    let mut tx = pool.inner().begin().await.map_err(db_error)?;
    // One global transaction lock across all API instances, independent of tenant.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('openvietgis.marketplace.global-trust.v1',0))")
        .execute(&mut *tx).await.map_err(db_error)?;
    let safe:bool=sqlx::query_scalar("SELECT current_user='openvietgis_runtime' AND row_security_active('extensions.plugin_trust_checkpoints'::regclass)")
        .fetch_one(&mut *tx).await.map_err(db_error)?;
    if !safe {
        return Err(AppError::internal("trust checkpoint requires runtime role"));
    }
    let p = verified.policy();
    if let Some(row)=sqlx::query("SELECT sequence,catalog_epoch,policy_digest,root_key_id,policy FROM extensions.plugin_trust_checkpoints ORDER BY sequence DESC LIMIT 1")
        .fetch_optional(&mut *tx).await.map_err(db_error)?{
        let prior:TrustPolicy=serde_json::from_value(row.try_get("policy").map_err(db_error)?).map_err(|_|AppError::internal("invalid persisted trust checkpoint"))?;
        let prior_digest:String=row.try_get("policy_digest").map_err(db_error)?;
        let prior_root:String=row.try_get("root_key_id").map_err(db_error)?;
        if row.try_get::<i64,_>("sequence").map_err(db_error)?!=prior.sequence as i64||
            row.try_get::<i64,_>("catalog_epoch").map_err(db_error)?!=prior.catalog_epoch as i64||
            ovg_module_sdk::trust::fingerprint(&ovg_module_sdk::release_v2::canonical(&prior)?)!=prior_digest{
            return Err(AppError::internal("corrupt trust checkpoint"));
        }
        verified.ensure_successor(&prior,&prior_digest,&prior_root)?;
        if p.sequence==prior.sequence{return tx.commit().await.map_err(db_error);}
    }
    sqlx::query("INSERT INTO extensions.plugin_trust_checkpoints(sequence,catalog_epoch,policy_digest,root_key_id,policy) VALUES($1,$2,$3,$4,$5)")
        .bind(p.sequence as i64).bind(p.catalog_epoch as i64).bind(verified.digest()).bind(verified.root_key_id())
        .bind(serde_json::to_value(p).map_err(|_|AppError::internal("invalid trust policy"))?)
        .execute(&mut *tx).await.map_err(db_error)?;
    tx.commit().await.map_err(db_error)
}
