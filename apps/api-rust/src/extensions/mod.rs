//! E14 Rust-native extension platform, marketplace, developer portal, and partner routes.

pub mod activations;
pub mod manifests;
pub mod marketplace;
pub mod marketplace_v2;
pub mod runtime_v2;
pub mod marketplace_developers;
pub mod partner_platform;
pub mod records;
pub mod validation;

use axum::body::to_bytes;
use axum::extract::FromRequestParts;
use axum::http::{header, HeaderMap, Request, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::{Json, Router};
use ovg_core::AppError;
use ovg_storage::catalog::{ProjectRead, ProjectReadPolicy};
use ovg_storage::editing::{authenticate_service, domain::py_strip, EditError, ServiceAuth};
use ovg_storage::extensions::{OrgExtensionScope, TENANT_NOT_FOUND};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::catalog::CatalogError;
use crate::identity::{self, HumanSession, IdentityError};
use crate::AppState;

const MAX_BODY_BYTES: usize = 16 * 1024 * 1024;

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .merge(activations::router())
        .merge(manifests::router())
        .merge(records::router())
        .merge(marketplace::router())
        .merge(marketplace_v2::router())
        .merge(runtime_v2::router())
        .merge(marketplace_developers::router())
        .merge(partner_platform::router())
}

pub(crate) enum Actor {
    Human(HumanSession),
    Service(ServiceAuth),
}

impl Actor {
    pub(crate) fn actor_id(&self) -> Uuid {
        match self {
            Self::Human(h) => h.actor_id(),
            Self::Service(s) => s.principal_id(),
        }
    }

    pub(crate) fn is_human(&self) -> bool {
        matches!(self, Self::Human(_))
    }
}

pub(crate) fn project_actor_display(scope: &ProjectRead<'_>, actor: &Actor) -> String {
    match actor {
        Actor::Human(h) => h.display_name().to_owned(),
        Actor::Service(_) => scope.service_display_name().unwrap_or_default().to_owned(),
    }
}

pub(crate) async fn authenticate_actor(
    state: &AppState,
    parts: &mut axum::http::request::Parts,
    organization: &str,
) -> Result<Actor, ExtensionError> {
    let bearer = parts
        .headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .filter(|value| {
            value
                .get(..7)
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("bearer "))
        })
        .map(|value| py_strip(&value[7..]));
    if let Some(raw) = bearer {
        let org = Uuid::parse_str(organization).map_err(|_| {
            ExtensionError::Detail(
                StatusCode::UNAUTHORIZED,
                "invalid service credential".into(),
            )
        })?;
        let pool = state.db_pool.as_ref().ok_or_else(|| {
            ExtensionError::Domain(AppError::service_unavailable(
                "DATABASE_UNAVAILABLE",
                "database unavailable",
            ))
        })?;
        let svc = authenticate_service(pool, org, raw)
            .await
            .map_err(ExtensionError::from_edit_error)?;
        return Ok(Actor::Service(svc));
    }
    HumanSession::from_request_parts(parts, state)
        .await
        .map(Actor::Human)
        .map_err(|error| ExtensionError::Access(CatalogError::Identity(error)))
}

pub(crate) async fn authenticate_json(
    state: &AppState,
    request: Request<axum::body::Body>,
    organization: &str,
) -> Result<(Actor, HeaderMap, Option<Value>), ExtensionError> {
    let (mut parts, body) = request.into_parts();
    let bytes = to_bytes(body, MAX_BODY_BYTES)
        .await
        .map_err(|_| ExtensionError::InvalidRequest)?;
    let json = if bytes.is_empty() || !json_content_type(&parts.headers) {
        None
    } else {
        Some(serde_json::from_slice(&bytes).map_err(|_| ExtensionError::InvalidRequest)?)
    };
    let actor = authenticate_actor(state, &mut parts, organization).await?;
    Ok((actor, parts.headers, json))
}

pub(crate) async fn authenticate_human_json(
    state: &AppState,
    request: Request<axum::body::Body>,
) -> Result<(HumanSession, HeaderMap, Option<Value>), ExtensionError> {
    let (mut parts, body) = request.into_parts();
    let bytes = to_bytes(body, MAX_BODY_BYTES)
        .await
        .map_err(|_| ExtensionError::InvalidRequest)?;
    let json = if bytes.is_empty() || !json_content_type(&parts.headers) {
        None
    } else {
        Some(serde_json::from_slice(&bytes).map_err(|_| ExtensionError::InvalidRequest)?)
    };
    let session = HumanSession::from_request_parts(&mut parts, state)
        .await
        .map_err(|error| ExtensionError::Access(CatalogError::Identity(error)))?;
    Ok((session, parts.headers, json))
}

fn json_content_type(headers: &HeaderMap) -> bool {
    let Some(value) = headers.get(header::CONTENT_TYPE) else {
        return true;
    };
    let value = value.to_str().unwrap_or("invalid");
    if value.is_empty() {
        return true;
    }
    let mime = value
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    mime.split_once('/').is_some_and(|(main, sub)| {
        main == "application" && (sub == "json" || sub.ends_with("+json"))
    })
}

