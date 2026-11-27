//! Bounded OIDC discovery, token exchange, and ID token verification worker.

use std::process::Stdio;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct OidcMetadata {
    pub(crate) authorization_endpoint: String,
    pub(crate) token_endpoint: String,
    pub(crate) jwks_uri: String,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct VerifiedOidcClaims {
    pub(crate) issuer: String,
    pub(crate) subject: String,
    pub(crate) display_name: String,
    pub(crate) email: Option<String>,
    pub(crate) email_verified: bool,
}

#[derive(Debug)]
pub(crate) enum OidcWorkerError {
    InvalidResponse(String),
    Internal(String),
}

#[derive(Deserialize)]
struct WorkerReply<T> {
    ok: bool,
    error: Option<String>,
    data: Option<T>,
}

const WORKER_TIMEOUT: Duration = Duration::from_secs(15);

const WORKER_SCRIPT: &str = r#"
import base64
import hmac
import hashlib
import json
import os
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request


def b64u_decode(data: str) -> bytes:
    pad = (-len(data)) % 4
    return base64.urlsafe_b64decode(data + ("=" * pad))


def der_len(n: int) -> bytes:
    if n < 0x80:
        return bytes([n])
    raw = n.to_bytes((n.bit_length() + 7) // 8, "big")
    return bytes([0x80 | len(raw)]) + raw


def der_tlv(tag: int, val: bytes) -> bytes:
    return bytes([tag]) + der_len(len(val)) + val


def der_int(raw: bytes) -> bytes:
    stripped = raw.lstrip(b"\x00") or b"\x00"
    if stripped[0] & 0x80:
        stripped = b"\x00" + stripped
    return der_tlv(0x02, stripped)


def rsa_spki_pem(n_b64: str, e_b64: str) -> bytes:
    n_der = der_int(b64u_decode(n_b64))
    e_der = der_int(b64u_decode(e_b64))
    pub_seq = der_tlv(0x30, n_der + e_der)
    # rsaEncryption OID 1.2.840.113549.1.1.1 + NULL
    alg_id = der_tlv(0x30, b"\x06\x09\x2a\x86\x48\x86\xf7\x0d\x01\x01\x01\x05\x00")
    bit_str = der_tlv(0x03, b"\x00" + pub_seq)
    spki = der_tlv(0x30, alg_id + bit_str)
    b64 = base64.encodebytes(spki).decode("ascii")
    return f"-----BEGIN PUBLIC KEY-----\n{b64}-----END PUBLIC KEY-----\n".encode("ascii")


def ec_spki_pem(crv: str, x_b64: str, y_b64: str) -> tuple[bytes, int]:
    if crv == "P-256":
        coord_len = 32
        crv_oid = b"\x06\x08\x2a\x86\x48\xce\x3d\x03\x01\x07"
    elif crv == "P-384":
        coord_len = 48
        crv_oid = b"\x06\x05\x2b\x81\x04\x00\x22"
    else:
        raise ValueError(f"unsupported EC curve: {crv}")
    x = b64u_decode(x_b64).rjust(coord_len, b"\x00")
    y = b64u_decode(y_b64).rjust(coord_len, b"\x00")
    ec_oid = b"\x06\x07\x2a\x86\x48\xce\x3d\x02\x01"
    alg_id = der_tlv(0x30, ec_oid + crv_oid)
    point = b"\x04" + x + y
    bit_str = der_tlv(0x03, b"\x00" + point)
    spki = der_tlv(0x30, alg_id + bit_str)
    b64 = base64.encodebytes(spki).decode("ascii")
    return (
        f"-----BEGIN PUBLIC KEY-----\n{b64}-----END PUBLIC KEY-----\n".encode("ascii"),
        coord_len,
    )


def verify_openssl(pem: bytes, digest_flag: str, signing_input: bytes, der_sig: bytes) -> bool:
    with tempfile.TemporaryDirectory() as tmp:
        pem_path = os.path.join(tmp, "key.pem")
        sig_path = os.path.join(tmp, "sig.bin")
        with open(pem_path, "wb") as fh:
            fh.write(pem)
        with open(sig_path, "wb") as fh:
            fh.write(der_sig)
        proc = subprocess.run(
            ["/usr/bin/openssl", "dgst", digest_flag, "-verify", pem_path, "-signature", sig_path],
            input=signing_input,
            capture_output=True,
            timeout=5,
        )
        return proc.returncode == 0


def verify_jws_signature(jwk: dict, alg: str, signing_input: bytes, sig: bytes) -> bool:
    if alg in {"RS256", "RS384", "RS512"}:
        if jwk.get("kty") != "RSA":
            return False
        digest_flag = {"RS256": "-sha256", "RS384": "-sha384", "RS512": "-sha512"}[alg]
        pem = rsa_spki_pem(jwk["n"], jwk["e"])
        return verify_openssl(pem, digest_flag, signing_input, sig)
    if alg in {"ES256", "ES384"}:
        if jwk.get("kty") != "EC":
            return False
        expected_crv, digest_flag = ("P-256", "-sha256") if alg == "ES256" else ("P-384", "-sha384")
        if jwk.get("crv") != expected_crv:
            return False
        pem, coord_len = ec_spki_pem(expected_crv, jwk["x"], jwk["y"])
        if len(sig) != coord_len * 2:
            return False
        r_der = der_int(sig[:coord_len])
        s_der = der_int(sig[coord_len:])
        der_sig = der_tlv(0x30, r_der + s_der)
        return verify_openssl(pem, digest_flag, signing_input, der_sig)
    return False


def fetch_json(url: str, data: bytes | None = None, headers: dict[str, str] | None = None) -> dict:
    req_headers = {"Accept": "application/json"}
    if headers:
        req_headers.update(headers)
    req = urllib.request.Request(url, data=data, headers=req_headers)
    with urllib.request.urlopen(req, timeout=8) as resp:
        return json.loads(resp.read().decode("utf-8"))


def op_discover(req: dict) -> dict:
    issuer = req["issuer"].rstrip("/")
    doc = fetch_json(f"{issuer}/.well-known/openid-configuration")
    for key in ("authorization_endpoint", "token_endpoint", "jwks_uri"):
        if not isinstance(doc.get(key), str) or not doc[key]:
            raise ValueError(f"missing {key}")
    return {
        "authorization_endpoint": doc["authorization_endpoint"],
        "token_endpoint": doc["token_endpoint"],
        "jwks_uri": doc["jwks_uri"],
    }


def op_exchange(req: dict) -> dict:
    form = {
        "grant_type": "authorization_code",
        "code": req["code"],
        "redirect_uri": req["redirect_uri"],
        "code_verifier": req["code_verifier"],
    }
    headers = {"Content-Type": "application/x-www-form-urlencoded"}
    secret = req.get("client_secret")
    if secret:
        raw_creds = (
            urllib.parse.quote(req["client_id"], safe="")
            + ":"
            + urllib.parse.quote(secret, safe="")
        )
        headers["Authorization"] = "Basic " + base64.b64encode(raw_creds.encode("utf-8")).decode("ascii")
    else:
        form["client_id"] = req["client_id"]

    body = urllib.parse.urlencode(form).encode("utf-8")
    token_resp = fetch_json(req["token_endpoint"], data=body, headers=headers)
    id_token = token_resp.get("id_token")
    if not isinstance(id_token, str) or not id_token:
        raise ValueError("missing id_token")

    parts = id_token.split(".")
    if len(parts) != 3:
        raise ValueError("malformed JWT")
    header = json.loads(b64u_decode(parts[0]).decode("utf-8"))
    claims = json.loads(b64u_decode(parts[1]).decode("utf-8"))
    sig = b64u_decode(parts[2])
    signing_input = f"{parts[0]}.{parts[1]}".encode("ascii")

    alg = header.get("alg")
    if not isinstance(alg, str) or alg == "none":
        raise ValueError("invalid JWT alg")

    if alg in {"HS256", "HS384", "HS512"}:
        if not secret:
            raise ValueError("missing client_secret for HS*")
        digestmod = {"HS256": hashlib.sha256, "HS384": hashlib.sha384, "HS512": hashlib.sha512}[alg]
        expected_sig = hmac.new(secret.encode("utf-8"), signing_input, digestmod).digest()
        if not hmac.compare_digest(expected_sig, sig):
            raise ValueError("invalid HMAC ID token signature")
    else:
        jwks = fetch_json(req["jwks_uri"])
        keys = jwks.get("keys")
        if not isinstance(keys, list):
            raise ValueError("invalid JWKS")
        kid = header.get("kid")
        candidates = [
            k for k in keys
            if isinstance(k, dict)
            and k.get("use", "sig") == "sig"
            and (kid is None or k.get("kid") == kid)
        ]
        if not any(verify_jws_signature(k, alg, signing_input, sig) for k in candidates):
            raise ValueError("invalid ID token signature")

    now = time.time()
    exp = claims.get("exp")
    if not isinstance(exp, (int, float)) or now > float(exp) + 60:
        raise ValueError("expired ID token")
    nbf = claims.get("nbf")
    if nbf is not None and (not isinstance(nbf, (int, float)) or now + 60 < float(nbf)):
        raise ValueError("ID token not yet valid")

    if claims.get("iss") != req["expected_issuer"]:
        raise ValueError("invalid provider issuer")
    sub = claims.get("sub")
    if not isinstance(sub, str) or not sub:
        raise ValueError("invalid provider subject")
    aud = claims.get("aud")
    client_id = req["client_id"]
    if isinstance(aud, str):
        if aud != client_id:
            raise ValueError("invalid aud")
    elif isinstance(aud, list):
        if client_id not in aud:
            raise ValueError("invalid aud")
        if len(aud) > 1 and claims.get("azp") not in (None, client_id):
            raise ValueError("invalid azp")
    else:
        raise ValueError("missing aud")
    if claims.get("nonce") != req["expected_nonce"]:
        raise ValueError("invalid nonce")

    email = claims.get("email")
    display_name = claims.get("name") or claims.get("preferred_username") or email or sub
    return {
        "issuer": req["expected_issuer"],
        "subject": sub,
        "display_name": str(display_name),
        "email": str(email) if email is not None else None,
        "email_verified": claims.get("email_verified") is True,
    }


def main() -> None:
    try:
        req = json.loads(sys.stdin.read())
        op = req.get("op")
        if op == "discover":
            data = op_discover(req)
        elif op == "exchange":
            data = op_exchange(req)
        else:
            raise ValueError(f"unsupported op: {op}")
        sys.stdout.write(json.dumps({"ok": True, "data": data}))
    except Exception as exc:
        sys.stdout.write(json.dumps({"ok": False, "error": str(exc)}))


if __name__ == "__main__":
    main()
"#;

pub(crate) async fn discover(issuer: &str) -> Result<OidcMetadata, OidcWorkerError> {
    let payload = serde_json::json!({
        "op": "discover",
        "issuer": issuer,
    });
    run_worker(&payload).await
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn exchange_and_verify(
    code: &str,
    redirect_uri: &str,
    code_verifier: &str,
    client_id: &str,
    client_secret: Option<&str>,
    token_endpoint: &str,
    jwks_uri: &str,
    expected_issuer: &str,
    expected_nonce: &str,
) -> Result<VerifiedOidcClaims, OidcWorkerError> {
    let payload = serde_json::json!({
        "op": "exchange",
        "code": code,
        "redirect_uri": redirect_uri,
        "code_verifier": code_verifier,
        "client_id": client_id,
        "client_secret": client_secret,
        "token_endpoint": token_endpoint,
        "jwks_uri": jwks_uri,
        "expected_issuer": expected_issuer,
        "expected_nonce": expected_nonce,
    });
    run_worker(&payload).await
}

async fn run_worker<T: for<'de> Deserialize<'de>>(
    payload: &serde_json::Value,
) -> Result<T, OidcWorkerError> {
    let input = serde_json::to_vec(payload)
        .map_err(|err| OidcWorkerError::Internal(format!("serialize oidc request: {err}")))?;

    let mut child = Command::new("/usr/bin/python3")
        .args(["-E", "-s", "-B", "-c", WORKER_SCRIPT])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|err| OidcWorkerError::Internal(format!("spawn oidc worker: {err}")))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(&input)
            .await
            .map_err(|err| OidcWorkerError::Internal(format!("write oidc stdin: {err}")))?;
    }

    let output = tokio::time::timeout(WORKER_TIMEOUT, child.wait_with_output())
        .await
        .map_err(|_| OidcWorkerError::InvalidResponse("OIDC worker timed out".to_string()))?
        .map_err(|err| OidcWorkerError::Internal(format!("wait oidc worker: {err}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(OidcWorkerError::InvalidResponse(format!(
            "OIDC worker failed: {stderr}"
        )));
    }

    let reply: WorkerReply<T> = serde_json::from_slice(&output.stdout).map_err(|err| {
        OidcWorkerError::InvalidResponse(format!("invalid OIDC worker JSON: {err}"))
    })?;

    if !reply.ok {
        return Err(OidcWorkerError::InvalidResponse(
            reply
                .error
                .unwrap_or_else(|| "invalid OIDC response".to_string()),
        ));
    }

    reply
        .data
        .ok_or_else(|| OidcWorkerError::InvalidResponse("missing OIDC worker payload".to_string()))
}
