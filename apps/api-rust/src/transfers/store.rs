//! SHA-256-verified immutable local object store (`adapters/storage/local_object_store.py`)
//! and the bounded multipart reader for untrusted uploads.

use std::path::PathBuf;

use axum::body::Bytes;
use futures_util::stream::Stream;
use ovg_storage::transfers::TransferError;
use sha2::{Digest, Sha256};
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use uuid::Uuid;

use super::codec::hex;
use crate::AppState;

const CHUNK: usize = 64 * 1024;

pub(crate) struct Store {
    root: PathBuf,
}

fn io(error: std::io::Error) -> TransferError {
    TransferError::internal(format!("artifact store I/O failed: {error}"))
}

/// `^[a-z0-9][a-z0-9/_.-]{0,199}$` without `..` segments.
fn valid_key(key: &str) -> bool {
    let bytes = key.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 200
        && (bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit())
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"/_.-".contains(b))
        && !key.split('/').any(|segment| segment == "..")
}

impl Store {
    pub(crate) fn new(state: &AppState) -> Self {
        Self {
            root: state.config.artifact_store_dir.clone(),
        }
    }

    fn path(&self, key: &str) -> Result<PathBuf, TransferError> {
        if !valid_key(key) {
            return Err(TransferError::validation(
                "INVALID_STORAGE_KEY",
                "Invalid storage key.",
            ));
        }
        Ok(self.root.join(key))
    }

    /// Stage a new object next to its final key; nothing is visible until `finish`.
    pub(super) async fn stage(&self, key: &str) -> Result<Staged, TransferError> {
        let target = self.path(key)?;
        let parent = target
            .parent()
            .ok_or_else(|| TransferError::internal("storage key has no parent"))?;
        tokio::fs::create_dir_all(parent).await.map_err(io)?;
        let temp = parent.join(format!(".tmp_{}", Uuid::new_v4().simple()));
        let file = File::create(&temp).await.map_err(io)?;
        Ok(Staged {
            file,
            temp,
            target,
            hasher: Sha256::new(),
            size: 0,
        })
    }

    pub(crate) async fn put(&self, key: &str, data: &[u8]) -> Result<(String, i64), TransferError> {
        let mut staged = self.stage(key).await?;
        if let Err(error) = staged.write(data).await {
            staged.abort().await;
            return Err(error);
        }
        staged.finish().await
    }

    pub(crate) async fn delete(&self, key: &str) {
        if let Ok(path) = self.path(key) {
            let _ = tokio::fs::remove_file(path).await;
        }
    }

    /// Open an object after verifying its SHA-256 in bounded chunks.
    pub(crate) async fn open_verified(
        &self,
        key: &str,
        sha256: &str,
    ) -> Result<(File, u64), TransferError> {
        let path = self.path(key)?;
        let missing = || {
            TransferError::coded(
                410,
                "ARTIFACT_MISSING",
                "Stored immutable object is missing.",
            )
        };
        let metadata = tokio::fs::metadata(&path).await.map_err(|_| missing())?;
        if !metadata.is_file() {
            return Err(missing());
        }
        let mut file = File::open(&path).await.map_err(|_| missing())?;
        let mut hasher = Sha256::new();
        let mut buffer = vec![0_u8; CHUNK];
        let mut length = 0_u64;
        loop {
            let read = file.read(&mut buffer).await.map_err(io)?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
            length += read as u64;
        }
        if hex(&hasher.finalize()) != sha256 {
            return Err(TransferError::coded(
                410,
                "ARTIFACT_CORRUPTED",
                "Stored immutable object failed integrity check.",
            ));
        }
        // Stream the verified file descriptor, not a second path lookup.
        file.seek(std::io::SeekFrom::Start(0)).await.map_err(io)?;
        Ok((file, length))
    }

    /// Whole-object read for bounded (<= 25 MiB) source assets.
    pub(crate) async fn read_verified(
        &self,
        key: &str,
        sha256: &str,
    ) -> Result<Vec<u8>, TransferError> {
        let (mut file, length) = self.open_verified(key, sha256).await?;
        let mut data = Vec::with_capacity(usize::try_from(length).unwrap_or(0));
        file.read_to_end(&mut data).await.map_err(io)?;
        if hex(&Sha256::digest(&data)) != sha256 {
            return Err(TransferError::coded(
                410,
                "ARTIFACT_CORRUPTED",
                "Stored immutable object failed integrity check.",
            ));
        }
        Ok(data)
    }

