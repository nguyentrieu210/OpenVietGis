//! Organization chat REST (`org_chat`): rooms, messages, attachments, reactions,
//! pins, people and contacts. Human organization members only.
//!
//! Order follows FastAPI: malformed JSON (422) -> credential (401) -> CSRF (403)
//! -> path/body/query validation (422) -> human actor (403) -> membership (404).

use axum::body::Body;
use axum::extract::{Path, Request, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, patch, post};
use axum::{Json, Router};
use ovg_core::AppError;
use ovg_storage::editing::domain::py_strip;
use ovg_storage::editing::EditError;
use ovg_storage::tenancy::chat::{Attachment, NewChatMessage};
use ovg_storage::tenancy::TenantTx;
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

use crate::editing::{authenticate_actor, authenticate_json, decode, require_csrf, Actor, Failure};
use crate::managed_sync::{int_param, invalid, param, query_pairs, stripped, uuid, Pairs};
use crate::AppState;

const BASE: &str = "/organizations/{organization_id}/chat";

pub(crate) fn router() -> Router<AppState> {
    let room = format!("{BASE}/rooms/{{room_id}}");
    let message = format!("{BASE}/messages/{{message_id}}");
    Router::new()
        .route(&format!("{BASE}/rooms"), get(rooms).post(create_room))
        .route(&format!("{BASE}/rooms/direct"), post(direct_room))
        .route(&room, patch(rename_room))
        .route(&format!("{room}/members"), post(invite))
        .route(
            &format!("{room}/members/{{user_id}}"),
            delete(remove_member),
        )
        .route(
            &format!("{room}/messages"),
            get(messages).post(post_message),
        )
        .route(&format!("{room}/read"), post(mark_read))
        .route(&format!("{room}/pins"), get(pins))
        .route(
            &format!("{BASE}/attachments/{{attachment_id}}"),
            get(attachment),
        )
        .route(&format!("{message}/reactions"), post(react))
        .route(&format!("{message}/pin"), post(pin).delete(unpin))
        .route(&format!("{BASE}/people"), get(people))
        .route(&format!("{BASE}/people/{{user_id}}/avatar"), get(avatar))
        .route(&format!("{BASE}/contacts"), get(contacts))
        .route(&format!("{BASE}/contacts/requests"), post(request_contact))
        .route(
            &format!("{BASE}/contacts/requests/{{request_id}}/accept"),
            post(accept),
        )
        .route(
            &format!("{BASE}/contacts/requests/{{request_id}}/decline"),
            post(decline),
        )
        .route(
            &format!("{BASE}/contacts/{{user_id}}"),
            delete(remove_contact),
        )
}

type Org = Path<String>;
type OrgChild = Path<(String, String)>;
type OrgChildren = Path<(String, String, String)>;

fn app(error: AppError) -> Failure {
    match error {
        AppError::Internal { internal_detail } => EditError::Internal(internal_detail).into(),
        error => EditError::detail(error.status_code(), error.to_string()).into(),
    }
}

/// GET: credential, then path and query validation.
async fn read(
    state: &AppState,
    organization: &str,
    request: Request,
) -> Result<(Actor, Uuid, Pairs), Failure> {
    let (mut parts, _) = request.into_parts();
    let actor = authenticate_actor(state, &mut parts, organization).await?;
    let organization = uuid(organization)?;
    Ok((actor, organization, query_pairs(&parts.uri)?))
}

/// POST/PATCH/DELETE: body, credential, CSRF, then path validation.
async fn write(
    state: &AppState,
    organization: &str,
    request: Request,
) -> Result<(Actor, Uuid, Option<Value>), Failure> {
    let (actor, headers, body) = authenticate_json(state, request, organization).await?;
    require_csrf(state, &actor, &headers)?;
    Ok((actor, uuid(organization)?, body))
}

/// `org_scope`: a human actor (403), then an active member (404).
async fn member(state: &AppState, actor: Actor, organization: Uuid) -> Result<TenantTx, Failure> {
    let Actor::Human(auth) = actor else {
        return Err(EditError::detail(403, "Human organisation member required").into());
    };
    let pool = state
        .db_pool
        .as_ref()
        .ok_or_else(|| EditError::detail(503, "database unavailable"))?;
    let (actor, session) = (auth.actor_id(), auth.session_id());
    let mut tx = TenantTx::begin(pool, actor, session, auth.display_name())
        .await
        .map_err(app)?;
    tx.chat_scope(organization).await.map_err(app)?;
    Ok(tx)
}

