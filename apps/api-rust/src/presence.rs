//! E11 opt-in, ephemeral WebSocket presence (`team_presence`): map cursors and views
//! of full project members, no spatial writes. RFC 6455 framing runs directly on the
//! hyper upgrade so no WebSocket dependency is added.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use axum::body::Body;
use axum::extract::{FromRequestParts, Path, Request, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use hyper_util::rt::TokioIo;
use ovg_storage::catalog::{ProjectRead, ProjectReadPolicy};
use serde_json::{json, Value};
use sha1::Sha1;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{mpsc, watch};
use tokio::time::Instant;
use uuid::Uuid;

use crate::identity::HumanSession;
use crate::AppState;

const COLORS: [&str; 6] = [
    "#2563eb", "#db2777", "#059669", "#9333ea", "#b45309", "#0891b2",
];
const MAX_PEERS: usize = 64;
const MAX_ROOM: usize = 40;
// Not in the Python route: one user cannot take the whole process budget.
const MAX_PER_ACTOR: usize = 8;
const KEEPALIVE: Duration = Duration::from_secs(20);
const WRITE_TIMEOUT: Duration = Duration::from_secs(3);
const ACCEPT_GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
const REAUTHORIZE_EVERY: Duration = Duration::from_secs(5);
const MIN_INTERVAL: Duration = Duration::from_millis(80);
// Match the ASGI WebSocket receive ceiling; presence itself applies the stricter
// 2048-character policy only after a complete text message has been reassembled.
const MAX_FRAME: usize = 16 * 1024 * 1024;
const POLICY: u16 = 1008;

struct Peer {
    id: String,
    actor: String,
    name: String,
    color: &'static str,
    cursor: Value,
    view: Value,
    outbox: watch::Sender<String>,
}

type Room = Vec<Peer>;

static ROOMS: LazyLock<Mutex<HashMap<(Uuid, Uuid), Room>>> = LazyLock::new(Mutex::default);

pub(crate) fn router() -> Router<AppState> {
    Router::new().route(
        "/organizations/{organization_id}/projects/{project_id}/collaboration/presence",
        get(presence),
    )
}

/// `{"type": "presence", "self_id", "users"}` to every peer; only the latest is kept.
fn broadcast(room: &Room) {
    let users: Vec<Value> = room
        .iter()
        .map(|peer| {
            json!({
                "id": peer.id,
                "actor": peer.actor,
                "name": peer.name,
                "color": peer.color,
                "cursor": peer.cursor,
                "view": peer.view,
            })
        })
        .collect();
    for peer in room {
        let message = json!({"type": "presence", "self_id": peer.id, "users": users});
        peer.outbox.send_replace(message.to_string());
    }
}

fn rooms() -> std::sync::MutexGuard<'static, HashMap<(Uuid, Uuid), Room>> {
    ROOMS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn leave(key: (Uuid, Uuid), peer: &str) {
    let mut rooms = rooms();
    if let Some(room) = rooms.get_mut(&key) {
        room.retain(|candidate| candidate.id != peer);
        if room.is_empty() {
            rooms.remove(&key);
        } else {
            broadcast(room);
        }
    }
}

/// Python `COLORS[int(sha256(actor).hexdigest(), 16) % 6]`.
fn color(actor: &str) -> &'static str {
    let digest = Sha256::digest(actor.as_bytes());
    let index = digest.iter().fold(0usize, |acc, byte| {
        (acc * 256 + usize::from(*byte)) % COLORS.len()
    });
    COLORS[index]
}

/// Cookies are ambient credentials: reject cross-site WebSocket requests.
fn origin_allowed(state: &AppState, headers: &HeaderMap) -> bool {
    let base = state.config.web_base_url.as_str();
    let expected = base
        .split_once("://")
        .map(|(scheme, rest)| {
            let netloc = rest.split(['/', '?', '#']).next().unwrap_or("");
            format!("{scheme}://{netloc}")
        })
        .unwrap_or_default();
    headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|origin| origin == expected)
}