    pub(crate) async fn delete(&self, key: &str) {
        if let Ok(path) = self.path(key) {
            let _ = tokio::fs::remove_file(path).await;
        }
    }
}

pub(super) struct Staged {
    file: File,
    temp: PathBuf,
    target: PathBuf,
    hasher: Sha256,
    size: u64,
}

impl Staged {
    pub(super) async fn write(&mut self, data: &[u8]) -> Result<(), TransferError> {
        self.hasher.update(data);
        self.size += data.len() as u64;
        self.file.write_all(data).await.map_err(io)
    }

    /// fsync, then atomically publish under the final key.
    pub(super) async fn finish(mut self) -> Result<(String, i64), TransferError> {
        let synced = async {
            self.file.flush().await?;
            self.file.sync_all().await?;
            tokio::fs::rename(&self.temp, &self.target).await
        }
        .await;
        if let Err(error) = synced {
            let _ = tokio::fs::remove_file(&self.temp).await;
            return Err(io(error));
        }
        let size =
            i64::try_from(self.size).map_err(|_| TransferError::internal("object too large"))?;
        Ok((hex(&self.hasher.finalize()), size))
    }

    pub(super) async fn abort(self) {
        drop(self.file);
        let _ = tokio::fs::remove_file(&self.temp).await;
    }
}

/// Bounded chunked body for an opened artifact.
pub(crate) fn file_stream(file: File) -> impl Stream<Item = std::io::Result<Bytes>> + Send {
    futures_util::stream::unfold(Some(file), |state| async move {
        let mut file = state?;
        let mut buffer = vec![0_u8; CHUNK];
        match file.read(&mut buffer).await {
            Ok(0) => None,
            Ok(read) => {
                buffer.truncate(read);
                Some((Ok(Bytes::from(buffer)), Some(file)))
            }
            Err(error) => Some((Err(error), None)),
        }
    })
}

/// Extract all parts `(name, filename, content)` of a `multipart/form-data` body.
/// Returns `None` for malformed multipart payloads or invalid headers.
pub(crate) fn multipart_parts<'a>(
    content_type: &str,
    body: &'a [u8],
) -> Option<Vec<(String, Option<&'a str>, &'a [u8])>> {
    let (mime, params) = content_type.split_once(';')?;
    if !mime.trim().eq_ignore_ascii_case("multipart/form-data") {
        return None;
    }
    let boundary = params.split(';').find_map(|p| {
        let (key, value) = p.split_once('=')?;
        key.trim()
            .eq_ignore_ascii_case("boundary")
            .then(|| value.trim().trim_matches('"'))
    })?;
    if boundary.is_empty() || boundary.len() > 200 {
        return None;
    }
    let delimiter = format!("--{boundary}");
    let separator = format!("\r\n--{boundary}");
    // A delimiter is a complete boundary line, not merely a prefix in a payload.
    // Require the opening boundary and validate each following boundary suffix.
    let mut position = body
        .strip_prefix(delimiter.as_bytes())
        .map(|_| delimiter.len())?;
    let mut parts = Vec::new();
    loop {
        let rest = body.get(position..)?;
        if rest == b"--" || rest.starts_with(b"--\r\n") {
            return Some(parts);
        }
        if !rest.starts_with(b"\r\n") {
            return None;
        }
        position += 2;
        let header_end = find(body, b"\r\n\r\n", position)?;
        let headers = std::str::from_utf8(&body[position..header_end]).ok()?;
        let content_start = header_end + 4;
        let mut search = content_start;
        let (content_end, is_final) = loop {
            let at = find(body, separator.as_bytes(), search)?;
            let suffix = body.get(at + separator.len()..)?;
            if suffix.starts_with(b"\r\n") {
                break (at, false);
            }
            if suffix == b"--" || suffix.starts_with(b"--\r\n") {
                break (at, true);
            }
            search = at + 1;
        };
        if let Some((name, filename)) = parse_disposition(headers) {
            parts.push((name, filename, &body[content_start..content_end]));
        }
        if is_final {
            return Some(parts);
        }
        position = content_end + separator.len();
    }
}

