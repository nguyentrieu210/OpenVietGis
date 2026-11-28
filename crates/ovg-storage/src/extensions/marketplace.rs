//! E14 marketplace publisher & submission review storage over canonical RLS tables.

use chrono::{DateTime, Utc};
use ovg_core::AppError;
use serde_json::{json, Value};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use crate::DbPool;

use super::{db_error, iso_z};

pub const DEVELOPER_QUOTA: i64 = 30;

#[derive(Debug, Clone)]
pub struct PublisherRow {
    pub id: String,
    pub name: String,
    pub public_key: String,
    pub created_at: DateTime<Utc>,
    pub key_revision: i64,
}

impl PublisherRow {
    #[must_use]
    pub fn wire(&self) -> Value {
        json!({
            "id": self.id,
            "name": self.name,
            "public_key": self.public_key,
            "created_at": iso_z(self.created_at),
            "key_revision":self.key_revision,
        })
    }
}

#[derive(Debug, Clone)]
pub struct SubmissionRow {
    pub id: Uuid,
    pub publisher_id: String,
    pub owner_user_id: Uuid,
    pub manifest: Value,
    pub signature: String,
    pub status: String,
    pub review_note: Option<String>,
    pub reviewed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub publisher_name: String,
    pub public_key: String,
}

impl SubmissionRow {
    #[must_use]
    pub fn wire(&self, viewer_uid: Uuid, published: bool) -> Value {
        json!({
            "id": self.id,
            "publisher_id": self.publisher_id,
            "manifest": self.manifest,
            "signature": self.signature,
            "status": self.status,
            "review_note": self.review_note,
            "reviewed_at": self.reviewed_at.map(iso_z),
            "created_at": iso_z(self.created_at),
            "publisher_name": self.publisher_name,
            "public_key": self.public_key,
            "own": self.owner_user_id == viewer_uid,
            "published": published,
        })
    }
}

fn publisher_from_row(row: &sqlx::postgres::PgRow) -> Result<PublisherRow, AppError> {
    Ok(PublisherRow {
        id: row.try_get("id").map_err(db_error)?,
        name: row.try_get("name").map_err(db_error)?,
        public_key: row.try_get("public_key").map_err(db_error)?,
        created_at: row.try_get("created_at").map_err(db_error)?,
        key_revision: row.try_get("key_revision").map_err(db_error)?,
    })
}

fn submission_from_row(row: &sqlx::postgres::PgRow) -> Result<SubmissionRow, AppError> {
    Ok(SubmissionRow {
        id: row.try_get("id").map_err(db_error)?,
        publisher_id: row.try_get("publisher_id").map_err(db_error)?,
        owner_user_id: row.try_get("owner_user_id").map_err(db_error)?,
        manifest: row.try_get("manifest").map_err(db_error)?,
        signature: row.try_get("signature").map_err(db_error)?,
        status: row.try_get("status").map_err(db_error)?,
        review_note: row.try_get("review_note").map_err(db_error)?,
        reviewed_at: row.try_get("reviewed_at").map_err(db_error)?,
        created_at: row.try_get("created_at").map_err(db_error)?,
        publisher_name: row.try_get("publisher_name").map_err(db_error)?,
        public_key: row.try_get("public_key").map_err(db_error)?,
    })
}

const SELECT_SUBMISSION: &str = "
    SELECT s.id, s.publisher_id, s.owner_user_id, s.manifest, s.signature, s.status,
           s.review_note, s.reviewed_at, s.created_at, p.name AS publisher_name, s.signing_public_key AS public_key
    FROM extensions.marketplace_submissions s
    JOIN extensions.marketplace_publishers p ON p.id = s.publisher_id
";

/// Transaction for human developer/reviewer marketplace actions under RLS.
pub struct DeveloperTx<'a> {
    tx: Transaction<'a, Postgres>,
    actor_id: Uuid,
    reviewer: bool,
}