/// The `Sec-WebSocket-Accept` value, if this is a version 13 upgrade request.
fn accept_key(headers: &HeaderMap) -> Option<String> {
    let has = |name: header::HeaderName, token: &str| {
        headers.get_all(name).iter().any(|value| {
            value.to_str().is_ok_and(|value| {
                value
                    .split(',')
                    .any(|part| part.trim().eq_ignore_ascii_case(token))
            })
        })
    };
    let version = headers.get(header::SEC_WEBSOCKET_VERSION)?;
    if !has(header::UPGRADE, "websocket") || !has(header::CONNECTION, "upgrade") || version != "13"
    {
        return None;
    }
    let key = headers.get(header::SEC_WEBSOCKET_KEY)?.as_bytes();
    let mut hasher = Sha1::new();
    hasher.update(key);
    hasher.update(ACCEPT_GUID.as_bytes());
    Some(STANDARD.encode(hasher.finalize()))
}

/// Session, full project read access and no GIS scoping, in a short transaction.
async fn still_allowed(state: &AppState, key: (Uuid, Uuid), actor: Uuid, session: Uuid) -> bool {
    let Some(pool) = state.db_pool.as_ref() else {
        return false;
    };
    let policy = ProjectReadPolicy::Unrestricted;
    match ProjectRead::begin(pool, key.0, key.1, actor, session, policy).await {
        Ok(read) => read.commit().await.is_ok(),
        Err(_) => false,
    }
}