async fn done(
    tx: TenantTx,
    body: Value,
    status: StatusCode,
    cache: Option<&'static str>,
) -> Result<Response, Failure> {
    tx.commit().await.map_err(app)?;
    let mut response = (status, Json(body)).into_response();
    if let Some(cache) = cache {
        response
            .headers_mut()
            .insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    }
    Ok(response)
}

async fn rooms(
    State(state): State<AppState>,
    Path(organization): Org,
    request: Request,
) -> Result<Response, Failure> {
    let (actor, organization, _) = read(&state, &organization, request).await?;
    let mut tx = member(&state, actor, organization).await?;
    let body = tx.chat_rooms_list(organization).await?;
    done(tx, body, StatusCode::OK, Some("no-store")).await
}

/// `Name`: stripped, 1..=80 characters.
fn room_name(name: String) -> Result<String, Failure> {
    stripped(Some(name), "", 80)
}

fn user_list(ids: &[Uuid]) -> Result<(), Failure> {
    if (1..=49).contains(&ids.len()) {
        Ok(())
    } else {
        Err(invalid())
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RoomBody {
    name: String,
    member_ids: Vec<Uuid>,
}

async fn create_room(
    State(state): State<AppState>,
    Path(organization): Org,
    request: Request,
) -> Result<Response, Failure> {
    let (actor, organization, body) = write(&state, &organization, request).await?;
    let body: RoomBody = decode(body)?;
    let name = room_name(body.name)?;
    user_list(&body.member_ids)?;
    let mut tx = member(&state, actor, organization).await?;
    let ids = body.member_ids;
    let room = tx.create_chat_room(organization, &name, &ids).await?;
    done(tx, room, StatusCode::CREATED, None).await
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UserBody {
    user_id: Uuid,
}

async fn direct_room(
    State(state): State<AppState>,
    Path(organization): Org,
    request: Request,
) -> Result<Response, Failure> {
    let (actor, organization, body) = write(&state, &organization, request).await?;
    let body: UserBody = decode(body)?;
    let mut tx = member(&state, actor, organization).await?;
    let room = tx.direct_room(organization, body.user_id).await?;
    done(tx, room, StatusCode::OK, None).await
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RenameBody {
    name: String,
}

async fn rename_room(
    State(state): State<AppState>,
    Path((organization, room)): OrgChild,
    request: Request,
) -> Result<Response, Failure> {
    let (actor, organization, body) = write(&state, &organization, request).await?;
    let room = uuid(&room)?;
    let body: RenameBody = decode(body)?;
    let name = room_name(body.name)?;
    let mut tx = member(&state, actor, organization).await?;
    let result = tx.rename_chat_room(organization, room, &name).await?;
    done(tx, result, StatusCode::OK, None).await
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UsersBody {
    user_ids: Vec<Uuid>,
}

async fn invite(
    State(state): State<AppState>,
    Path((organization, room)): OrgChild,
    request: Request,
) -> Result<Response, Failure> {
    let (actor, organization, body) = write(&state, &organization, request).await?;
    let room = uuid(&room)?;
    let body: UsersBody = decode(body)?;
    user_list(&body.user_ids)?;
    let mut tx = member(&state, actor, organization).await?;
    let ids = body.user_ids;
    let result = tx.invite_chat_members(organization, room, &ids).await?;
    done(tx, result, StatusCode::OK, None).await
}

async fn remove_member(
    State(state): State<AppState>,
    Path((organization, room, user)): OrgChildren,
    request: Request,
) -> Result<Response, Failure> {
    let (actor, organization, _) = write(&state, &organization, request).await?;
    let (room, user) = (uuid(&room)?, uuid(&user)?);
    let mut tx = member(&state, actor, organization).await?;
    tx.remove_chat_member(organization, room, user).await?;
    tx.commit().await.map_err(app)?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

async fn messages(
    State(state): State<AppState>,
    Path((organization, room)): OrgChild,
    request: Request,
) -> Result<Response, Failure> {
    let (actor, organization, pairs) = read(&state, &organization, request).await?;
    let room = uuid(&room)?;
    let limit = int_param(&pairs, "limit", 50, 1, 100)?;
    let before = param(&pairs, "before").map(uuid).transpose()?;
    let mut tx = member(&state, actor, organization).await?;
    let body = tx.chat_messages(organization, room, limit, before).await?;
    done(tx, body, StatusCode::OK, Some("no-store")).await
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AttachmentBody {
    kind: String,
    data_url: String,
    width: i32,
    height: i32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MessageBody {
    #[serde(default)]
    body: String,
    #[serde(default)]
    reply_to_id: Option<Uuid>,
    #[serde(default)]
    mention_ids: Vec<Uuid>,
    #[serde(default)]
    attachments: Vec<AttachmentBody>,
}

/// `ChatMessageRequest` field constraints and its "body or attachment" validator.
fn message_body(body: MessageBody) -> Result<(String, MessageBody), Failure> {
    let text = py_strip(&body.body).to_owned();
    let attachments_ok = body.attachments.iter().all(|a| {
        a.kind == "image"
            && (1..=560_000).contains(&a.data_url.chars().count())
            && (1..=4096).contains(&a.width)
            && (1..=4096).contains(&a.height)
    });
    let valid = text.chars().count() <= 4000
        && body.mention_ids.len() <= 20
        && body.attachments.len() <= 4
        && attachments_ok
        && !(text.is_empty() && body.attachments.is_empty());
    if !valid {
        return Err(invalid());
    }
    Ok((text, body))
}

async fn post_message(
    State(state): State<AppState>,
    Path((organization, room)): OrgChild,
    request: Request,
) -> Result<Response, Failure> {
    let (actor, organization, body) = write(&state, &organization, request).await?;
    let room = uuid(&room)?;
    let (text, body) = message_body(decode(body)?)?;
    let attachments = body
        .attachments
        .into_iter()
        .map(|a| Attachment {
            data_url: a.data_url,
            width: a.width,
            height: a.height,
        })
        .collect();
    let message = NewChatMessage {
        body: text,
        reply_to: body.reply_to_id,
        mention_ids: body.mention_ids,
        attachments,
    };
    let mut tx = member(&state, actor, organization).await?;
    let message = tx.post_chat_message(organization, room, message).await?;
    done(tx, message, StatusCode::CREATED, None).await
}

async fn attachment(
    State(state): State<AppState>,
    Path((organization, attachment)): OrgChild,
    request: Request,
) -> Result<Response, Failure> {
    let (actor, organization, _) = read(&state, &organization, request).await?;
    let attachment = uuid(&attachment)?;
    let mut tx = member(&state, actor, organization).await?;
    let (content_type, data) = tx.chat_attachment(organization, attachment).await?;
    tx.commit().await.map_err(app)?;
    let content_type = HeaderValue::from_str(&content_type)
        .map_err(|_| EditError::Internal("attachment content type".into()))?;
    let headers = [
        (header::CONTENT_TYPE, content_type),
        (
            header::CACHE_CONTROL,
            HeaderValue::from_static("private, max-age=86400"),
        ),
        (
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ),
        (
            header::CONTENT_DISPOSITION,
            HeaderValue::from_static("inline"),
        ),
    ];
    Ok((headers, Body::from(data)).into_response())
}

async fn mark_read(
    State(state): State<AppState>,
    Path((organization, room)): OrgChild,
    request: Request,
) -> Result<Response, Failure> {
    let (actor, organization, _) = write(&state, &organization, request).await?;
    let room = uuid(&room)?;
    let mut tx = member(&state, actor, organization).await?;
    let body = tx.mark_chat_read(organization, room).await?;
    done(tx, body, StatusCode::OK, None).await
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReactionBody {
    emoji: String,
}

async fn react(
    State(state): State<AppState>,
    Path((organization, message)): OrgChild,
    request: Request,
) -> Result<Response, Failure> {
    let (actor, organization, body) = write(&state, &organization, request).await?;
    let message = uuid(&message)?;
    let body: ReactionBody = decode(body)?;
    let emoji = stripped(Some(body.emoji), "", 16)?;
    let mut tx = member(&state, actor, organization).await?;
    let result = tx.react(organization, message, &emoji).await?;
    done(tx, result, StatusCode::OK, None).await
}

async fn pin(
    State(state): State<AppState>,
    Path((organization, message)): OrgChild,
    request: Request,
) -> Result<Response, Failure> {
    let (actor, organization, _) = write(&state, &organization, request).await?;
    let message = uuid(&message)?;
    let mut tx = member(&state, actor, organization).await?;
    let result = tx.pin(organization, message).await?;
    done(tx, result, StatusCode::OK, None).await
}

async fn unpin(
    State(state): State<AppState>,
    Path((organization, message)): OrgChild,
    request: Request,
) -> Result<Response, Failure> {
    let (actor, organization, _) = write(&state, &organization, request).await?;
    let message = uuid(&message)?;
    let mut tx = member(&state, actor, organization).await?;
    let result = tx.unpin(organization, message).await?;
    done(tx, result, StatusCode::OK, None).await
}

async fn pins(
    State(state): State<AppState>,
    Path((organization, room)): OrgChild,
    request: Request,
) -> Result<Response, Failure> {
    let (actor, organization, _) = read(&state, &organization, request).await?;
    let room = uuid(&room)?;
    let mut tx = member(&state, actor, organization).await?;
    let body = tx.chat_pins(organization, room).await?;
    done(tx, body, StatusCode::OK, Some("no-store")).await
}

async fn people(
    State(state): State<AppState>,
    Path(organization): Org,
    request: Request,
) -> Result<Response, Failure> {
    let (actor, organization, pairs) = read(&state, &organization, request).await?;
    let query = param(&pairs, "q").map(str::to_owned);
    if query.as_ref().is_some_and(|q| q.chars().count() > 80) {
        return Err(invalid());
    }
    let mut tx = member(&state, actor, organization).await?;
    let body = tx.chat_people(organization, query.as_deref()).await?;
    done(tx, body, StatusCode::OK, Some("no-store")).await
}

async fn avatar(
    State(state): State<AppState>,
    Path((organization, user)): OrgChild,
    request: Request,
) -> Result<Response, Failure> {
    let (actor, organization, _) = read(&state, &organization, request).await?;
    let user = uuid(&user)?;
    let mut tx = member(&state, actor, organization).await?;
    let body = tx.person_avatar(organization, user).await?;
    done(tx, body, StatusCode::OK, Some("private, max-age=300")).await
}

async fn contacts(
    State(state): State<AppState>,
    Path(organization): Org,
    request: Request,
) -> Result<Response, Failure> {
    let (actor, organization, _) = read(&state, &organization, request).await?;
    let mut tx = member(&state, actor, organization).await?;
    let body = tx.chat_contacts(organization).await?;
    done(tx, body, StatusCode::OK, Some("no-store")).await
}

async fn request_contact(
    State(state): State<AppState>,
    Path(organization): Org,
    request: Request,
) -> Result<Response, Failure> {
    let (actor, organization, body) = write(&state, &organization, request).await?;
    let body: UserBody = decode(body)?;
    let mut tx = member(&state, actor, organization).await?;
    let result = tx.request_contact(organization, body.user_id).await?;
    done(tx, result, StatusCode::CREATED, None).await
}

async fn answer(
    state: &AppState,
    organization: &str,
    request_id: &str,
    request: Request,
    accept: bool,
) -> Result<Response, Failure> {
    let (actor, organization, _) = write(state, organization, request).await?;
    let request_id = uuid(request_id)?;
    let mut tx = member(state, actor, organization).await?;
    let result = tx.answer_contact(organization, request_id, accept).await?;
    done(tx, result, StatusCode::OK, None).await
}

async fn accept(
    State(state): State<AppState>,
    Path((organization, request_id)): OrgChild,
    request: Request,
) -> Result<Response, Failure> {
    answer(&state, &organization, &request_id, request, true).await
}

async fn decline(
    State(state): State<AppState>,
    Path((organization, request_id)): OrgChild,
    request: Request,
) -> Result<Response, Failure> {
    answer(&state, &organization, &request_id, request, false).await
}

async fn remove_contact(
    State(state): State<AppState>,
    Path((organization, user)): OrgChild,
    request: Request,
) -> Result<Response, Failure> {
    let (actor, organization, _) = write(&state, &organization, request).await?;
    let user = uuid(&user)?;
    let mut tx = member(&state, actor, organization).await?;
    tx.remove_contact(organization, user).await?;
    tx.commit().await.map_err(app)?;
    Ok(StatusCode::NO_CONTENT.into_response())
}
