//! R011 project collaboration REST (`team_chat`, `team_locks`, `collaboration`):
//! project chat, feature comments, tasks, notifications, edit leases, change
//! reviews and the conflict-rebase report.
//!
//! Order follows FastAPI: malformed JSON (422) -> credential (401) ->
//! `project_access_guard` (404/403) -> CSRF (403) -> body/query validation (422)
//! -> human-only scope (403) -> project permission -> domain.

use axum::extract::{Path, Request, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use chrono::NaiveDate;
use ovg_storage::editing::team::NewTask;
use ovg_storage::editing::{EditError, EditScope, Guard};
use serde::Deserialize;
use serde_json::{Map, Value};
use uuid::Uuid;

use crate::editing::{
    authenticate_actor, authenticate_json, decode, open, require_csrf, Actor, Failure,
};
use crate::managed_sync::{
    bool_param, finish, int_param, invalid, param, query_pairs, stripped, uuid, uuid_param, Pairs,
};
use crate::AppState;

const BASE: &str = "/organizations/{organization_id}/projects/{project_id}/collaboration";
const SOURCE: &str = "/organizations/{organization_id}/projects/{project_id}/sources/{source_id}";

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route(&format!("{BASE}/members"), get(members))
        .route(
            &format!("{BASE}/messages"),
            get(messages).post(post_message),
        )
        .route(
            &format!("{BASE}/comments"),
            get(comments).post(post_comment),
        )
        .route(&format!("{BASE}/comment-pins"), get(comment_pins))
        .route(&format!("{BASE}/tasks"), get(tasks).post(create_task))
        .route(&format!("{BASE}/tasks/{{task_id}}"), patch(change_task))
        .route(&format!("{BASE}/notifications"), get(notifications))
        .route(
            &format!("{BASE}/notifications/{{task_id}}/read"),
            post(read_notification),
        )
        .route(
            &format!("{BASE}/mentions/{{message_id}}/read"),
            post(read_mention),
        )
        .route(&format!("{BASE}/edit-locks"), get(leases).post(acquire))
        .route(&format!("{BASE}/edit-locks/release"), post(release))
        .route(&format!("{BASE}/reviews"), get(reviews).post(create_review))
        .route(
            &format!("{BASE}/reviews/{{review_id}}/comments"),
            get(review_comments).post(add_review_comment),
        )
        .route(
            &format!("{BASE}/reviews/{{review_id}}/decide"),
            post(decide_review),
        )
        .route(&format!("{SOURCE}/conflict-rebase"), post(conflict_rebase))
}

type Ids = Path<(String, String)>;
type ChildIds = Path<(String, String, String)>;

/// GET: credential, guard (scoped accounts denied), query string.
async fn read_scope<'a>(
    state: &'a AppState,
    ids: &(String, String),
    request: Request,
) -> Result<(Actor, Option<EditScope<'a>>, Pairs), Failure> {
    let (mut parts, _) = request.into_parts();
    let actor = authenticate_actor(state, &mut parts, &ids.0).await?;
    let scope = open(state, &actor, &ids.0, &ids.1, Guard::Read).await?;
    Ok((actor, scope, query_pairs(&parts.uri)?))
}

/// POST/PATCH: body, credential, guard (scoped accounts denied), CSRF.
async fn write_scope<'a>(
    state: &'a AppState,
    ids: &(String, String),
    request: Request,
) -> Result<(Actor, Option<EditScope<'a>>, Option<Value>), Failure> {
    let (actor, headers, body) = authenticate_json(state, request, &ids.0).await?;
    let scope = open(state, &actor, &ids.0, &ids.1, Guard::ScopedDenied).await?;
    require_csrf(state, &actor, &headers)?;
    Ok((actor, scope, body))
}

/// `team_chat.scope`: a valid project path, then a human actor.
fn human<'a>(actor: &Actor, scope: Option<EditScope<'a>>) -> Result<EditScope<'a>, Failure> {
    let Some(scope) = scope else {
        return Err(invalid());
    };
    if matches!(actor, Actor::Service(_)) {
        return Err(EditError::detail(403, "Human project member required").into());
    }
    Ok(scope)
}

