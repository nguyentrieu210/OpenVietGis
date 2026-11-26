//! Organization chat (`org_chat`): rooms, messages, image attachments, reactions,
//! pins, people search and contacts for one verified human member.

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use ovg_core::AppError;
use serde_json::{json, Map, Value};
use sqlx::postgres::PgRow;
use base64::alphabet;
use base64::engine::{GeneralPurpose, GeneralPurposeConfig};
use base64::Engine;
use sqlx::Row;
use unicode_normalization::char::canonical_combining_class;
use unicode_normalization::UnicodeNormalization;
use uuid::Uuid;

use super::TenantTx;
use crate::editing::team::row_json;
use crate::editing::EditError;

const T: &str = "collaboration.";
const MAX_ROOM_MEMBERS: usize = 50;
const MAX_PINS: i64 = 20;
const MEMBERS_SQL: &str = "SELECT u.id, u.display_name FROM org.organization_memberships m
  JOIN identity.users u ON u.id=m.user_id AND u.status='active'
  WHERE m.organization_id=$1 AND m.status='active'";
const MESSAGE_SQL: &str = "SELECT m.id,m.author_id,m.author_name,m.body,m.created_at,m.mentions,
  CASE WHEN q.id IS NULL THEN NULL ELSE json_build_object('id',q.id,'author_name',q.author_name,
    'body',left(q.body,140)) END AS reply_to,
  EXISTS(SELECT 1 FROM collaboration.chat_pins p WHERE p.organization_id=m.organization_id
    AND p.message_id=m.id) AS pinned,
  COALESCE((SELECT json_agg(json_build_object('id',a.id,'kind','image','width',a.width,
    'height',a.height) ORDER BY a.position) FROM collaboration.chat_attachments a
    WHERE a.organization_id=m.organization_id AND a.message_id=m.id),'[]') AS attachments
  FROM collaboration.chat_messages m LEFT JOIN collaboration.chat_messages q
    ON q.organization_id=m.organization_id AND q.id=m.reply_to_id
  WHERE m.organization_id=$1";

/// One image attachment as submitted: a data URL and its declared size.
/// A validated `ChatMessageRequest`.
pub struct NewChatMessage {
    pub body: String,
    pub reply_to: Option<Uuid>,
    pub mention_ids: Vec<Uuid>,
    pub attachments: Vec<Attachment>,
}

pub struct Attachment {
    pub data_url: String,
    pub width: i32,
    pub height: i32,
}

const MAX_ATTACHMENT_BYTES: usize = 400 * 1024;

/// Python `decode_image`: a JPEG, PNG or WebP data URL whose bytes match its type.
pub fn decode_image(data_url: &str) -> Result<(String, Vec<u8>), EditError> {
    let not_image = || EditError::detail(422, "Attachments must be JPEG, PNG or WebP data URLs");
    let rest = data_url.strip_prefix("data:image/").ok_or_else(not_image)?;
    let (kind, payload) = rest.split_once(";base64,").ok_or_else(not_image)?;
    let allowed = |c: u8| c.is_ascii_alphanumeric() || matches!(c, b'+' | b'/' | b'=');
    if !matches!(kind, "jpeg" | "png" | "webp")
        || payload.is_empty()
        || !payload.bytes().all(allowed)
    {
        return Err(not_image());
    }
    let engine = GeneralPurpose::new(
        &alphabet::STANDARD,
        GeneralPurposeConfig::new().with_decode_allow_trailing_bits(true),
    );
    let raw = engine
        .decode(payload)
        .map_err(|_| EditError::detail(422, "Attachment is not valid base64"))?;
    let magic = match kind {
        "jpeg" => raw.starts_with(&[0xff, 0xd8, 0xff]),
        "png" => raw.starts_with(b"\x89PNG\r\n\x1a\n"),
        _ => raw.starts_with(b"RIFF") && raw.get(8..12) == Some(b"WEBP".as_slice()),
    };
    if raw.is_empty() || raw.len() > MAX_ATTACHMENT_BYTES || !magic {
        return Err(EditError::detail(
            422,
            "Attachment content does not match its image type or is too big",
        ));
    }
    Ok((format!("image/{kind}"), raw))
}

/// Python `org_chat.fold`: casefold, `đ` to `d`, then strip combining marks.
pub fn fold(value: &str) -> String {
    value
        .to_lowercase()
        .replace('đ', "d")
        .nfd()
        .filter(|c| canonical_combining_class(*c) == 0)
        .collect()
}

fn app(error: AppError) -> EditError {
    match error {
        AppError::Internal { internal_detail } => EditError::Internal(internal_detail),
        error => EditError::detail(error.status_code(), error.to_string()),
    }
}

fn db(error: sqlx::Error) -> EditError {
    EditError::Internal(format!("chat database operation failed: {error}"))
}

fn iso(time: DateTime<Utc>) -> String {
    crate::collaboration::isoformat(time)
}

fn rows_json(rows: &[PgRow]) -> Result<Vec<Value>, EditError> {
    rows.iter().map(row_json).collect()
}

impl TenantTx {
    /// `org_scope`: an active member (404 otherwise), then bind the tenant.
    pub async fn chat_scope(&mut self, organization_id: Uuid) -> Result<(), AppError> {
        self.bind(None).await?;
        self.active_membership(organization_id)
            .await?
            .ok_or_else(|| AppError::not_found("organization not found"))?;
        self.bind(Some(organization_id)).await
    }

    fn me(&self) -> Uuid {
        self.actor_id
    }

    fn name_200(&self) -> String {
        self.display_name.chars().take(200).collect()
    }

    async fn chat_audit(
        &mut self,
        organization: Uuid,
        resource: (&str, Uuid),
        action: &str,
        metadata: Value,
    ) -> Result<(), EditError> {
        let (resource_type, id) = (resource.0, Some(resource.1));
        self.audit(organization, action, resource_type, id, "success", metadata)
            .await
            .map_err(app)
    }