impl<'a> DeveloperTx<'a> {
    pub async fn begin(
        pool: &'a DbPool,
        actor_id: Uuid,
        session_id: Uuid,
        reviewer: bool,
    ) -> Result<Self, AppError> {
        let mut tx = pool.inner().begin().await.map_err(db_error)?;
        sqlx::query(crate::rls::BIND_TENANT_GUCS_SQL)
            .bind("")
            .bind(actor_id.to_string())
            .bind("")
            .execute(&mut *tx)
            .await
            .map_err(db_error)?;

        let safe_role: bool = sqlx::query_scalar(
            "SELECT current_user = 'openvietgis_runtime'
                 AND row_security_active('extensions.marketplace_publishers'::regclass)
                 AND row_security_active('extensions.marketplace_submissions'::regclass)",
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(db_error)?;
        if !safe_role {
            return Err(AppError::internal(
                "marketplace requires the PostGIS runtime RLS role",
            ));
        }

        let current_session: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                 SELECT 1
                 FROM identity.web_sessions AS s
                 JOIN identity.users AS u ON u.id = s.user_id
                 JOIN identity.identity_links AS l ON l.id = s.identity_link_id AND l.user_id = u.id
                 WHERE s.id = $1 AND u.id = $2 AND u.status = 'active'
                   AND s.revoked_at IS NULL AND s.expires_at > clock_timestamp()
             )",
        )
        .bind(session_id)
        .bind(actor_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(db_error)?;
        if !current_session {
            return Err(AppError::unauthorized("authentication required"));
        }

        sqlx::query("SELECT set_config('openvietgis.marketplace_reviewer', $1, true)")
            .bind(if reviewer { "yes" } else { "no" })
            .execute(&mut *tx)
            .await
            .map_err(db_error)?;
        Ok(Self {
            tx,
            actor_id,
            reviewer,
        })
    }

    pub async fn lock_user(&mut self) -> Result<(), AppError> {
        let lock_key = format!("marketplace-publisher:{}", self.actor_id);
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(&lock_key)
            .execute(&mut *self.tx)
            .await
            .map_err(db_error)?;
        Ok(())
    }

    pub async fn get_publisher(&mut self) -> Result<Option<PublisherRow>, AppError> {
        let row = sqlx::query(
            "SELECT id, name, public_key, created_at,key_revision
             FROM extensions.marketplace_publishers
             WHERE owner_user_id = $1",
        )
        .bind(self.actor_id)
        .fetch_optional(&mut *self.tx)
        .await
        .map_err(db_error)?;
        row.as_ref().map(publisher_from_row).transpose()
    }

    pub async fn list_own_submissions(&mut self) -> Result<Vec<SubmissionRow>, AppError> {
        let sql = format!(
            "{SELECT_SUBMISSION} WHERE s.owner_user_id = $1 ORDER BY s.created_at DESC, s.id"
        );
        let rows = sqlx::query(&sql)
            .bind(self.actor_id)
            .fetch_all(&mut *self.tx)
            .await
            .map_err(db_error)?;
        rows.iter().map(submission_from_row).collect()
    }

    pub async fn insert_publisher(
        &mut self,
        pid: &str,
        name: &str,
        public_key: &str,
    ) -> Result<PublisherRow, AppError> {
        let row = sqlx::query(
            "INSERT INTO extensions.marketplace_publishers (id, owner_user_id, name, public_key)
             VALUES ($1, $2, $3, $4)
             RETURNING id, name, public_key, created_at,key_revision",
        )
        .bind(pid)
        .bind(self.actor_id)
        .bind(name)
        .bind(public_key)
        .fetch_one(&mut *self.tx)
        .await
        .map_err(db_error)?;
        publisher_from_row(&row)
    }

