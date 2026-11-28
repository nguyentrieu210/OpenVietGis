//! Durable project-local invalidation replay. Events carry no permission or business payload.
use crate::catalog::ProjectRead;
use ovg_core::AppError;
use serde_json::{json, Value};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModuleCursor { pub epoch: Uuid, pub sequence: i64 }
impl ModuleCursor {
    pub fn parse(raw: &str) -> Result<Self, AppError> {
        let (epoch, sequence) = raw.split_once(':').ok_or_else(AppError::invalid_cursor)?;
        let epoch = Uuid::parse_str(epoch).map_err(|_| AppError::invalid_cursor())?;
        let sequence = sequence.parse::<i64>().map_err(|_| AppError::invalid_cursor())?;
        let cursor = Self { epoch, sequence };
        if sequence < 0 || (epoch.is_nil() && sequence != 0) || cursor.encode() != raw {
            return Err(AppError::invalid_cursor());
        }
        Ok(cursor)
    }
    pub fn encode(self) -> String { format!("{}:{}", self.epoch, self.sequence) }
}

// Read head and bounded event page from one statement snapshot. Never use a
// separate max(sequence) query: a concurrent transaction could be skipped.
pub async fn replay(read: &mut ProjectRead<'_>, cursor: Option<ModuleCursor>, limit: i64) -> Result<Value, AppError> {
    if !(1..=200).contains(&limit) { return Err(AppError::bad_request("limit must be 1..200")); }
    let cursor_epoch = cursor.map(|c|c.epoch);
    let cursor_seq = cursor.map(|c|c.sequence).unwrap_or(0);
    let result: Value = sqlx::query_scalar(
        "WITH stream AS (
           SELECT epoch,head FROM extensions.module_change_streams WHERE organization_id=$1 AND project_id=$2
         ), pinned AS (
           SELECT coalesce((SELECT epoch FROM stream),'00000000-0000-0000-0000-000000000000'::uuid) AS epoch,
                  coalesce((SELECT head FROM stream),0) AS head
         ), page AS (
           SELECT sequence,extension_key,kind FROM extensions.module_change_events,pinned
           WHERE organization_id=$1 AND project_id=$2 AND pinned.epoch=$3 AND sequence>$4 AND sequence<=head
           ORDER BY sequence LIMIT $5
         )
         SELECT jsonb_build_object('epoch',epoch,'head',head::text,
           'items',coalesce((SELECT jsonb_agg(jsonb_build_object('sequence',sequence::text,'extension_key',extension_key,'kind',kind) ORDER BY sequence) FROM page),'[]'::jsonb))
         FROM pinned"
    ).bind(read.organization_id()).bind(read.project_id()).bind(cursor_epoch).bind(cursor_seq).bind(limit)
     .fetch_one(&mut **read.transaction()).await.map_err(super::db_error)?;
    let epoch = result["epoch"].as_str().and_then(|v|Uuid::parse_str(v).ok()).ok_or_else(||AppError::internal("invalid module event epoch"))?;
    let head = result["head"].as_str().and_then(|v|v.parse::<i64>().ok()).ok_or_else(||AppError::internal("invalid module event head"))?;
    let snapshot = || json!({"mode":"snapshot_required","cursor":ModuleCursor{epoch,sequence:head}.encode(),"items":[],"has_more":false});
    let Some(cursor)=cursor else{return Ok(snapshot());};
    if cursor.epoch!=epoch || cursor.sequence>head { return Ok(snapshot()); }
    let items = result["items"].as_array().ok_or_else(||AppError::internal("invalid event page"))?;
    let mut last=cursor.sequence;
    for item in items {
        let seq=item["sequence"].as_str().and_then(|v|v.parse::<i64>().ok());
        if seq!=last.checked_add(1) { return Ok(snapshot()); }
        last=seq.ok_or_else(||AppError::internal("invalid sequence"))?;
    }
    if items.is_empty() && last<head {return Ok(snapshot());}
    Ok(json!({"mode":"delta","cursor":ModuleCursor{epoch,sequence:last}.encode(),"items":items,"has_more":last<head}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cursor_roundtrip_and_rejects_noncanonical() {
        let c=ModuleCursor{epoch:Uuid::new_v4(),sequence:42};
        assert_eq!(ModuleCursor::parse(&c.encode()).unwrap(),c);
        for raw in ["", "bad:1", "00000000-0000-0000-0000-000000000000:1"] {assert!(ModuleCursor::parse(raw).is_err());}
        for seq in ["-1","01","+1","9223372036854775808"] {assert!(ModuleCursor::parse(&format!("{}:{seq}",c.epoch)).is_err());}
    }
}