    async fn room_audit(
        &mut self,
        organization: Uuid,
        room: Uuid,
        action: &str,
        metadata: Value,
    ) -> Result<(), EditError> {
        self.chat_audit(organization, ("chat_room", room), action, metadata)
            .await
    }

    async fn contact_audit(
        &mut self,
        organization: Uuid,
        id: Uuid,
        action: &str,
        metadata: Value,
    ) -> Result<(), EditError> {
        self.chat_audit(organization, ("chat_contact", id), action, metadata)
            .await
    }

    /// `people`: active organization members by ID.
    async fn people(&mut self, organization: Uuid) -> Result<HashMap<Uuid, String>, EditError> {
        let rows: Vec<(Uuid, String)> = sqlx::query_as(MEMBERS_SQL)
            .bind(organization)
            .fetch_all(&mut *self.tx)
            .await
            .map_err(db)?;
        Ok(rows.into_iter().collect())
    }

    async fn org_room(&mut self, organization: Uuid) -> Result<Uuid, EditError> {
        // Lazy and idempotent; the partial unique index serialises creators.
        sqlx::query(&format!(
            "INSERT INTO {T}chat_rooms(organization_id,id,kind) VALUES($1,$2,'org')
             ON CONFLICT (organization_id) WHERE kind='org' DO NOTHING"
        ))
        .bind(organization)
        .bind(Uuid::new_v4())
        .execute(&mut *self.tx)
        .await
        .map_err(db)?;
        sqlx::query_scalar(&format!(
            "SELECT id FROM {T}chat_rooms WHERE organization_id=$1 AND kind='org'"
        ))
        .bind(organization)
        .fetch_one(&mut *self.tx)
        .await
        .map_err(db)
    }

    async fn member_role(
        &mut self,
        organization: Uuid,
        room: Uuid,
        user: Uuid,
    ) -> Result<Option<String>, EditError> {
        sqlx::query_scalar(&format!(
            "SELECT role FROM {T}chat_room_members
             WHERE organization_id=$1 AND room_id=$2 AND user_id=$3"
        ))
        .bind(organization)
        .bind(room)
        .bind(user)
        .fetch_optional(&mut *self.tx)
        .await
        .map_err(db)
    }

    /// `room_access`: the room kind and this member's role (404 when hidden).
    async fn room_access(
        &mut self,
        organization: Uuid,
        room: Uuid,
    ) -> Result<(String, Option<String>), EditError> {
        let kind: Option<String> = sqlx::query_scalar(&format!(
            "SELECT kind FROM {T}chat_rooms WHERE organization_id=$1 AND id=$2"
        ))
        .bind(organization)
        .bind(room)
        .fetch_optional(&mut *self.tx)
        .await
        .map_err(db)?;
        let Some(kind) = kind else {
            return Err(EditError::detail(404, "Room not found"));
        };
        let role = self.member_role(organization, room, self.me()).await?;
        if kind != "org" && role.is_none() {
            return Err(EditError::detail(404, "Room not found"));
        }
        Ok((kind, role))
    }

    async fn list_rooms(
        &mut self,
        organization: Uuid,
        room: Option<Uuid>,
    ) -> Result<Vec<Value>, EditError> {
        let rows = sqlx::query(&format!(
            "SELECT r.id, r.kind, r.name,
               CASE WHEN r.kind='org' THEN (SELECT count(*) FROM ({MEMBERS_SQL}) a)
                 ELSE (SELECT count(*) FROM {T}chat_room_members c
                   WHERE c.organization_id=r.organization_id AND c.room_id=r.id) END
                 AS member_count,
               (SELECT count(*) FROM {T}chat_messages x WHERE x.organization_id=r.organization_id
                 AND x.room_id=r.id AND x.author_id<>$2 AND x.created_at>COALESCE(
                   rm.last_read_at, rm.joined_at, '-infinity')) AS unread,
               EXISTS(SELECT 1 FROM {T}chat_messages x WHERE x.organization_id=r.organization_id
                 AND x.room_id=r.id AND x.author_id<>$2 AND x.created_at>COALESCE(
                   rm.last_read_at, rm.joined_at, '-infinity')
                 AND x.mentions @> jsonb_build_array(jsonb_build_object('id', CAST($2 AS text))))
                 AS mentioned,
               lm.id AS lm_id, lm.body AS lm_body, lm.author_name AS lm_author,
               lm.created_at AS lm_at, p.id AS peer_id, p.display_name AS peer_name,
               EXISTS(SELECT 1 FROM identity.user_avatars av WHERE av.user_id=p.id)
                 AS peer_avatar
             FROM {T}chat_rooms r
             LEFT JOIN {T}chat_room_members rm ON rm.organization_id=r.organization_id
               AND rm.room_id=r.id AND rm.user_id=$2
             LEFT JOIN LATERAL (SELECT id, body, author_name, created_at FROM {T}chat_messages x
               WHERE x.organization_id=r.organization_id AND x.room_id=r.id
               ORDER BY x.created_at DESC, x.id DESC LIMIT 1) lm ON true
             LEFT JOIN identity.users p ON r.kind='direct'
               AND p.id=CASE WHEN r.direct_low=$2 THEN r.direct_high ELSE r.direct_low END
             WHERE r.organization_id=$1 AND (r.kind='org' OR rm.user_id IS NOT NULL)
               AND ($3::uuid IS NULL OR r.id=$3)
             ORDER BY COALESCE(lm.created_at, r.created_at) DESC, r.id"
        ))
        .bind(organization)
        .bind(self.me())
        .bind(room)
        .fetch_all(&mut *self.tx)
        .await
        .map_err(db)?;
        rows.iter().map(room_wire).collect()
    }