    pub async fn count_own_submissions(&mut self) -> Result<i64, AppError> {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM extensions.marketplace_submissions WHERE owner_user_id = $1",
        )
        .bind(self.actor_id)
        .fetch_one(&mut *self.tx)
        .await
        .map_err(db_error)
    }

    pub async fn insert_submission(
        &mut self,
        id: Uuid,
        publisher_id: &str,
        extension_key: &str,
        version: &str,
        manifest: &Value,
        signature: &str,
    ) -> Result<Option<SubmissionRow>, AppError> {
        let inserted: Option<Uuid> = sqlx::query_scalar(
            "INSERT INTO extensions.marketplace_submissions
               (id, publisher_id, owner_user_id, extension_key, version, manifest, signature)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             ON CONFLICT (extension_key, version) DO NOTHING
             RETURNING id",
        )
        .bind(id)
        .bind(publisher_id)
        .bind(self.actor_id)
        .bind(extension_key)
        .bind(version)
        .bind(manifest)
        .bind(signature)
        .fetch_optional(&mut *self.tx)
        .await
        .map_err(db_error)?;
        let Some(mid) = inserted else {
            return Ok(None);
        };
        let sql = format!("{SELECT_SUBMISSION} WHERE s.id = $1");
        let row = sqlx::query(&sql)
            .bind(mid)
            .fetch_one(&mut *self.tx)
            .await
            .map_err(db_error)?;
        submission_from_row(&row).map(Some)
    }

    pub async fn withdraw_submission(
        &mut self,
        submission_id: Uuid,
    ) -> Result<SubmissionRow, AppError> {
        let found: Option<String> = sqlx::query_scalar(
            "SELECT status FROM extensions.marketplace_submissions
             WHERE id = $1 AND owner_user_id = $2",
        )
        .bind(submission_id)
        .bind(self.actor_id)
        .fetch_optional(&mut *self.tx)
        .await
        .map_err(db_error)?;
        if found.is_none() {
            return Err(AppError::not_found("submission not found"));
        }

        let changed: Option<Uuid> = sqlx::query_scalar(
            "UPDATE extensions.marketplace_submissions SET status = 'withdrawn'
             WHERE id = $1 AND owner_user_id = $2 AND status = 'pending'
             RETURNING id",
        )
        .bind(submission_id)
        .bind(self.actor_id)
        .fetch_optional(&mut *self.tx)
        .await
        .map_err(db_error)?;
        if changed.is_none() {
            return Err(AppError::conflict(
                "CONFLICT",
                "only pending submissions may be withdrawn",
            ));
        }

        let sql = format!("{SELECT_SUBMISSION} WHERE s.id = $1");
        let row = sqlx::query(&sql)
            .bind(submission_id)
            .fetch_one(&mut *self.tx)
            .await
            .map_err(db_error)?;
        submission_from_row(&row)
    }

    pub async fn list_review_queue(
        &mut self,
        state: &str,
        offset: i64,
    ) -> Result<Vec<SubmissionRow>, AppError> {
        let sql = format!(
            "{SELECT_SUBMISSION} WHERE s.status = $1 ORDER BY s.created_at, s.id LIMIT 101 OFFSET $2"
        );
        let rows = sqlx::query(&sql)
            .bind(state)
            .bind(offset)
            .fetch_all(&mut *self.tx)
            .await
            .map_err(db_error)?;
        rows.iter().map(submission_from_row).collect()
    }

    pub async fn decide_submission(
        &mut self,
        submission_id: Uuid,
        decision: &str,
        note: &str,
    ) -> Result<SubmissionRow, AppError> {
        let existing: Option<(Uuid, String)> = sqlx::query_as(
            "SELECT owner_user_id, status FROM extensions.marketplace_submissions WHERE id = $1",
        )
        .bind(submission_id)
        .fetch_optional(&mut *self.tx)
        .await
        .map_err(db_error)?;
        let Some((owner_user_id, _status)) = existing else {
            return Err(AppError::not_found("submission not found"));
        };
        if owner_user_id == self.actor_id {
            return Err(AppError::forbidden("self-review forbidden"));
        }

        let changed: Option<Uuid> = sqlx::query_scalar(
            "UPDATE extensions.marketplace_submissions
             SET status = $2, review_note = $3, reviewed_by = $4, reviewed_at = now()
             WHERE id = $1 AND status = 'pending' AND owner_user_id <> $4
             RETURNING id",
        )
        .bind(submission_id)
        .bind(decision)
        .bind(note)
        .bind(self.actor_id)
        .fetch_optional(&mut *self.tx)
        .await
        .map_err(db_error)?;
        if changed.is_none() {
            return Err(AppError::conflict("CONFLICT", "submission already decided"));
        }

        let sql = format!("{SELECT_SUBMISSION} WHERE s.id = $1");
        let row = sqlx::query(&sql)
            .bind(submission_id)
            .fetch_one(&mut *self.tx)
            .await
            .map_err(db_error)?;
        submission_from_row(&row)
    }

    pub async fn commit(self) -> Result<(), AppError> {
        self.tx.commit().await.map_err(db_error)
    }
}