async fn presence(
    State(state): State<AppState>,
    Path((organization, project)): Path<(String, String)>,
    request: Request,
) -> Response {
    let (mut parts, body) = request.into_parts();
    let Some(accept) = accept_key(&parts.headers) else {
        return (StatusCode::NOT_FOUND, Json(json!({"detail": "Not Found"}))).into_response();
    };
    // A close before accept is an HTTP 403 for the WebSocket client.
    let forbidden = || StatusCode::FORBIDDEN.into_response();
    let ids = Uuid::parse_str(&organization).ok();
    let Some(key) = ids.zip(Uuid::parse_str(&project).ok()) else {
        return forbidden();
    };
    if !origin_allowed(&state, &parts.headers) {
        return forbidden();
    }
    let Ok(auth) = HumanSession::from_request_parts(&mut parts, &state).await else {
        return forbidden();
    };
    let (actor, session) = (auth.actor_id(), auth.session_id());
    if !still_allowed(&state, key, actor, session).await {
        return forbidden();
    }
    let (outbox, inbox) = watch::channel(String::new());
    let actor_text = actor.to_string();
    let peer = Peer {
        id: Uuid::new_v4().to_string(),
        name: auth.display_name().chars().take(100).collect(),
        color: color(&actor_text),
        actor: actor_text,
        cursor: Value::Null,
        view: Value::Null,
        outbox,
    };
    let peer_id = peer.id.clone();
    {
        let mut rooms = rooms();
        let total: usize = rooms.values().map(Vec::len).sum();
        let same_actor = |other: &&Peer| other.actor == peer.actor;
        let mine = rooms.values().flatten().filter(same_actor).count();
        if total >= MAX_PEERS
            || rooms.get(&key).map_or(0, Vec::len) >= MAX_ROOM
            || mine >= MAX_PER_ACTOR
        {
            return forbidden();
        }
        rooms.entry(key).or_default().push(peer);
    }
    let mut request = Request::from_parts(parts, body);
    let upgrade = hyper::upgrade::on(&mut request);
    let connection = Connection {
        state,
        key,
        peer: peer_id,
        actor,
        session,
    };
    tokio::spawn(async move {
        match upgrade.await {
            Ok(upgraded) => connection.run(TokioIo::new(upgraded), inbox).await,
            Err(_) => leave(connection.key, &connection.peer),
        }
    });
    Response::builder()
        .status(StatusCode::SWITCHING_PROTOCOLS)
        .header(header::UPGRADE, "websocket")
        .header(header::CONNECTION, "Upgrade")
        .header(header::SEC_WEBSOCKET_ACCEPT, accept)
        .body(Body::empty())
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

#[derive(Debug, PartialEq)]
enum Frame {
    Text(String),
    Binary,
    Ping(Vec<u8>),
    Pong,
    Close(Option<u16>),
}

/// Incremental RFC 6455 reader for masked client frames, with fragment reassembly.
#[derive(Default)]
struct Reader {
    buffer: Vec<u8>,
    fragments: Option<(u8, Vec<u8>)>,
}

impl Reader {
    /// The next complete message, `Ok(None)` when more bytes are needed, or a close code.
    fn next(&mut self) -> Result<Option<Frame>, u16> {
        loop {
            let Some((fin, opcode, payload)) = self.frame()? else {
                return Ok(None);
            };
            let message = match opcode {
                0x0 => {
                    let Some((kind, mut data)) = self.fragments.take() else {
                        return Err(1002);
                    };
                    data.extend_from_slice(&payload);
                    if data.len() > MAX_FRAME {
                        return Err(POLICY);
                    }
                    if !fin {
                        self.fragments = Some((kind, data));
                        continue;
                    }
                    (kind, data)
                }
                0x1 | 0x2 if self.fragments.is_some() => return Err(1002),
                0x1 | 0x2 if !fin => {
                    self.fragments = Some((opcode, payload));
                    continue;
                }
                0x1 | 0x2 => (opcode, payload),
                0x8 => {
                    let code = payload
                        .get(..2)
                        .map(|code| u16::from_be_bytes([code[0], code[1]]));
                    return Ok(Some(Frame::Close(code)));
                }
                0x9 => return Ok(Some(Frame::Ping(payload))),
                0xA => return Ok(Some(Frame::Pong)),
                _ => return Err(1002),
            };
            return match message {
                (0x1, data) => String::from_utf8(data)
                    .map(|text| Some(Frame::Text(text)))
                    .map_err(|_| 1007),
                _ => Ok(Some(Frame::Binary)),
            };
        }
    }

    /// One complete frame from the buffer: (fin, opcode, unmasked payload).
    fn frame(&mut self) -> Result<Option<(bool, u8, Vec<u8>)>, u16> {
        let buffer = &self.buffer;
        if buffer.len() < 2 {
            return Ok(None);
        }
        let (fin, opcode) = (buffer[0] & 0x80 != 0, buffer[0] & 0x0f);
        if buffer[0] & 0x70 != 0 || buffer[1] & 0x80 == 0 {
            return Err(1002);
        }
        let (length, mut offset) = match buffer[1] & 0x7f {
            126 if buffer.len() >= 4 => {
                let length = u16::from_be_bytes([buffer[2], buffer[3]]);
                (usize::from(length), 4)
            }
            127 if buffer.len() >= 10 => {
                let mut bytes = [0u8; 8];
                bytes.copy_from_slice(&buffer[2..10]);
                (
                    usize::try_from(u64::from_be_bytes(bytes)).unwrap_or(usize::MAX),
                    10,
                )
            }
            126 | 127 => return Ok(None),
            short => (usize::from(short), 2),
        };
        // RFC 6455 5.5: control frames are unfragmented, at most 125 bytes, and a close
        // body is either empty or a two-byte code plus reason.
        if opcode >= 0x8 && (!fin || length > 125 || (opcode == 0x8 && length == 1)) {
            return Err(1002);
        }
        if length > MAX_FRAME {
            return Err(POLICY);
        }
        if buffer.len() < offset + 4 + length {
            return Ok(None);
        }
        let mask = [
            buffer[offset],
            buffer[offset + 1],
            buffer[offset + 2],
            buffer[offset + 3],
        ];
        offset += 4;
        let payload: Vec<u8> = buffer[offset..offset + length]
            .iter()
            .enumerate()
            .map(|(index, byte)| byte ^ mask[index % 4])
            .collect();
        self.buffer.drain(..offset + length);
        Ok(Some((fin, opcode, payload)))
    }
}

/// One unmasked server frame.
fn encode(opcode: u8, payload: &[u8]) -> Vec<u8> {
    let mut frame = vec![0x80 | opcode];
    match payload.len() {
        length @ 0..=125 => frame.push(length as u8),
        length @ 126..=0xffff => {
            frame.push(126);
            frame.extend_from_slice(&(length as u16).to_be_bytes());
        }
        length => {
            frame.push(127);
            frame.extend_from_slice(&(length as u64).to_be_bytes());
        }
    }
    frame.extend_from_slice(payload);
    frame
}

/// Python `numbers`: a list of real numbers within each (low, high) bound.
fn numbers(value: &Value, limits: &[(f64, f64)]) -> Option<Value> {
    let items = value.as_array()?;
    let fits = items.len() == limits.len()
        && items.iter().zip(limits).all(|(item, (low, high))| {
            item.as_f64()
                .is_some_and(|number| (*low..=*high).contains(&number))
        });
    fits.then(|| value.clone())
}

/// Python `update`: the new (cursor, view), or `None` for an invalid message.
fn update(cursor: &Value, view: &Value, raw: &str) -> Option<(Value, Value)> {
    if raw.chars().count() > 2048 {
        return None;
    }
    let Ok(Value::Object(data)) = serde_json::from_str::<Value>(raw) else {
        return None;
    };
    if data.is_empty() || data.keys().any(|key| key != "cursor" && key != "view") {
        return None;
    }
    let cursor = match data.get("cursor") {
        None => cursor.clone(),
        Some(Value::Null) => Value::Null,
        Some(value) => numbers(value, &[(-180.0, 180.0), (-90.0, 90.0)])?,
    };
    let view = match data.get("view") {
        None => view.clone(),
        Some(value) => numbers(
            value,
            &[
                (-180.0, 180.0),
                (-85.0, 85.0),
                (0.0, 24.0),
                (-180.0, 180.0),
                (0.0, 85.0),
            ],
        )?,
    };
    Some((cursor, view))
}

struct Connection {
    state: AppState,
    key: (Uuid, Uuid),
    peer: String,
    actor: Uuid,
    session: Uuid,
}

/// How a presence connection ended.
#[derive(Debug, PartialEq)]
enum Ending {
    /// The server closes with this code.
    Close(u16),
    /// The client closed; the echo is already queued.
    Echoed,
    /// The transport is gone or the writer failed: nothing more to send.
    Gone,
}

impl Connection {
    async fn run(self, io: TokioIo<hyper::upgrade::Upgraded>, mut inbox: watch::Receiver<String>) {
        let (mut reader, mut writer) = tokio::io::split(io);
        let (control, mut controls) = mpsc::channel::<Vec<u8>>(4);
        let mut sender = tokio::spawn(async move {
            loop {
                let frame = tokio::select! {
                    changed = inbox.changed() => {
                        if changed.is_err() {
                            break;
                        }
                        encode(0x1, inbox.borrow_and_update().as_bytes())
                    }
                    frame = controls.recv() => {
                        let Some(frame) = frame else {
                            break;
                        };
                        frame
                    }
                };
                // Every write is bounded so a stalled peer cannot pin the connection.
                let written = tokio::time::timeout(WRITE_TIMEOUT, writer.write_all(&frame));
                if !matches!(written.await, Ok(Ok(()))) {
                    break;
                }
                if frame.first() == Some(&0x88) {
                    let _ = tokio::time::timeout(WRITE_TIMEOUT, writer.shutdown()).await;
                    break;
                }
            }
        });
        if let Some(room) = rooms().get(&self.key) {
            broadcast(room);
        }
        let ending = self.receive(&mut reader, &control, &sender).await;
        let queued = match ending {
            Ending::Close(code) => control.try_send(encode(0x8, &code.to_be_bytes())).is_ok(),
            Ending::Echoed => true,
            Ending::Gone => false,
        };
        if queued {
            // Let the close frame go out (bounded by the write timeout) before dropping.
            let _ = tokio::time::timeout(Duration::from_secs(2), &mut sender).await;
        }
        sender.abort();
        leave(self.key, &self.peer);
    }

    /// Read until the connection ends.
    async fn receive(
        &self,
        reader: &mut tokio::io::ReadHalf<TokioIo<hyper::upgrade::Upgraded>>,
        control: &mpsc::Sender<Vec<u8>>,
        sender: &tokio::task::JoinHandle<()>,
    ) -> Ending {
        let mut frames = Reader::default();
        let mut check = Instant::now() + REAUTHORIZE_EVERY;
        let mut ping_at = Instant::now() + KEEPALIVE;
        let mut pong_by: Option<Instant> = None;
        let mut last: Option<Instant> = None;
        loop {
            if sender.is_finished() {
                return Ending::Gone;
            }
            let now = Instant::now();
            if now >= check {
                let (actor, session) = (self.actor, self.session);
                if !still_allowed(&self.state, self.key, actor, session).await {
                    return Ending::Close(POLICY);
                }
                check = Instant::now() + REAUTHORIZE_EVERY;
            }
            // Keepalive like uvicorn: ping every 20 s, give up after 20 s of silence.
            if pong_by.is_some_and(|deadline| now >= deadline) {
                return Ending::Close(1011);
            }
            if pong_by.is_none() && now >= ping_at {
                if control.try_send(encode(0x9, b"")).is_err() {
                    return Ending::Gone;
                }
                pong_by = Some(now + KEEPALIVE);
                ping_at = now + KEEPALIVE;
            }
            let frame = match frames.next() {
                Err(code) => return Ending::Close(code),
                Ok(Some(frame)) => frame,
                Ok(None) => {
                    let wake = check.min(pong_by.unwrap_or(ping_at));
                    let read = tokio::time::timeout_at(wake, reader.read_buf(&mut frames.buffer));
                    match read.await {
                        Err(_) => continue,
                        Ok(Ok(0) | Err(_)) => return Ending::Gone,
                        Ok(Ok(_)) => continue,
                    }
                }
            };
            // Any frame proves the peer is alive.
            pong_by = None;
            match frame {
                Frame::Text(text) => {
                    let now = Instant::now();
                    if last.is_some_and(|last| now - last < MIN_INTERVAL) {
                        continue;
                    }
                    last = Some(now);
                    if !self.apply(&text) {
                        return Ending::Close(POLICY);
                    }
                }
                // Binary frames are not part of the presence protocol.
                Frame::Binary => return Ending::Close(1003),
                Frame::Ping(payload) => {
                    // A peer that does not drain its pongs is stuck: drop it.
                    if control.try_send(encode(0xA, &payload)).is_err() {
                        return Ending::Gone;
                    }
                }
                Frame::Pong => {}
                Frame::Close(code) => {
                    let echo = code.map(|code| code.to_be_bytes().to_vec());
                    let echo = encode(0x8, &echo.unwrap_or_default());
                    return if control.try_send(echo).is_ok() {
                        Ending::Echoed
                    } else {
                        Ending::Gone
                    };
                }
            }
        }
    }

    /// Validate and store one presence update, then broadcast the room.
    fn apply(&self, raw: &str) -> bool {
        let mut rooms = rooms();
        let Some(room) = rooms.get_mut(&self.key) else {
            return false;
        };
        let Some(peer) = room.iter_mut().find(|peer| peer.id == self.peer) else {
            return false;
        };
        let Some((cursor, view)) = update(&peer.cursor, &peer.view, raw) else {
            return false;
        };
        peer.cursor = cursor;
        peer.view = view;
        broadcast(room);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn masked(first: u8, payload: &[u8]) -> Vec<u8> {
        let mask = [1u8, 2, 3, 4];
        let mut frame = vec![first];
        match payload.len() {
            length @ 0..=125 => frame.push(0x80 | length as u8),
            length @ 126..=0xffff => {
                frame.push(0x80 | 126);
                frame.extend_from_slice(&(length as u16).to_be_bytes());
            }
            length => {
                frame.push(0x80 | 127);
                frame.extend_from_slice(&(length as u64).to_be_bytes());
            }
        }
        frame.extend_from_slice(&mask);
        frame.extend(payload.iter().enumerate().map(|(i, b)| b ^ mask[i % 4]));
        frame
    }

    #[test]
    fn reader_unmasks_and_reassembles() {
        let mut reader = Reader::default();
        reader.buffer.extend(masked(0x01, b"{\"cur"));
        assert_eq!(reader.next(), Ok(None));
        reader.buffer.extend(masked(0x80, b"sor\":null}"));
        assert_eq!(
            reader.next(),
            Ok(Some(Frame::Text("{\"cursor\":null}".into())))
        );
        reader.buffer.extend(masked(0x82, b"\x00"));
        assert_eq!(reader.next(), Ok(Some(Frame::Binary)));
        reader.buffer.extend(masked(0x88, &1000u16.to_be_bytes()));
        assert_eq!(reader.next(), Ok(Some(Frame::Close(Some(1000)))));
        reader.buffer.extend([0x81, 0x01, b'x']);
        assert_eq!(reader.next(), Err(1002));
        // Control frames must be final, short, and never carry a one-byte close body.
        for frame in [masked(0x09, b""), masked(0x88, b"x")] {
            let mut reader = Reader::default();
            reader.buffer.extend(frame);
            assert_eq!(reader.next(), Err(1002));
        }
    }

    #[test]
    fn reader_accepts_extended_frames_before_presence_policy() {
        let payload = vec![b'x'; 70 * 1024];
        let mut reader = Reader::default();
        reader.buffer.extend(masked(0x81, &payload));
        let Some(Frame::Text(text)) = reader.next().unwrap() else {
            panic!("expected text frame");
        };
        assert_eq!(text.len(), payload.len());
        // Transport accepts it; the application-level presence validator rejects it.
        assert!(update(&Value::Null, &Value::Null, &text).is_none());
    }

    #[test]
    fn update_follows_the_python_validator() {
        let null = Value::Null;
        let (cursor, _) = update(&null, &null, r#"{"cursor":[105.8,21]}"#).unwrap();
        assert_eq!(cursor, json!([105.8, 21]));
        assert!(update(&null, &null, r#"{"cursor":[200,0]}"#).is_none());
        assert!(update(&null, &null, r#"{"cursor":[true,0]}"#).is_none());
        assert!(update(&null, &null, r#"{"view":null}"#).is_none());
        assert!(update(&null, &null, r#"{}"#).is_none());
        assert!(update(&null, &null, r#"{"zoom":1}"#).is_none());
        let view = r#"{"view":[105,21,12,0,45]}"#;
        assert_eq!(
            update(&null, &null, view).unwrap().1,
            json!([105, 21, 12, 0, 45])
        );
    }

    #[test]
    fn handshake_and_colors() {
        let mut headers = HeaderMap::new();
        headers.insert(header::UPGRADE, "websocket".parse().unwrap());
        headers.insert(header::CONNECTION, "keep-alive, Upgrade".parse().unwrap());
        headers.insert(header::SEC_WEBSOCKET_VERSION, "13".parse().unwrap());
        headers.insert(
            header::SEC_WEBSOCKET_KEY,
            "dGhlIHNhbXBsZSBub25jZQ==".parse().unwrap(),
        );
        // RFC 6455 section 1.3 example.
        assert_eq!(
            accept_key(&headers).as_deref(),
            Some("s3pPLMBiTxaQ9kYGzzhZRbK+xOo=")
        );
        assert!(COLORS.contains(&color("actor")));
        assert_eq!(encode(0x1, b"hi"), vec![0x81, 2, b'h', b'i']);
    }
}