    async fn one_room(&mut self, organization: Uuid, room: Uuid) -> Result<Value, EditError> {
        let mut rooms = self.list_rooms(organization, Some(room)).await?;
        if rooms.is_empty() {
            return Err(EditError::Internal("room vanished".into()));
        }
        Ok(rooms.swap_remove(0))
    }

    pub async fn chat_rooms_list(&mut self, organization: Uuid) -> Result<Value, EditError> {
        self.org_room(organization).await?;
        Ok(json!({ "items": self.list_rooms(organization, None).await? }))
    }

    pub async fn create_chat_room(
        &mut self,
        organization: Uuid,
        name: &str,
        member_ids: &[Uuid],
    ) -> Result<Value, EditError> {
        let me = self.me();
        let mut ids: Vec<Uuid> = Vec::new();
        for id in member_ids {
            if *id != me && !ids.contains(id) {
                ids.push(*id);
            }
        }
        let known = self.people(organization).await?;
        if ids.is_empty() || ids.iter().any(|id| !known.contains_key(id)) {
            return Err(EditError::detail(
                422,
                "Members must be other active organisation members",
            ));
        }
        let room = Uuid::new_v4();
        sqlx::query(&format!(
            "INSERT INTO {T}chat_rooms(organization_id,id,kind,name,created_by)
             VALUES($1,$2,'group',$3,$4)"
        ))
        .bind(organization)
        .bind(room)
        .bind(name)
        .bind(me)
        .execute(&mut *self.tx)
        .await
        .map_err(db)?;
        let members = std::iter::once((me, "admin")).chain(ids.iter().map(|id| (*id, "member")));
        for (user, role) in members {
            sqlx::query(&format!(
                "INSERT INTO {T}chat_room_members(organization_id,room_id,user_id,role)
                 VALUES($1,$2,$3,$4)"
            ))
            .bind(organization)
            .bind(room)
            .bind(user)
            .bind(role)
            .execute(&mut *self.tx)
            .await
            .map_err(db)?;
        }
        let metadata = json!({ "members": ids.len() + 1 });
        self.room_audit(organization, room, "chat.room.create", metadata)
            .await?;
        self.one_room(organization, room).await
    }

    pub async fn direct_room(
        &mut self,
        organization: Uuid,
        user: Uuid,
    ) -> Result<Value, EditError> {
        let me = self.me();
        if user == me || !self.people(organization).await?.contains_key(&user) {
            return Err(EditError::detail(404, "Person not found"));
        }
        let (low, high) = (me.min(user), me.max(user));
        sqlx::query(&format!(
            "INSERT INTO {T}chat_rooms(organization_id,id,kind,direct_low,direct_high,created_by)
             VALUES($1,$2,'direct',$3,$4,$5)
             ON CONFLICT (organization_id,direct_low,direct_high) WHERE kind='direct' DO NOTHING"
        ))
        .bind(organization)
        .bind(Uuid::new_v4())
        .bind(low)
        .bind(high)
        .bind(me)
        .execute(&mut *self.tx)
        .await
        .map_err(db)?;
        let room: Uuid = sqlx::query_scalar(&format!(
            "SELECT id FROM {T}chat_rooms WHERE organization_id=$1 AND kind='direct'
               AND direct_low=$2 AND direct_high=$3"
        ))
        .bind(organization)
        .bind(low)
        .bind(high)
        .fetch_one(&mut *self.tx)
        .await
        .map_err(db)?;
        for member in [low, high] {
            sqlx::query(&format!(
                "INSERT INTO {T}chat_room_members(organization_id,room_id,user_id)
                 VALUES($1,$2,$3) ON CONFLICT DO NOTHING"
            ))
            .bind(organization)
            .bind(room)
            .bind(member)
            .execute(&mut *self.tx)
            .await
            .map_err(db)?;
        }
        self.one_room(organization, room).await
    }

    pub async fn rename_chat_room(
        &mut self,
        organization: Uuid,
        room: Uuid,
        name: &str,
    ) -> Result<Value, EditError> {
        let (kind, role) = self.room_access(organization, room).await?;
        if kind != "group" {
            return Err(EditError::detail(422, "Only group rooms can be renamed"));
        }
        if role.as_deref() != Some("admin") {
            return Err(EditError::detail(403, "Room admin required"));
        }
        sqlx::query(&format!(
            "UPDATE {T}chat_rooms SET name=$3 WHERE organization_id=$1 AND id=$2"
        ))
        .bind(organization)
        .bind(room)
        .bind(name)
        .execute(&mut *self.tx)
        .await
        .map_err(db)?;
        self.room_audit(organization, room, "chat.room.rename", json!({}))
            .await?;
        self.one_room(organization, room).await
    }

