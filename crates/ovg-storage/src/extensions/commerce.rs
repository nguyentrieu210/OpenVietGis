//! Append-only sandbox commerce, isolated from production runtime permissions.
//! All calls require the existing human organization administrator proof.
use super::{db_error, OrgExtensionScope};
use base64::{engine::general_purpose::STANDARD, Engine};
use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, VerifyingKey};
use ovg_core::AppError;
use ovg_module_sdk::commerce::{verify_sandbox, OrderLedger, Quote, SandboxEvent};
use ovg_module_sdk::payment_offer::{verify_offer, PriceOffer, VerifiedOffer};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::Row;
use uuid::Uuid;
fn invalid() -> AppError {
    AppError::validation("INVALID_SANDBOX_ORDER", "invalid sandbox order")
}
fn corrupt() -> AppError {
    AppError::internal("sandbox accounting verification failed")
}
fn encode<T: serde::Serialize>(v: &T) -> Result<Value, AppError> {
    serde_json::to_value(v).map_err(|_| corrupt())
}

/// Versioned JSON in the existing immutable order column. Legacy orders
/// retain their exact Quote representation. This is not a payment session.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AcceptedOffer {
    schema: String,
    offer: PriceOffer,
    offer_digest: String,
    signature: String,
    authority_public_key: String,
}
impl AcceptedOffer {
    fn verify(&self, created: DateTime<Utc>) -> Result<(), AppError> {
        if self.schema != "openvietgis.sandbox.accepted-offer.v1" {
            return Err(corrupt());
        }
        let key_raw: [u8; 32] = STANDARD
            .decode(&self.authority_public_key)
            .map_err(|_| corrupt())?
            .try_into()
            .map_err(|_| corrupt())?;
        let key = VerifyingKey::from_bytes(&key_raw).map_err(|_| corrupt())?;
        let sig = Signature::from_slice(&STANDARD.decode(&self.signature).map_err(|_| corrupt())?)
            .map_err(|_| corrupt())?;
        let proof = verify_offer(
            self.offer.clone(),
            &sig,
            &key,
            &self.offer.audience,
            created,
        )
        .map_err(|_| corrupt())?;
        if proof.digest() != self.offer_digest {
            return Err(corrupt());
        }
        Ok(())
    }
    fn check_event(&self, event: &SandboxEvent, created: DateTime<Utc>) -> Result<(), AppError> {
        if event.occurred_at < created
            || (event.kind == ovg_module_sdk::commerce::EventKind::Capture
                && event.occurred_at >= self.offer.expires_at)
        {
            return Err(invalid());
        }
        Ok(())
    }
}
impl OrgExtensionScope<'_> {
    pub(super) async fn sandbox_admin(&mut self) -> Result<(), AppError> {
        if self.is_service {
            return Err(AppError::forbidden("human administrator required"));
        }
        let status: Option<String> =
            sqlx::query_scalar("SELECT status FROM org.organizations WHERE id=$1 FOR UPDATE")
                .bind(self.organization_id)
                .fetch_optional(&mut *self.tx)
                .await
                .map_err(db_error)?;
        let member:Option<(String,String)>=sqlx::query_as("SELECT role,status FROM org.organization_memberships WHERE organization_id=$1 AND user_id=$2 FOR UPDATE")
            .bind(self.organization_id).bind(self.actor_id).fetch_optional(&mut *self.tx).await.map_err(db_error)?;
        if status.as_deref() != Some("active")
            || !member
                .is_some_and(|(r, s)| s == "active" && matches!(r.as_str(), "owner" | "admin"))
        {
            return Err(AppError::forbidden("organization administrator required"));
        }
        Ok(())
    }
    pub async fn check_sandbox_admin(&mut self) -> Result<(), AppError> {
        self.sandbox_admin().await
    }
    pub async fn create_sandbox_order(
        &mut self,
        quote: &Quote,
        digest: &str,
    ) -> Result<Value, AppError> {
        self.sandbox_admin().await?;
        quote.validate()?;
        if quote.organization_id != self.organization_id
            || digest.len() != 64
            || !digest
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(invalid());
        }
        let q = encode(quote)?;
        let prior=sqlx::query("SELECT quote,release_digest FROM extensions.plugin_sandbox_orders WHERE organization_id=$1 AND id=$2")
            .bind(self.organization_id).bind(quote.order_id).fetch_optional(&mut *self.tx).await.map_err(db_error)?;
        if let Some(row) = prior {
            if row.try_get::<Value, _>("quote").map_err(db_error)? != q
                || row
                    .try_get::<String, _>("release_digest")
                    .map_err(db_error)?
                    != digest
            {
                return Err(AppError::conflict(
                    "SANDBOX_ORDER_CONFLICT",
                    "order ID reused",
                ));
            }
        } else {
            sqlx::query("INSERT INTO extensions.plugin_sandbox_orders(organization_id,id,actor_id,release_digest,quote) VALUES($1,$2,$3,$4,$5)")
                .bind(self.organization_id).bind(quote.order_id).bind(self.actor_id).bind(digest).bind(&q)
                .execute(&mut *self.tx).await.map_err(db_error)?;
        }
        Ok(json!({"sandbox":true,"quote":q,"release_digest":digest}))
    }
    pub async fn accept_sandbox_offer(
        &mut self,
        proof: &VerifiedOffer,
        sig: &Signature,
        key: &VerifyingKey,
        now: DateTime<Utc>,
    ) -> Result<Value, AppError> {
        self.sandbox_admin().await?;
        let offer = proof.offer();
        if offer.quote.organization_id != self.organization_id {
            return Err(invalid());
        }
        // Recheck signature after acquiring organization lock and use DB clock.
        let created: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&mut *self.tx)
            .await
            .map_err(db_error)?;
        if created < now - chrono::Duration::seconds(30) {
            return Err(invalid());
        }
        let checked = verify_offer(offer.clone(), sig, key, &offer.audience, created)?;
        if checked.digest() != proof.digest() {
            return Err(invalid());
        }
        let accepted = AcceptedOffer {
            schema: "openvietgis.sandbox.accepted-offer.v1".into(),
            offer: offer.clone(),
            offer_digest: proof.digest().into(),
            signature: STANDARD.encode(sig.to_bytes()),
            authority_public_key: STANDARD.encode(key.as_bytes()),
        };
        let value = encode(&accepted)?;
        let prior=sqlx::query("SELECT quote,release_digest,created_at FROM extensions.plugin_sandbox_orders WHERE organization_id=$1 AND id=$2")
            .bind(self.organization_id).bind(offer.quote.order_id).fetch_optional(&mut *self.tx).await.map_err(db_error)?;
        let replayed = prior.is_some();
        if let Some(row) = prior {
            if row.try_get::<Value, _>("quote").map_err(db_error)? != value
                || row
                    .try_get::<String, _>("release_digest")
                    .map_err(db_error)?
                    != offer.release_digest
            {
                return Err(AppError::conflict(
                    "SANDBOX_ORDER_CONFLICT",
                    "order ID already used",
                ));
            }
            accepted.verify(row.try_get("created_at").map_err(db_error)?)?;
        } else {
            sqlx::query("INSERT INTO extensions.plugin_sandbox_orders(organization_id,id,actor_id,release_digest,quote,created_at) VALUES($1,$2,$3,$4,$5,$6)")
                .bind(self.organization_id).bind(offer.quote.order_id).bind(self.actor_id).bind(&offer.release_digest).bind(value).bind(created)
                .execute(&mut *self.tx).await.map_err(db_error)?;
        }
        Ok(
            json!({"sandbox":true,"checkout_created":true,"provider_session_created":false,"replayed":replayed,"order_id":offer.quote.order_id,"offer_digest":proof.digest(),"expires_at":offer.expires_at}),
        )
    }
    async fn replay_sandbox(
        &mut self,
        id: Uuid,
        key: &VerifyingKey,
        now: DateTime<Utc>,
    ) -> Result<
        (
            OrderLedger,
            String,
            Vec<Value>,
            Option<(AcceptedOffer, DateTime<Utc>)>,
        ),
        AppError,
    > {
        let row=sqlx::query("SELECT quote,release_digest,created_at FROM extensions.plugin_sandbox_orders WHERE organization_id=$1 AND id=$2")
            .bind(self.organization_id).bind(id).fetch_optional(&mut *self.tx).await.map_err(db_error)?
            .ok_or_else(||AppError::not_found("sandbox order not found"))?;
        let stored: Value = row.try_get("quote").map_err(db_error)?;
        let digest: String = row.try_get("release_digest").map_err(db_error)?;
        let created: DateTime<Utc> = row.try_get("created_at").map_err(db_error)?;
        let (quote, accepted) = if stored.get("schema").is_some() {
            let a: AcceptedOffer = serde_json::from_value(stored).map_err(|_| corrupt())?;
            a.verify(created)?;
            if a.offer.release_digest != digest {
                return Err(corrupt());
            }
            (a.offer.quote.clone(), Some((a, created)))
        } else {
            (serde_json::from_value(stored).map_err(|_| corrupt())?, None)
        };
        if quote.order_id != id || quote.organization_id != self.organization_id {
            return Err(corrupt());
        }
        let mut ledger = OrderLedger::new(quote)?;
        let rows=sqlx::query("SELECT payload,signature,receipt,provider_event_id,recorded_at FROM extensions.plugin_sandbox_events WHERE organization_id=$1 AND order_id=$2 ORDER BY sequence LIMIT 1025")
            .bind(self.organization_id).bind(id).fetch_all(&mut *self.tx).await.map_err(db_error)?;
        if rows.len() > 1024 {
            return Err(corrupt());
        }
        let mut receipts = Vec::new();
        for row in rows {
            let event: SandboxEvent =
                serde_json::from_value(row.try_get("payload").map_err(db_error)?)
                    .map_err(|_| corrupt())?;
            let event_id: String = row.try_get("provider_event_id").map_err(db_error)?;
            if event.provider_event_id != event_id {
                return Err(corrupt());
            }
            let sig: String = row.try_get("signature").map_err(db_error)?;
            let recorded: DateTime<Utc> = row.try_get("recorded_at").map_err(db_error)?;
            let sig = Signature::from_slice(&STANDARD.decode(sig).map_err(|_| corrupt())?)
                .map_err(|_| corrupt())?;
            let proof =
                verify_sandbox(event, key, &sig, recorded.min(now)).map_err(|_| corrupt())?;
            if let Some((a, t)) = &accepted {
                a.check_event(proof.event(), *t).map_err(|_| corrupt())?;
            }
            let calculated = encode(&ledger.apply(&proof).map_err(|_| corrupt())?)?;
            let receipt: Value = row.try_get("receipt").map_err(db_error)?;
            if receipt != calculated {
                return Err(corrupt());
            }
            receipts.push(receipt);
        }
        Ok((ledger, digest, receipts, accepted))
    }
    pub async fn sandbox_snapshot(
        &mut self,
        id: Uuid,
        key: &VerifyingKey,
        now: DateTime<Utc>,
    ) -> Result<Value, AppError> {
        self.sandbox_admin().await?;
        let (ledger, digest, receipts, accepted) = self.replay_sandbox(id, key, now).await?;
        Ok(
            json!({"sandbox":true,"order_id":id,"release_digest":digest,"entitlement":ledger.entitlement(now),"outstanding_minor":ledger.outstanding_minor(),"event_count":ledger.event_count(),"receipts":receipts,"checkout_offer_digest":accepted.as_ref().map(|(a,_)|&a.offer_digest),"checkout_expires_at":accepted.as_ref().map(|(a,_)|a.offer.expires_at)}),
        )
    }
    pub async fn sandbox_settlement(
        &mut self,
        id: Uuid,
        key: &VerifyingKey,
        now: DateTime<Utc>,
    ) -> Result<ovg_module_sdk::settlement::Settlement, AppError> {
        self.sandbox_admin().await?;
        let (ledger, _, receipts, _) = self.replay_sandbox(id, key, now).await?;
        let receipts = receipts
            .into_iter()
            .map(serde_json::from_value)
            .collect::<Result<Vec<ovg_module_sdk::commerce::LedgerReceipt>, _>>()
            .map_err(|_| corrupt())?;
        let report =
            ovg_module_sdk::settlement::Settlement::from_receipts(ledger.currency(), &receipts)?;
        if report.clearing_minor != ledger.outstanding_minor() {
            return Err(corrupt());
        }
        Ok(report)
    }
    pub async fn record_sandbox_event(
        &mut self,
        event: SandboxEvent,
        signature: &Signature,
        key: &VerifyingKey,
        now: DateTime<Utc>,
    ) -> Result<Value, AppError> {
        self.sandbox_admin().await?;
        if event.organization_id != self.organization_id {
            return Err(invalid());
        }
        let proof = verify_sandbox(event, key, signature, now)?;
        let e = proof.event();
        let (mut ledger, digest, _, accepted) = self.replay_sandbox(e.order_id, key, now).await?;
        if let Some((a, t)) = &accepted {
            a.check_event(e, *t)?;
        }
        let existing=sqlx::query("SELECT order_id,payload,receipt FROM extensions.plugin_sandbox_events WHERE organization_id=$1 AND provider_event_id=$2")
            .bind(self.organization_id).bind(&e.provider_event_id).fetch_optional(&mut *self.tx).await.map_err(db_error)?;
        let payload = encode(e)?;
        let receipt = encode(&ledger.apply(&proof)?)?;
        let replayed = existing.is_some();
        if let Some(row) = existing {
            if row.try_get::<Uuid, _>("order_id").map_err(db_error)? != e.order_id
                || row.try_get::<Value, _>("payload").map_err(db_error)? != payload
                || row.try_get::<Value, _>("receipt").map_err(db_error)? != receipt
            {
                return Err(AppError::conflict(
                    "PAYMENT_EVENT_CONFLICT",
                    "event ID reused",
                ));
            }
        } else {
            sqlx::query("INSERT INTO extensions.plugin_sandbox_events(organization_id,order_id,provider_event_id,payload,signature,receipt) VALUES($1,$2,$3,$4,$5,$6)")
                .bind(self.organization_id).bind(e.order_id).bind(&e.provider_event_id).bind(payload).bind(STANDARD.encode(signature.to_bytes())).bind(&receipt)
                .execute(&mut *self.tx).await.map_err(db_error)?;
        }
        Ok(
            json!({"sandbox":true,"replayed":replayed,"order_id":e.order_id,"release_digest":digest,"receipt":receipt,"entitlement":ledger.entitlement(now),"outstanding_minor":ledger.outstanding_minor()}),
        )
    }
}