/// Extract the `file` part of a `multipart/form-data` body: (filename, content).
/// Returns `None` for anything that is not a well-formed upload with that part.
pub(super) fn multipart_file<'a>(
    content_type: &str,
    body: &'a [u8],
) -> Option<(Option<&'a str>, &'a [u8])> {
    multipart_parts(content_type, body)?
        .into_iter()
        .find_map(|(name, filename, content)| {
            // Starlette treats a part without a filename as a plain form field, not a file.
            (name == "file" && filename.is_some()).then_some((filename, content))
        })
}

fn parse_disposition(headers: &str) -> Option<(String, Option<&str>)> {
    let disposition = headers.split("\r\n").find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.trim()
            .eq_ignore_ascii_case("content-disposition")
            .then_some(value)
    })?;
    let mut part_name = None;
    let mut filename = None;
    for item in disposition.split(';').skip(1) {
        let Some((key, value)) = item.split_once('=') else {
            continue;
        };
        let value = value.trim().trim_matches('"');
        match key.trim().to_ascii_lowercase().as_str() {
            "name" => part_name = Some(value.to_owned()),
            "filename" => filename = Some(value),
            _ => {}
        }
    }
    Some((part_name?, filename))
}

fn find(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    haystack
        .get(from..)?
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|at| at + from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_keys_reject_traversal_and_uppercase() {
        assert!(valid_key("artifacts/0a/1b/x.geojson"));
        assert!(!valid_key("artifacts/../etc"));
        assert!(!valid_key("/abs"));
        assert!(!valid_key("Upper"));
        assert!(!valid_key(""));
    }

    #[tokio::test]
    async fn verified_object_is_rewound_and_rejects_wrong_digest() {
        let root = std::env::temp_dir().join(format!("ovg-transfer-{}", Uuid::new_v4()));
        let store = Store { root: root.clone() };
        let payload = b"verified transfer content";
        let (digest, _) = store.put("object.bin", payload).await.unwrap();
        let (mut file, length) = store.open_verified("object.bin", &digest).await.unwrap();
        let mut actual = Vec::new();
        file.read_to_end(&mut actual).await.unwrap();
        assert_eq!(length, payload.len() as u64);
        assert_eq!(actual.as_slice(), payload);
        let wrong_digest = "0".repeat(64);
        assert!(matches!(
            store.open_verified("object.bin", &wrong_digest).await,
            Err(TransferError::Coded { status: 410, .. })
        ));
        drop(file);
        tokio::fs::remove_dir_all(root).await.unwrap();
    }

    #[test]
    fn multipart_ignores_boundary_prefix_inside_payload() {
        let body = b"--B\r\nContent-Disposition: form-data; name=\"file\"; filename=\"data.geojson\"\r\n\r\nfirst\r\n--Bprefix\r\nlast\r\n--B--\r\n";
        let (_, content) = multipart_file("multipart/form-data; boundary=B", body).unwrap();
        assert_eq!(content, b"first\r\n--Bprefix\r\nlast");
        let malformed = b"--BxxContent-Disposition: form-data; name=\"file\"; filename=\"x\"\r\n\r\nabc\r\n--B--\r\n";
        assert!(multipart_file("multipart/form-data; boundary=B", malformed).is_none());
    }

    #[test]
    fn multipart_extracts_only_the_file_part() {
        let body = b"--XyZ\r\nContent-Disposition: form-data; name=\"note\"\r\n\r\nhi\r\n\
--XyZ\r\nContent-Disposition: form-data; name=\"file\"; filename=\"a.geojson\"\r\n\
Content-Type: application/json\r\n\r\n{\"a\":1}\r\n--XyZ--\r\n";
        let (name, content) = multipart_file("multipart/form-data; boundary=XyZ", body).unwrap();
        assert_eq!(name, Some("a.geojson"));
        assert_eq!(content, b"{\"a\":1}");
        assert!(multipart_file("application/json", body).is_none());
        let field_only =
            b"--B\r\nContent-Disposition: form-data; name=\"file\"\r\n\r\nx\r\n--B--\r\n";
        assert!(multipart_file("multipart/form-data; boundary=B", field_only).is_none());
    }
}