    pub async fn invite_chat_members(
        &mut self,
        organization: Uuid,
        room: Uuid,
        user_ids: &[Uuid],
    ) -> Result<Value, EditError> {
        let (kind, _) = self.room_access(organization, room).await?;
        if kind != "group" {
            return Err(EditError::detail(422, "Only group rooms accept members"));
        }
        let known = self.people(organization).await?;
        let mut ids: Vec<Uuid> = Vec::new();
        for id in user_ids {
            if !ids.contains(id) {
                ids.push(*id);
            }
        }
        if ids.iter().any(|id| !known.contains_key(id)) {
            return Err(EditError::detail(
                422,
                "Members must be active organisation members",
            ));
        }
        // Serialise invites so concurrent requests cannot exceed the member cap.
        sqlx::query(&format!(
            "SELECT 1 FROM {T}chat_rooms WHERE organization_id=$1 AND id=$2 FOR UPDATE"
        ))
        .bind(organization)
        .bind(room)
        .execute(&mut *self.tx)
        .await
        .map_err(db)?;
        let current: HashSet<Uuid> = sqlx::query_scalar::<_, Uuid>(&format!(
            "SELECT user_id FROM {T}chat_room_members WHERE organization_id=$1 AND room_id=$2"
        ))
        .bind(organization)
        .bind(room)
        .fetch_all(&mut *self.tx)
        .await
        .map_err(db)?
        .into_iter()
        .collect();
        let added: Vec<Uuid> = ids.into_iter().filter(|id| !current.contains(id)).collect();
        if current.len() + added.len() > MAX_ROOM_MEMBERS {
            return Err(EditError::detail(
                422,
                format!("A room holds at most {MAX_ROOM_MEMBERS} members"),
            ));
        }
        for user in &added {
            sqlx::query(&format!(
                "INSERT INTO {T}chat_room_members(organization_id,room_id,user_id)
                 VALUES($1,$2,$3) ON CONFLICT DO NOTHING"
            ))
            .bind(organization)
            .bind(room)
            .bind(user)
            .execute(&mut *self.tx)
            .await
            .map_err(db)?;
        }
        let metadata = json!({ "added": added });
        self.room_audit(organization, room, "chat.room.members.add", metadata)
            .await?;
        self.one_room(organization, room).await
    }

    pub async fn remove_chat_member(
        &mut self,
        organization: Uuid,
        room: Uuid,
        user: Uuid,
    ) -> Result<(), EditError> {
        let (kind, role) = self.room_access(organization, room).await?;
        if kind != "group" {
            return Err(EditError::detail(
                422,
                "Only group rooms have removable members",
            ));
        }
        if user != self.me() && role.as_deref() != Some("admin") {
            return Err(EditError::detail(403, "Room admin required"));
        }
        let removed: Option<Uuid> = sqlx::query_scalar(&format!(
            "DELETE FROM {T}chat_room_members WHERE organization_id=$1 AND room_id=$2
               AND user_id=$3 RETURNING user_id"
        ))
        .bind(organization)
        .bind(room)
        .bind(user)
        .fetch_optional(&mut *self.tx)
        .await
        .map_err(db)?;
        if removed.is_none() {
            return Err(EditError::detail(404, "Member not found"));
        }
        // Never leave a group without an admin: promote the longest-standing member.
        sqlx::query(&format!(
            "UPDATE {T}chat_room_members SET role='admin' WHERE organization_id=$1
               AND room_id=$2 AND user_id=(SELECT user_id FROM {T}chat_room_members
                 WHERE organization_id=$1 AND room_id=$2 ORDER BY joined_at, user_id LIMIT 1)
               AND NOT EXISTS(SELECT 1 FROM {T}chat_room_members WHERE organization_id=$1
                 AND room_id=$2 AND role='admin')"
        ))
        .bind(organization)
        .bind(room)
        .execute(&mut *self.tx)
        .await
        .map_err(db)?;
        let metadata = json!({ "user_id": user });
        self.room_audit(organization, room, "chat.room.members.remove", metadata)
            .await
    }

    async fn reactions(
        &mut self,
        organization: Uuid,
        ids: &[Uuid],
    ) -> Result<HashMap<Uuid, Vec<Value>>, EditError> {
        let mut out: HashMap<Uuid, Vec<Value>> = ids.iter().map(|id| (*id, Vec::new())).collect();
        if ids.is_empty() {
            return Ok(out);
        }
        let rows = sqlx::query_as::<_, (Uuid, String, i64, Option<bool>)>(&format!(
            "SELECT message_id, emoji, count(*) AS count, bool_or(user_id=$3) AS mine
             FROM {T}chat_reactions WHERE organization_id=$1 AND message_id=ANY($2)
             GROUP BY message_id, emoji ORDER BY min(created_at), emoji"
        ))
        .bind(organization)
        .bind(ids)
        .bind(self.me())
        .fetch_all(&mut *self.tx)
        .await
        .map_err(db)?;
        for (message, emoji, count, mine) in rows {
            if let Some(list) = out.get_mut(&message) {
                list.push(json!({"emoji": emoji, "count": count, "mine": mine}));
            }
        }
        Ok(out)
    }

    /// `shape`: each message row plus its reaction summary.
    async fn shape(
        &mut self,
        organization: Uuid,
        rows: Vec<Value>,
    ) -> Result<Vec<Value>, EditError> {
        let ids: Vec<Uuid> = rows
            .iter()
            .filter_map(|row| row["id"].as_str().and_then(|id| Uuid::parse_str(id).ok()))
            .collect();
        let mut found = self.reactions(organization, &ids).await?;
        Ok(rows
            .into_iter()
            .zip(ids)
            .map(|(mut row, id)| {
                row["reactions"] = Value::Array(found.remove(&id).unwrap_or_default());
                row
            })
            .collect())
    }

    async fn message_room(
        &mut self,
        organization: Uuid,
        message: Uuid,
    ) -> Result<(Uuid, Uuid, String, Option<String>), EditError> {
        let (room, author) = sqlx::query_as::<_, (Uuid, Uuid)>(&format!(
            "SELECT room_id, author_id FROM {T}chat_messages WHERE organization_id=$1 AND id=$2"
        ))
        .bind(organization)
        .bind(message)
        .fetch_optional(&mut *self.tx)
        .await
        .map_err(db)?
        .ok_or_else(|| EditError::detail(404, "Message not found"))?;
        let (kind, role) = self.room_access(organization, room).await?;
        Ok((room, author, kind, role))
    }