// SPEC075 uses the same publisher and v1 submission identity. The bundle is
// immutable; moderation and withdrawal requests never edit signed bytes.
impl DeveloperTx<'_> {
    pub async fn parent_submission(&mut self, id: Uuid) -> Result<SubmissionRow, AppError> {
        let sql = format!("{SELECT_SUBMISSION} WHERE s.id=$1");
        let row = sqlx::query(&sql)
            .bind(id)
            .fetch_optional(&mut *self.tx)
            .await
            .map_err(db_error)?
            .ok_or_else(|| AppError::not_found("submission not found"))?;
        submission_from_row(&row)
    }
    pub async fn attach_v2(
        &mut self,
        id: Uuid,
        bundle: &Value,
        digest: &str,
    ) -> Result<Value, AppError> {
        self.lock_user().await?;
        let parent = self.parent_submission(id).await?;
        if parent.owner_user_id != self.actor_id
            || !matches!(parent.status.as_str(), "pending" | "approved")
        {
            return Err(AppError::forbidden("active owned submission required"));
        }
        let inserted=sqlx::query("INSERT INTO extensions.marketplace_v2_submissions(submission_id,owner_user_id,bundle,release_digest)
          VALUES($1,$2,$3,$4) ON CONFLICT(submission_id) DO NOTHING")
            .bind(id).bind(self.actor_id).bind(bundle).bind(digest).execute(&mut *self.tx).await.map_err(db_error)?;
        let row = self.v2_detail(id).await?;
        if inserted.rows_affected() == 0
            && (row["release_digest"].as_str() != Some(digest) || row["bundle"] != *bundle)
        {
            return Err(AppError::conflict(
                "V2_SUBMISSION_CONFLICT",
                "bundle already attached; use new version",
            ));
        }
        Ok(row)
    }
    pub async fn v2_detail(&mut self, id: Uuid) -> Result<Value, AppError> {
        sqlx::query_scalar("SELECT jsonb_build_object('submission_id',v.submission_id,'status',v.status,
          'release_digest',v.release_digest,'bundle',v.bundle,'review_note',v.review_note,'reviewed_at',v.reviewed_at,
          'parent_status',s.status,'publisher_id',s.publisher_id,'extension_key',s.extension_key,'version',s.version,
          'own',v.owner_user_id=$2,'withdrawal_requested',w.submission_id IS NOT NULL,'withdrawal_reason',w.reason)
          FROM extensions.marketplace_v2_submissions v JOIN extensions.marketplace_submissions s ON s.id=v.submission_id
          LEFT JOIN extensions.marketplace_v2_withdrawals w ON w.submission_id=v.submission_id WHERE v.submission_id=$1")
            .bind(id).bind(self.actor_id).fetch_optional(&mut *self.tx).await.map_err(db_error)?
            .ok_or_else(||AppError::not_found("v2 submission not found"))
    }
    pub async fn v2_list(&mut self, status: &str, offset: i64) -> Result<Vec<Value>, AppError> {
        sqlx::query_scalar("SELECT jsonb_build_object('submission_id',v.submission_id,'status',v.status,
          'release_digest',v.release_digest,'review_note',v.review_note,'parent_status',s.status,'publisher_id',s.publisher_id,
          'extension_key',s.extension_key,'version',s.version,'own',v.owner_user_id=$3,'withdrawal_requested',w.submission_id IS NOT NULL,'withdrawal_reason',w.reason)
          FROM extensions.marketplace_v2_submissions v JOIN extensions.marketplace_submissions s ON s.id=v.submission_id
          LEFT JOIN extensions.marketplace_v2_withdrawals w ON w.submission_id=v.submission_id
          WHERE ($1='all' OR v.status=$1) AND ($4 OR v.owner_user_id=$3)
          ORDER BY v.created_at DESC,v.submission_id LIMIT 101 OFFSET $2")
            .bind(status).bind(offset).bind(self.actor_id).bind(self.reviewer)
            .fetch_all(&mut *self.tx).await.map_err(db_error)
    }
    pub async fn v2_decide(
        &mut self,
        id: Uuid,
        decision: &str,
        note: &str,
    ) -> Result<Value, AppError> {
        if !self.reviewer || !matches!(decision, "approved" | "rejected") || note.trim().is_empty()
        {
            return Err(AppError::forbidden("reviewer required"));
        }
        let parent = self.parent_submission(id).await?;
        if parent.owner_user_id == self.actor_id {
            return Err(AppError::forbidden("self-review forbidden"));
        }
        if decision == "approved" && parent.status != "approved" {
            return Err(AppError::conflict(
                "V1_REVIEW_REQUIRED",
                "approve legacy manifest first",
            ));
        }
        let changed=sqlx::query("UPDATE extensions.marketplace_v2_submissions SET status=$2,review_note=$3,reviewed_by=$4,reviewed_at=clock_timestamp()
          WHERE submission_id=$1 AND status='pending' AND owner_user_id<>$4")
          .bind(id).bind(decision).bind(note).bind(self.actor_id).execute(&mut *self.tx).await.map_err(db_error)?;
        if changed.rows_affected() != 1 {
            return Err(AppError::conflict(
                "V2_ALREADY_DECIDED",
                "submission already decided",
            ));
        }
        self.v2_detail(id).await
    }
    pub async fn v2_withdraw(&mut self, id: Uuid, reason: &str) -> Result<Value, AppError> {
        let row = self.v2_detail(id).await?;
        if row["status"] == "pending" && row["own"] == true {
            let result=sqlx::query("UPDATE extensions.marketplace_v2_submissions SET status='withdrawn' WHERE submission_id=$1 AND status='pending' AND owner_user_id=$2")
                .bind(id).bind(self.actor_id).execute(&mut *self.tx).await.map_err(db_error)?;
            if result.rows_affected() != 1 {
                return Err(AppError::conflict(
                    "V2_ALREADY_DECIDED",
                    "submission already decided",
                ));
            }
        } else if row["status"] == "approved" && (row["own"] == true || self.reviewer) {
            sqlx::query("INSERT INTO extensions.marketplace_v2_withdrawals(submission_id,requested_by,reason) VALUES($1,$2,$3) ON CONFLICT(submission_id) DO NOTHING")
                .bind(id).bind(self.actor_id).bind(reason).execute(&mut *self.tx).await.map_err(db_error)?;
        } else {
            return Err(AppError::forbidden("withdrawal unavailable"));
        }
        self.v2_detail(id).await
    }
}

impl DeveloperTx<'_> {
    /// Caller verified two ownership signatures over exact current revision.
    pub async fn rotate_key(
        &mut self,
        publisher_id: &str,
        expected: i64,
        old: &str,
        new: &str,
        old_proof: &str,
        new_proof: &str,
    ) -> Result<PublisherRow, AppError> {
        self.lock_user().await?;
        let current = self
            .get_publisher()
            .await?
            .ok_or_else(|| AppError::not_found("publisher not found"))?;
        if current.id != publisher_id
            || current.key_revision != expected
            || current.public_key != old
            || old == new
            || expected <= 0
        {
            return Err(AppError::conflict(
                "STALE_PUBLISHER_KEY",
                "publisher key changed",
            ));
        }
        let prior:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM extensions.marketplace_publisher_rotations WHERE publisher_id=$1 AND (old_public_key=$2 OR new_public_key=$2))")
          .bind(publisher_id).bind(new).fetch_one(&mut *self.tx).await.map_err(db_error)?;
        if prior {
            return Err(AppError::conflict(
                "REUSED_PUBLISHER_KEY",
                "use a new signing key",
            ));
        }
        let revision = expected
            .checked_add(1)
            .ok_or_else(|| AppError::internal("publisher revision overflow"))?;
        sqlx::query("INSERT INTO extensions.marketplace_publisher_rotations(publisher_id,owner_user_id,revision,old_public_key,new_public_key,old_proof,new_proof) VALUES($1,$2,$3,$4,$5,$6,$7)")
          .bind(publisher_id).bind(self.actor_id).bind(revision).bind(old).bind(new).bind(old_proof).bind(new_proof)
          .execute(&mut *self.tx).await.map_err(db_error)?;
        let row=sqlx::query("UPDATE extensions.marketplace_publishers SET public_key=$1,key_revision=$2 WHERE id=$3 AND owner_user_id=$4 AND key_revision=$5 AND public_key=$6 RETURNING id,name,public_key,created_at,key_revision")
          .bind(new).bind(revision).bind(publisher_id).bind(self.actor_id).bind(expected).bind(old)
          .fetch_optional(&mut *self.tx).await.map_err(db_error)?.ok_or_else(||AppError::conflict("STALE_PUBLISHER_KEY","publisher key changed"))?;
        publisher_from_row(&row)
    }
}