fn path_scope(scope: Option<EditScope<'_>>) -> Result<EditScope<'_>, Failure> {
    scope.ok_or_else(invalid)
}

async fn no_store(scope: EditScope<'_>, body: Value) -> Result<Response, Failure> {
    let mut response = finish(scope, body, StatusCode::OK).await?;
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

/// `Field(min_length=1, max_length=128)` without stripping.
fn feature_length(value: &str) -> Result<(), Failure> {
    if (1..=128).contains(&value.chars().count()) {
        Ok(())
    } else {
        Err(invalid())
    }
}

async fn members(
    State(state): State<AppState>,
    Path(ids): Ids,
    request: Request,
) -> Result<Response, Failure> {
    let (actor, scope, _) = read_scope(&state, &ids, request).await?;
    let mut scope = human(&actor, scope)?;
    let result = scope.team_members().await?;
    no_store(scope, result).await
}

async fn messages(
    State(state): State<AppState>,
    Path(ids): Ids,
    request: Request,
) -> Result<Response, Failure> {
    let (actor, scope, pairs) = read_scope(&state, &ids, request).await?;
    let limit = int_param(&pairs, "limit", 100, 1, 100)?;
    let peer = param(&pairs, "peer").map(uuid).transpose()?;
    let mut scope = human(&actor, scope)?;
    let result = scope.team_messages(limit, peer).await?;
    no_store(scope, result).await
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MessageBody {
    body: String,
    #[serde(default)]
    recipient_id: Option<Uuid>,
}

async fn post_message(
    State(state): State<AppState>,
    Path(ids): Ids,
    request: Request,
) -> Result<Response, Failure> {
    let (actor, scope, body) = write_scope(&state, &ids, request).await?;
    let body: MessageBody = decode(body)?;
    let text = stripped(Some(body.body), "", 4000)?;
    let mut scope = human(&actor, scope)?;
    let result = scope.post_team_message(&text, body.recipient_id).await?;
    finish(scope, result, StatusCode::CREATED).await
}

async fn comments(
    State(state): State<AppState>,
    Path(ids): Ids,
    request: Request,
) -> Result<Response, Failure> {
    let (actor, scope, pairs) = read_scope(&state, &ids, request).await?;
    let source = uuid_param(&pairs, "source_id")?;
    let feature = param(&pairs, "feature_id").ok_or_else(invalid)?.to_owned();
    feature_length(&feature)?;
    let limit = int_param(&pairs, "limit", 100, 1, 100)?;
    let mut scope = human(&actor, scope)?;
    let result = scope.feature_comments(source, &feature, limit).await?;
    no_store(scope, result).await
}

async fn comment_pins(
    State(state): State<AppState>,
    Path(ids): Ids,
    request: Request,
) -> Result<Response, Failure> {
    let (actor, scope, _) = read_scope(&state, &ids, request).await?;
    let mut scope = human(&actor, scope)?;
    let result = scope.comment_pins().await?;
    no_store(scope, result).await
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CommentBody {
    source_id: Uuid,
    feature_id: String,
    body: String,
    #[serde(default)]
    mention_ids: Vec<Uuid>,
}

async fn post_comment(
    State(state): State<AppState>,
    Path(ids): Ids,
    request: Request,
) -> Result<Response, Failure> {
    let (actor, scope, body) = write_scope(&state, &ids, request).await?;
    let body: CommentBody = decode(body)?;
    feature_length(&body.feature_id)?;
    let text = stripped(Some(body.body), "", 4000)?;
    if body.mention_ids.len() > 20 {
        return Err(invalid());
    }
    let mut scope = human(&actor, scope)?;
    let (source, feature) = (body.source_id, body.feature_id);
    let result = scope
        .post_feature_comment(source, &feature, &text, &body.mention_ids)
        .await?;
    finish(scope, result, StatusCode::CREATED).await
}

async fn tasks(
    State(state): State<AppState>,
    Path(ids): Ids,
    request: Request,
) -> Result<Response, Failure> {
    let (actor, scope, pairs) = read_scope(&state, &ids, request).await?;
    let mine = bool_param(&pairs, "mine")?;
    let limit = int_param(&pairs, "limit", 200, 1, 200)?;
    let mut scope = human(&actor, scope)?;
    let result = scope.team_tasks(mine, limit).await?;
    no_store(scope, result).await
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TaskBody {
    title: String,
    #[serde(default)]
    description: String,
    assignee_id: Uuid,
    #[serde(default)]
    source_message_id: Option<Uuid>,
    #[serde(default)]
    source_id: Option<Uuid>,
    #[serde(default)]
    feature_id: Option<String>,
    #[serde(default)]
    priority: Option<String>,
    #[serde(default)]
    due_date: Option<String>,
    #[serde(default)]
    longitude: Option<f64>,
    #[serde(default)]
    latitude: Option<f64>,
}

/// `TaskRequest` field constraints and its location model validator.
fn task(body: TaskBody) -> Result<NewTask, Failure> {
    let title = stripped(Some(body.title), "", 200)?;
    let priority = body.priority.unwrap_or_else(|| "normal".to_owned());
    let feature = body.feature_id.as_deref().unwrap_or("x");
    let due_date = body
        .due_date
        .map(|date| NaiveDate::parse_from_str(&date, "%Y-%m-%d"))
        .transpose()
        .map_err(|_| invalid())?;
    let valid = body.description.chars().count() <= 2000
        && matches!(priority.as_str(), "low" | "normal" | "high")
        && feature_length(feature).is_ok()
        && body.longitude.is_none_or(|x| (-180.0..=180.0).contains(&x))
        && body.latitude.is_none_or(|y| (-90.0..=90.0).contains(&y))
        && body.source_id.is_none() == body.feature_id.is_none()
        && body.longitude.is_none() == body.latitude.is_none();
    if !valid {
        return Err(invalid());
    }
    Ok(NewTask {
        title,
        description: body.description,
        assignee_id: body.assignee_id,
        source_message_id: body.source_message_id,
        source_id: body.source_id,
        feature_id: body.feature_id,
        priority,
        due_date,
        longitude: body.longitude,
        latitude: body.latitude,
    })
}

async fn create_task(
    State(state): State<AppState>,
    Path(ids): Ids,
    request: Request,
) -> Result<Response, Failure> {
    let (actor, scope, body) = write_scope(&state, &ids, request).await?;
    let task = task(decode(body)?)?;
    let mut scope = human(&actor, scope)?;
    let result = scope.create_team_task(task).await?;
    finish(scope, result, StatusCode::CREATED).await
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StatusBody {
    status: String,
    revision: i64,
}

async fn change_task(
    State(state): State<AppState>,
    Path((organization, project, task_id)): ChildIds,
    request: Request,
) -> Result<Response, Failure> {
    let (actor, scope, body) = write_scope(&state, &(organization, project), request).await?;
    let body: StatusBody = decode(body)?;
    let status_ok = matches!(body.status.as_str(), "todo" | "in_progress" | "done");
    if !status_ok || body.revision < 1 {
        return Err(invalid());
    }
    let task_id = uuid(&task_id)?;
    let mut scope = human(&actor, scope)?;
    let result = scope
        .change_team_task(task_id, &body.status, body.revision)
        .await?;
    finish(scope, result, StatusCode::OK).await
}

async fn notifications(
    State(state): State<AppState>,
    Path(ids): Ids,
    request: Request,
) -> Result<Response, Failure> {
    let (actor, scope, _) = read_scope(&state, &ids, request).await?;
    let mut scope = human(&actor, scope)?;
    let result = scope.team_notifications().await?;
    no_store(scope, result).await
}

async fn read_notification(
    State(state): State<AppState>,
    Path((organization, project, task_id)): ChildIds,
    request: Request,
) -> Result<Response, Failure> {
    let (actor, scope, _) = write_scope(&state, &(organization, project), request).await?;
    let task_id = uuid(&task_id)?;
    let mut scope = human(&actor, scope)?;
    let result = scope.read_task_notification(task_id).await?;
    finish(scope, result, StatusCode::OK).await
}

async fn read_mention(
    State(state): State<AppState>,
    Path((organization, project, message_id)): ChildIds,
    request: Request,
) -> Result<Response, Failure> {
    let (actor, scope, _) = write_scope(&state, &(organization, project), request).await?;
    let message_id = uuid(&message_id)?;
    let mut scope = human(&actor, scope)?;
    let result = scope.read_mention(message_id).await?;
    finish(scope, result, StatusCode::OK).await
}

async fn leases(
    State(state): State<AppState>,
    Path(ids): Ids,
    request: Request,
) -> Result<Response, Failure> {
    let (actor, scope, _) = read_scope(&state, &ids, request).await?;
    let mut scope = human(&actor, scope)?;
    let result = scope.edit_leases().await?;
    finish(scope, result, StatusCode::OK).await
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LeaseBody {
    source_id: Uuid,
    feature_id: String,
    token: Uuid,
}

async fn lease_scope<'a>(
    state: &'a AppState,
    ids: &(String, String),
    request: Request,
) -> Result<(EditScope<'a>, LeaseBody), Failure> {
    let (actor, scope, body) = write_scope(state, ids, request).await?;
    let body: LeaseBody = decode(body)?;
    feature_length(&body.feature_id)?;
    Ok((human(&actor, scope)?, body))
}

async fn acquire(
    State(state): State<AppState>,
    Path(ids): Ids,
    request: Request,
) -> Result<Response, Failure> {
    let (mut scope, body) = lease_scope(&state, &ids, request).await?;
    let result = scope
        .acquire_edit_lease(body.source_id, &body.feature_id, body.token)
        .await?;
    finish(scope, result, StatusCode::OK).await
}

async fn release(
    State(state): State<AppState>,
    Path(ids): Ids,
    request: Request,
) -> Result<Response, Failure> {
    let (mut scope, body) = lease_scope(&state, &ids, request).await?;
    let result = scope
        .release_edit_lease(body.source_id, &body.feature_id, body.token)
        .await?;
    finish(scope, result, StatusCode::OK).await
}

async fn reviews(
    State(state): State<AppState>,
    Path(ids): Ids,
    request: Request,
) -> Result<Response, Failure> {
    let (_, scope, _) = read_scope(&state, &ids, request).await?;
    let mut scope = path_scope(scope)?;
    let result = scope.list_reviews().await?;
    finish(scope, result, StatusCode::OK).await
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateReviewBody {
    changeset_id: Uuid,
    assignee_user_id: Uuid,
}

async fn create_review(
    State(state): State<AppState>,
    Path(ids): Ids,
    request: Request,
) -> Result<Response, Failure> {
    let (_, scope, body) = write_scope(&state, &ids, request).await?;
    let body: CreateReviewBody = decode(body)?;
    let mut scope = path_scope(scope)?;
    let result = scope
        .create_review(body.changeset_id, body.assignee_user_id)
        .await?;
    finish(scope, result, StatusCode::CREATED).await
}

async fn review_comments(
    State(state): State<AppState>,
    Path((organization, project, review_id)): ChildIds,
    request: Request,
) -> Result<Response, Failure> {
    let (_, scope, _) = read_scope(&state, &(organization, project), request).await?;
    let review_id = uuid(&review_id)?;
    let mut scope = path_scope(scope)?;
    let result = scope.review_comments(review_id).await?;
    finish(scope, result, StatusCode::OK).await
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AddCommentBody {
    body: String,
    #[serde(default)]
    feature_id: Option<String>,
    #[serde(default)]
    operation_id: Option<Uuid>,
}

async fn add_review_comment(
    State(state): State<AppState>,
    Path((organization, project, review_id)): ChildIds,
    request: Request,
) -> Result<Response, Failure> {
    let (_, scope, body) = write_scope(&state, &(organization, project), request).await?;
    let body: AddCommentBody = decode(body)?;
    let review_id = uuid(&review_id)?;
    let mut scope = path_scope(scope)?;
    let feature = body.feature_id.as_deref();
    let result = scope
        .add_review_comment(review_id, &body.body, feature, body.operation_id)
        .await?;
    finish(scope, result, StatusCode::CREATED).await
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DecideBody {
    status: String,
    #[serde(default)]
    decision_note: String,
}

async fn decide_review(
    State(state): State<AppState>,
    Path((organization, project, review_id)): ChildIds,
    request: Request,
) -> Result<Response, Failure> {
    let (_, scope, body) = write_scope(&state, &(organization, project), request).await?;
    let body: DecideBody = decode(body)?;
    let known = matches!(
        body.status.as_str(),
        "pending" | "approved" | "changes_requested"
    );
    if !known {
        return Err(invalid());
    }
    let review_id = uuid(&review_id)?;
    let mut scope = path_scope(scope)?;
    let result = scope
        .decide_review(review_id, &body.status, &body.decision_note)
        .await?;
    finish(scope, result, StatusCode::OK).await
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RebaseBody {
    base_content_revision: String,
    draft_operations: Vec<Map<String, Value>>,
}

async fn conflict_rebase(
    State(state): State<AppState>,
    Path((organization, project, source)): ChildIds,
    request: Request,
) -> Result<Response, Failure> {
    let (_, scope, body) = write_scope(&state, &(organization, project), request).await?;
    let body: RebaseBody = decode(body)?;
    let source = uuid(&source)?;
    let mut scope = path_scope(scope)?;
    let draft: Vec<Value> = body
        .draft_operations
        .into_iter()
        .map(Value::Object)
        .collect();
    let result = scope
        .conflict_rebase(source, &body.base_content_revision, &draft)
        .await?;
    finish(scope, result, StatusCode::OK).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_validator_matches_the_pydantic_model() {
        let body = |extra: Value| -> TaskBody {
            let mut base = serde_json::json!({
                "title": "  Kiểm tra  ",
                "assignee_id": Uuid::nil(),
            });
            for (key, value) in extra.as_object().unwrap() {
                base[key] = value.clone();
            }
            serde_json::from_value(base).unwrap()
        };
        let ok = task(body(serde_json::json!({}))).ok().unwrap();
        assert_eq!(ok.title, "Kiểm tra");
        assert_eq!(ok.priority, "normal");
        assert!(task(body(serde_json::json!({"longitude": 105.0}))).is_err());
        assert!(task(body(serde_json::json!({"source_id": Uuid::nil()}))).is_err());
        assert!(task(body(serde_json::json!({"priority": "urgent"}))).is_err());
        assert!(task(body(serde_json::json!({"due_date": "2026-13-01"}))).is_err());
    }
}