    pub async fn chat_messages(
        &mut self,
        organization: Uuid,
        room: Uuid,
        limit: i64,
        before: Option<Uuid>,
    ) -> Result<Value, EditError> {
        self.room_access(organization, room).await?;
        let cursor = if before.is_some() {
            " AND (m.created_at,m.id)<(SELECT b.created_at,b.id FROM collaboration.chat_messages b
               WHERE b.organization_id=$1 AND b.room_id=$2 AND b.id=$3)"
        } else {
            " AND $3::uuid IS NULL"
        };
        let mut rows = sqlx::query(&format!(
            "{MESSAGE_SQL} AND m.room_id=$2{cursor} ORDER BY m.created_at DESC, m.id DESC LIMIT $4"
        ))
        .bind(organization)
        .bind(room)
        .bind(before)
        .bind(limit)
        .fetch_all(&mut *self.tx)
        .await
        .map_err(db)?;
        rows.reverse();
        let items = self.shape(organization, rows_json(&rows)?).await?;
        Ok(json!({ "items": items }))
    }

    pub async fn post_chat_message(
        &mut self,
        organization: Uuid,
        room: Uuid,
        message: NewChatMessage,
    ) -> Result<Value, EditError> {
        let NewChatMessage {
            body,
            reply_to,
            mention_ids,
            attachments,
        } = message;
        let (kind, _) = self.room_access(organization, room).await?;
        if let Some(reply_to) = reply_to {
            let found: Option<i32> = sqlx::query_scalar(&format!(
                "SELECT 1 FROM {T}chat_messages WHERE organization_id=$1 AND room_id=$2 AND id=$3"
            ))
            .bind(organization)
            .bind(room)
            .bind(reply_to)
            .fetch_optional(&mut *self.tx)
            .await
            .map_err(db)?;
            if found.is_none() {
                return Err(EditError::detail(
                    422,
                    "Reply target must be a message in this room",
                ));
            }
        }
        let mut known = self.people(organization).await?;
        if kind != "org" {
            let members: HashSet<Uuid> = sqlx::query_scalar::<_, Uuid>(&format!(
                "SELECT user_id FROM {T}chat_room_members WHERE organization_id=$1 AND room_id=$2"
            ))
            .bind(organization)
            .bind(room)
            .fetch_all(&mut *self.tx)
            .await
            .map_err(db)?
            .into_iter()
            .collect();
            known.retain(|id, _| members.contains(id));
        }
        let mut mentions = Vec::new();
        let mut seen = HashSet::new();
        for id in &mention_ids {
            if !seen.insert(*id) {
                continue;
            }
            let Some(name) = known.get(id) else {
                return Err(EditError::detail(
                    422,
                    "Mentions must target members of this room",
                ));
            };
            mentions.push(json!({"id": id, "display_name": name}));
        }
        let images = attachments
            .into_iter()
            .map(|a| decode_image(&a.data_url).map(|image| (image, a.width, a.height)))
            .collect::<Result<Vec<_>, _>>()?;
        let id = Uuid::new_v4();
        sqlx::query(&format!(
            "INSERT INTO {T}chat_messages(organization_id,room_id,id,author_id,author_name,body,
               reply_to_id,mentions)
             VALUES($1,$2,$3,$4,$5,$6,$7,$8)"
        ))
        .bind(organization)
        .bind(room)
        .bind(id)
        .bind(self.me())
        .bind(self.name_200())
        .bind(body)
        .bind(reply_to)
        .bind(Value::Array(mentions))
        .execute(&mut *self.tx)
        .await
        .map_err(db)?;
        for (position, ((content_type, data), width, height)) in images.into_iter().enumerate() {
            sqlx::query(&format!(
                "INSERT INTO {T}chat_attachments(organization_id,id,message_id,position,
                   content_type,width,height,data) VALUES($1,$2,$3,$4,$5,$6,$7,$8)"
            ))
            .bind(organization)
            .bind(Uuid::new_v4())
            .bind(id)
            .bind(position as i16)
            .bind(content_type)
            .bind(width)
            .bind(height)
            .bind(data)
            .execute(&mut *self.tx)
            .await
            .map_err(db)?;
        }
        sqlx::query(&format!(
            "UPDATE {T}chat_rooms SET last_activity_at=now() WHERE organization_id=$1 AND id=$2"
        ))
        .bind(organization)
        .bind(room)
        .execute(&mut *self.tx)
        .await
        .map_err(db)?;
        let row = sqlx::query(&format!("{MESSAGE_SQL} AND m.id=$2"))
            .bind(organization)
            .bind(id)
            .fetch_one(&mut *self.tx)
            .await
            .map_err(db)?;
        let mut shaped = self.shape(organization, vec![row_json(&row)?]).await?;
        Ok(shaped.swap_remove(0))
    }

    /// Attachment bytes and content type, if its room is visible to this member.
    pub async fn chat_attachment(
        &mut self,
        organization: Uuid,
        attachment: Uuid,
    ) -> Result<(String, Vec<u8>), EditError> {
        let sql = format!(
            "SELECT a.message_id, a.content_type, a.data FROM {T}chat_attachments a
             WHERE a.organization_id=$1 AND a.id=$2"
        );
        let (message, content_type, data) = sqlx::query_as::<_, (Uuid, String, Vec<u8>)>(&sql)
        .bind(organization)
        .bind(attachment)
        .fetch_optional(&mut *self.tx)
        .await
        .map_err(db)?
        .ok_or_else(|| EditError::detail(404, "Attachment not found"))?;
        self.message_room(organization, message).await?;
        Ok((content_type, data))
    }

    pub async fn mark_chat_read(
        &mut self,
        organization: Uuid,
        room: Uuid,
    ) -> Result<Value, EditError> {
        self.room_access(organization, room).await?;
        sqlx::query(&format!(
            "INSERT INTO {T}chat_room_members(organization_id,room_id,user_id,last_read_at)
             VALUES($1,$2,$3,clock_timestamp()) ON CONFLICT (organization_id,room_id,user_id)
             DO UPDATE SET last_read_at=clock_timestamp()"
        ))
        .bind(organization)
        .bind(room)
        .bind(self.me())
        .execute(&mut *self.tx)
        .await
        .map_err(db)?;
        Ok(json!({ "read": true }))
    }

    pub async fn react(
        &mut self,
        organization: Uuid,
        message: Uuid,
        emoji: &str,
    ) -> Result<Value, EditError> {
        self.message_room(organization, message).await?;
        let removed: Option<String> = sqlx::query_scalar(&format!(
            "DELETE FROM {T}chat_reactions WHERE organization_id=$1 AND message_id=$2
               AND user_id=$3 AND emoji=$4 RETURNING emoji"
        ))
        .bind(organization)
        .bind(message)
        .bind(self.me())
        .bind(emoji)
        .fetch_optional(&mut *self.tx)
        .await
        .map_err(db)?;
        if removed.is_none() {
            sqlx::query(&format!(
                "INSERT INTO {T}chat_reactions(organization_id,message_id,user_id,emoji)
                 VALUES($1,$2,$3,$4) ON CONFLICT DO NOTHING"
            ))
            .bind(organization)
            .bind(message)
            .bind(self.me())
            .bind(emoji)
            .execute(&mut *self.tx)
            .await
            .map_err(db)?;
        }
        let mut found = self.reactions(organization, &[message]).await?;
        Ok(json!({ "items": found.remove(&message).unwrap_or_default() }))
    }

    pub async fn pin(&mut self, organization: Uuid, message: Uuid) -> Result<Value, EditError> {
        let (room, _, _, _) = self.message_room(organization, message).await?;
        // Lock the room row so concurrent pins cannot exceed the cap.
        sqlx::query(&format!(
            "SELECT 1 FROM {T}chat_rooms WHERE organization_id=$1 AND id=$2 FOR UPDATE"
        ))
        .bind(organization)
        .bind(room)
        .execute(&mut *self.tx)
        .await
        .map_err(db)?;
        let count: i64 = sqlx::query_scalar(&format!(
            "SELECT count(*) FROM {T}chat_pins WHERE organization_id=$1 AND room_id=$2"
        ))
        .bind(organization)
        .bind(room)
        .fetch_one(&mut *self.tx)
        .await
        .map_err(db)?;
        let exists: Option<i32> = sqlx::query_scalar(&format!(
            "SELECT 1 FROM {T}chat_pins WHERE organization_id=$1 AND message_id=$2"
        ))
        .bind(organization)
        .bind(message)
        .fetch_optional(&mut *self.tx)
        .await
        .map_err(db)?;
        if exists.is_none() {
            if count >= MAX_PINS {
                return Err(EditError::detail(
                    409,
                    format!("A room holds at most {MAX_PINS} pins"),
                ));
            }
            sqlx::query(&format!(
                "INSERT INTO {T}chat_pins(organization_id,room_id,message_id,pinned_by,
                   pinned_by_name) VALUES($1,$2,$3,$4,$5)"
            ))
            .bind(organization)
            .bind(room)
            .bind(message)
            .bind(self.me())
            .bind(self.name_200())
            .execute(&mut *self.tx)
            .await
            .map_err(db)?;
        }
        Ok(json!({ "pinned": true }))
    }

    pub async fn unpin(&mut self, organization: Uuid, message: Uuid) -> Result<Value, EditError> {
        let me = self.me();
        let (_, author, kind, role) = self.message_room(organization, message).await?;
        let pinner: Option<Uuid> = sqlx::query_scalar(&format!(
            "SELECT pinned_by FROM {T}chat_pins WHERE organization_id=$1 AND message_id=$2"
        ))
        .bind(organization)
        .bind(message)
        .fetch_optional(&mut *self.tx)
        .await
        .map_err(db)?;
        if kind == "group" && role.as_deref() != Some("admin") && author != me {
            return Err(EditError::detail(
                403,
                "Room admin or message author required",
            ));
        }
        if kind == "org" && me != author && Some(me) != pinner {
            let admin: Option<i32> = sqlx::query_scalar(
                "SELECT 1 FROM org.organization_memberships WHERE organization_id=$1
                   AND user_id=$2 AND status='active' AND role IN ('owner','admin')",
            )
            .bind(organization)
            .bind(me)
            .fetch_optional(&mut *self.tx)
            .await
            .map_err(db)?;
            if admin.is_none() {
                return Err(EditError::detail(
                    403,
                    "Message author, pinner or organisation admin required",
                ));
            }
        }
        sqlx::query(&format!(
            "DELETE FROM {T}chat_pins WHERE organization_id=$1 AND message_id=$2"
        ))
        .bind(organization)
        .bind(message)
        .execute(&mut *self.tx)
        .await
        .map_err(db)?;
        Ok(json!({ "pinned": false }))
    }

    pub async fn chat_pins(&mut self, organization: Uuid, room: Uuid) -> Result<Value, EditError> {
        self.room_access(organization, room).await?;
        let rows = sqlx::query(&format!(
            "SELECT * FROM ({MESSAGE_SQL} AND m.room_id=$2) msg JOIN (SELECT message_id,
               pinned_by_name, pinned_at FROM {T}chat_pins WHERE organization_id=$1
               AND room_id=$2) p ON p.message_id=msg.id ORDER BY p.pinned_at DESC
             LIMIT {MAX_PINS}"
        ))
        .bind(organization)
        .bind(room)
        .fetch_all(&mut *self.tx)
        .await
        .map_err(db)?;
        let mut shaped = Vec::with_capacity(rows.len());
        for row in &rows {
            let mut value = row_json(row)?;
            if let Value::Object(map) = &mut value {
                map.remove("message_id");
            }
            shaped.push(value);
        }
        Ok(json!({ "items": self.shape(organization, shaped).await? }))
    }

    /// Contacts of this user and their pending requests (newest first).
    async fn contact_state(&mut self) -> Result<(HashSet<Uuid>, Vec<PgRow>), EditError> {
        let me = self.me();
        let contacts: HashSet<Uuid> = sqlx::query_scalar::<_, Uuid>(&format!(
            "SELECT CASE WHEN user_low=$1 THEN user_high ELSE user_low END
             FROM {T}chat_contacts WHERE $1 IN (user_low,user_high)"
        ))
        .bind(me)
        .fetch_all(&mut *self.tx)
        .await
        .map_err(db)?
        .into_iter()
        .collect();
        let requests = sqlx::query(&format!(
            "SELECT id, from_user, to_user, created_at FROM {T}chat_contact_requests
             WHERE $1 IN (from_user,to_user) ORDER BY created_at DESC"
        ))
        .bind(me)
        .fetch_all(&mut *self.tx)
        .await
        .map_err(db)?;
        Ok((contacts, requests))
    }

    pub async fn chat_people(
        &mut self,
        organization: Uuid,
        query: Option<&str>,
    ) -> Result<Value, EditError> {
        let me = self.me();
        let (contacts, requests) = self.contact_state().await?;
        let mut pending: HashMap<Uuid, &str> = HashMap::new();
        for request in &requests {
            let from: Uuid = field(request, "from_user")?;
            let to: Uuid = field(request, "to_user")?;
            if to == me {
                pending.insert(from, "incoming");
            }
        }
        for request in &requests {
            let from: Uuid = field(request, "from_user")?;
            let to: Uuid = field(request, "to_user")?;
            if from == me {
                pending.insert(to, "outgoing");
            }
        }
        let avatars: HashSet<Uuid> = sqlx::query_scalar::<_, Uuid>(&format!(
            "SELECT a.user_id FROM identity.user_avatars a JOIN ({MEMBERS_SQL}) x
             ON x.id=a.user_id"
        ))
        .bind(organization)
        .fetch_all(&mut *self.tx)
        .await
        .map_err(db)?
        .into_iter()
        .collect();
        let needle = query.map(|q| fold(q.trim())).unwrap_or_default();
        // shortcut: filters after loading the roster, like the Python route; move to SQL
        // for organisations with many thousands of members.
        let mut found: Vec<(String, Uuid, String)> = self
            .people(organization)
            .await?
            .into_iter()
            .filter(|(id, _)| *id != me)
            .map(|(id, name)| (fold(&name), id, name))
            .filter(|(folded, _, _)| folded.contains(&needle))
            .collect();
        found.sort_by(|a, b| (&a.0, a.1.to_string()).cmp(&(&b.0, b.1.to_string())));
        let items: Vec<Value> = found
            .into_iter()
            .take(50)
            .map(|(_, id, name)| {
                json!({
                    "id": id,
                    "display_name": name,
                    "has_avatar": avatars.contains(&id),
                    "contact": contacts.contains(&id),
                    "request": pending.get(&id),
                })
            })
            .collect();
        Ok(json!({ "items": items }))
    }

    pub async fn person_avatar(
        &mut self,
        organization: Uuid,
        user: Uuid,
    ) -> Result<Value, EditError> {
        if !self.people(organization).await?.contains_key(&user) {
            return Err(EditError::detail(404, "Person not found"));
        }
        let value: Option<Option<String>> =
            sqlx::query_scalar("SELECT data_url FROM identity.user_avatars WHERE user_id=$1")
                .bind(user)
                .fetch_optional(&mut *self.tx)
                .await
                .map_err(db)?;
        Ok(json!({ "data_url": value.flatten() }))
    }

    pub async fn chat_contacts(&mut self, organization: Uuid) -> Result<Value, EditError> {
        let me = self.me();
        let known = self.people(organization).await?;
        let (mine, requests) = self.contact_state().await?;
        let mut items: Vec<(String, Value)> = mine
            .iter()
            .filter_map(|id| known.get(id).map(|name| (id, name)))
            .map(|(id, name)| (fold(name), json!({"user_id": id, "display_name": name})))
            .collect();
        items.sort_by(|a, b| a.0.cmp(&b.0));
        let side = |direction: &str, key: &str| -> Result<Vec<Value>, EditError> {
            let mut out = Vec::new();
            for request in &requests {
                let owner: Uuid = field(request, direction)?;
                let other: Uuid = field(request, key)?;
                if owner != me {
                    continue;
                }
                let Some(name) = known.get(&other) else {
                    continue;
                };
                let created_at: DateTime<Utc> = field(request, "created_at")?;
                out.push(json!({
                    "id": field::<Uuid>(request, "id")?,
                    "user_id": other,
                    "display_name": name,
                    "created_at": iso(created_at),
                }));
            }
            Ok(out)
        };
        Ok(json!({
            "items": items.into_iter().map(|(_, item)| item).collect::<Vec<_>>(),
            "incoming": side("to_user", "from_user")?,
            "outgoing": side("from_user", "to_user")?,
        }))
    }

    async fn make_contact(&mut self, a: Uuid, b: Uuid) -> Result<(), EditError> {
        sqlx::query(&format!(
            "INSERT INTO {T}chat_contacts(user_low,user_high) VALUES($1,$2)
             ON CONFLICT DO NOTHING"
        ))
        .bind(a.min(b))
        .bind(a.max(b))
        .execute(&mut *self.tx)
        .await
        .map_err(db)?;
        Ok(())
    }

    pub async fn request_contact(
        &mut self,
        organization: Uuid,
        user: Uuid,
    ) -> Result<Value, EditError> {
        let me = self.me();
        if user == me || !self.people(organization).await?.contains_key(&user) {
            return Err(EditError::detail(404, "Person not found"));
        }
        // Per-pair lock: crossing requests must end as one contact, not two pending.
        let pair = format!("chat-contact:{}:{}", me.min(user), me.max(user));
        sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
            .bind(pair)
            .execute(&mut *self.tx)
            .await
            .map_err(db)?;
        let (mine, requests) = self.contact_state().await?;
        if mine.contains(&user) {
            return Ok(json!({"id": null, "status": "contact"}));
        }
        let mut reverse = None;
        for request in &requests {
            if field::<Uuid>(request, "from_user")? == user
                && field::<Uuid>(request, "to_user")? == me
            {
                reverse = Some(field::<Uuid>(request, "id")?);
                break;
            }
        }
        if let Some(id) = reverse {
            sqlx::query(&format!("DELETE FROM {T}chat_contact_requests WHERE id=$1"))
                .bind(id)
                .execute(&mut *self.tx)
                .await
                .map_err(db)?;
            self.make_contact(me, user).await?;
            self.contact_audit(organization, user, "chat.contact.accept", json!({}))
                .await?;
            return Ok(json!({"id": id, "status": "contact"}));
        }
        sqlx::query(&format!(
            "INSERT INTO {T}chat_contact_requests(id,from_user,to_user) VALUES($1,$2,$3)
             ON CONFLICT (from_user,to_user) DO NOTHING"
        ))
        .bind(Uuid::new_v4())
        .bind(me)
        .bind(user)
        .execute(&mut *self.tx)
        .await
        .map_err(db)?;
        let id: Uuid = sqlx::query_scalar(&format!(
            "SELECT id FROM {T}chat_contact_requests WHERE from_user=$1 AND to_user=$2"
        ))
        .bind(me)
        .bind(user)
        .fetch_one(&mut *self.tx)
        .await
        .map_err(db)?;
        let metadata = json!({ "to": user });
        self.contact_audit(organization, id, "chat.contact.request", metadata)
            .await?;
        Ok(json!({"id": id, "status": "pending"}))
    }

    pub async fn answer_contact(
        &mut self,
        organization: Uuid,
        request: Uuid,
        accept: bool,
    ) -> Result<Value, EditError> {
        let me = self.me();
        let sender: Option<Uuid> = sqlx::query_scalar(&format!(
            "DELETE FROM {T}chat_contact_requests WHERE id=$1 AND to_user=$2 RETURNING from_user"
        ))
        .bind(request)
        .bind(me)
        .fetch_optional(&mut *self.tx)
        .await
        .map_err(db)?;
        let Some(sender) = sender else {
            return Err(EditError::detail(404, "Request not found"));
        };
        if accept {
            self.make_contact(me, sender).await?;
        }
        let action = if accept {
            "chat.contact.accept"
        } else {
            "chat.contact.decline"
        };
        let metadata = json!({ "from": sender });
        self.contact_audit(organization, request, action, metadata)
            .await?;
        Ok(json!({ "status": if accept { "contact" } else { "declined" } }))
    }

    pub async fn remove_contact(
        &mut self,
        organization: Uuid,
        user: Uuid,
    ) -> Result<(), EditError> {
        let me = self.me();
        let removed: Option<Uuid> = sqlx::query_scalar(&format!(
            "DELETE FROM {T}chat_contacts WHERE user_low=$1 AND user_high=$2 RETURNING user_low"
        ))
        .bind(me.min(user))
        .bind(me.max(user))
        .fetch_optional(&mut *self.tx)
        .await
        .map_err(db)?;
        if removed.is_some() {
            self.contact_audit(organization, user, "chat.contact.remove", json!({}))
                .await?;
        }
        Ok(())
    }
}