pub(crate) fn require_actor_csrf(
    state: &AppState,
    actor: &Actor,
    headers: &HeaderMap,
) -> Result<(), ExtensionError> {
    let Actor::Human(session) = actor else {
        return Ok(());
    };
    identity::require_csrf(state, session, headers)
        .map_err(|error| ExtensionError::Access(CatalogError::Identity(error)))
}

pub(crate) fn decode_body<T: DeserializeOwned>(body: Option<Value>) -> Result<T, ExtensionError> {
    body.and_then(|value| serde_json::from_value(value).ok())
        .ok_or(ExtensionError::InvalidRequest)
}

pub(crate) fn parse_uuid(value: &str) -> Result<Uuid, ExtensionError> {
    Uuid::parse_str(value).map_err(|_| ExtensionError::InvalidRequest)
}

pub(crate) async fn open_org_scope<'a>(
    state: &'a AppState,
    actor: &Actor,
    organization_id: Uuid,
) -> Result<OrgExtensionScope<'a>, ExtensionError> {
    let pool = state.db_pool.as_ref().ok_or_else(|| {
        ExtensionError::Domain(AppError::service_unavailable(
            "DATABASE_UNAVAILABLE",
            "database unavailable",
        ))
    })?;
    match actor {
        Actor::Human(session) => OrgExtensionScope::begin_human(
            pool,
            organization_id,
            session.actor_id(),
            session.session_id(),
        )
        .await
        .map_err(ExtensionError::from_catalog_app),
        Actor::Service(svc) => OrgExtensionScope::begin_service(pool, organization_id, svc)
            .await
            .map_err(ExtensionError::from_catalog_app),
    }
}

pub(crate) async fn open_project_scope<'a>(
    state: &'a AppState,
    actor: &Actor,
    organization_id: Uuid,
    project_id: Uuid,
    policy: ProjectReadPolicy,
) -> Result<ProjectRead<'a>, ExtensionError> {
    let pool = state.db_pool.as_ref().ok_or_else(|| {
        ExtensionError::Domain(AppError::service_unavailable(
            "DATABASE_UNAVAILABLE",
            "database unavailable",
        ))
    })?;
    match actor {
        Actor::Human(session) => ProjectRead::begin(
            pool,
            organization_id,
            project_id,
            session.actor_id(),
            session.session_id(),
            policy,
        )
        .await
        .map_err(|e| ExtensionError::Access(CatalogError::spatial(e))),
        Actor::Service(svc) => {
            ProjectRead::begin_service(pool, organization_id, project_id, svc, policy)
                .await
                .map_err(|e| ExtensionError::Access(CatalogError::spatial(e)))
        }
    }
}

pub(crate) enum ExtensionError {
    InvalidRequest,
    Detail(StatusCode, String),
    Access(CatalogError),
    Domain(AppError),
}

impl ExtensionError {
    /// Extension storage errors are FastAPI `PlatformDomainError`s (`{code, message}`),
    /// except tenant-access failures, which FastAPI renders as `{detail}`.
    pub(crate) fn from_catalog_app(error: AppError) -> Self {
        if matches!(&error, AppError::NotFound { code, .. } if *code == TENANT_NOT_FOUND) {
            return Self::Access(CatalogError::Application(error));
        }
        Self::Domain(error)
    }

    pub(crate) fn from_edit_error(error: EditError) -> Self {
        match error {
            EditError::Detail(status, msg) => Self::Detail(
                StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
                msg,
            ),
            EditError::InvalidRequest => Self::InvalidRequest,
            EditError::Internal(detail) => Self::Domain(AppError::internal(detail)),
            EditError::Spatial(status, code, detail) => Self::Access(CatalogError::Spatial(
                StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
                code,
                detail.into(),
            )),
            EditError::Platform {
                status,
                code,
                message,
                ..
            } => Self::Detail(
                StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
                format!("{code}: {message}"),
            ),
        }
    }
}

impl From<IdentityError> for ExtensionError {
    fn from(error: IdentityError) -> Self {
        Self::Access(CatalogError::Identity(error))
    }
}

impl IntoResponse for ExtensionError {
    fn into_response(self) -> Response {
        match self {
            Self::InvalidRequest => (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({
                    "detail": [{"loc": [], "msg": "invalid request payload", "type": "value_error"}]
                })),
            )
                .into_response(),
            Self::Detail(status, detail) => {
                (status, Json(json!({ "detail": detail }))).into_response()
            }
            Self::Access(error) => error.into_response(),
            Self::Domain(AppError::Internal { internal_detail }) => {
                tracing::error!(detail = %internal_detail, "extension request failed");
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response()
            }
            Self::Domain(error) => {
                let status = StatusCode::from_u16(error.status_code())
                    .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
                (status, Json(error.to_envelope(None))).into_response()
            }
        }
    }
}