fn field<T>(row: &PgRow, key: &str) -> Result<T, EditError>
where
    T: for<'r> sqlx::Decode<'r, sqlx::Postgres> + sqlx::Type<sqlx::Postgres>,
{
    row.try_get(key).map_err(db)
}

fn room_wire(row: &PgRow) -> Result<Value, EditError> {
    let kind: String = field(row, "kind")?;
    let peer_id: Option<Uuid> = field(row, "peer_id")?;
    let peer_name: Option<String> = field(row, "peer_name")?;
    let name: Option<String> = if kind == "direct" {
        peer_name.clone()
    } else {
        field(row, "name")?
    };
    let last_id: Option<Uuid> = field(row, "lm_id")?;
    let last_message = match last_id {
        None => Value::Null,
        Some(id) => {
            let at: DateTime<Utc> = field(row, "lm_at")?;
            json!({
                "id": id,
                "body": field::<String>(row, "lm_body")?,
                "author_name": field::<String>(row, "lm_author")?,
                "created_at": iso(at),
            })
        }
    };
    let peer = match peer_id {
        None => Value::Null,
        Some(id) => json!({
            "id": id,
            "display_name": peer_name,
            "has_avatar": field::<bool>(row, "peer_avatar")?,
        }),
    };
    let mut wire = Map::new();
    wire.insert("id".into(), json!(field::<Uuid>(row, "id")?));
    wire.insert("kind".into(), json!(kind));
    wire.insert("name".into(), json!(name));
    wire.insert("member_count".into(), json!(field::<i64>(row, "member_count")?));
    wire.insert("unread".into(), json!(field::<i64>(row, "unread")?));
    wire.insert("mentioned".into(), json!(field::<bool>(row, "mentioned")?));
    wire.insert("last_message".into(), last_message);
    wire.insert("peer".into(), peer);
    Ok(Value::Object(wire))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fold_matches_python() {
        assert_eq!(fold("Đặng Thị Ánh"), "dang thi anh");
        assert_eq!(fold("Nguyễn Văn Đức"), "nguyen van duc");
        assert!(fold("Phạm Hồng").contains(&fold("hong")));
    }
}
