"""Read-only HTTPS smoke for an isolated, unaccepted self-host Compose fixture.

No credentials, writes, redirects or public-network targets. This verifies
packaging and dependency behavior only; it cannot confer release acceptance.
"""

from __future__ import annotations

import argparse
import re
import ssl
from pathlib import Path
from urllib.parse import urlsplit

import httpx

_ENTRY = re.compile(r'src="(/assets/index-[A-Za-z0-9_-]+\.js)"')
_WORKER = re.compile(r"(/assets/maplibre-gl-worker-[A-Za-z0-9_-]+\.mjs)")


def check_origin(origin: str) -> str:
    parsed = urlsplit(origin)
    if (
        parsed.scheme != "https"
        or parsed.hostname not in ("localhost", "127.0.0.1", "::1")
        or parsed.username is not None
        or parsed.password is not None
        or parsed.port is None
        or parsed.path not in ("", "/")
        or parsed.query
        or parsed.fragment
    ):
        raise ValueError("Expected an explicit loopback HTTPS fixture origin with port")
    return origin.rstrip("/")


def check(client: httpx.Client, origin: str) -> None:
    def get(path: str, expected_status: int = 200) -> httpx.Response:
        response = client.get(path)
        if response.status_code != expected_status:
            raise RuntimeError(
                f"{path}: expected {expected_status}, got {response.status_code}"
            )
        return response

    for page in ("/", "/app", "/rewrite/?live=1"):
        html = get(page).text
        if "OpenVietGIS" not in html or not _ENTRY.search(html):
            raise RuntimeError(f"{page}: missing Studio entry bundle")
    entry = _ENTRY.search(get("/").text)
    if entry is None:
        raise RuntimeError("Studio bundle missing")
    bundle = get(entry.group(1))
    if "javascript" not in bundle.headers.get("content-type", ""):
        raise RuntimeError("Studio bundle is not JavaScript")
    worker = _WORKER.search(bundle.text)
    if worker is None:
        raise RuntimeError("MapLibre module worker missing from the Studio bundle")
    if "javascript" not in get(worker.group(1)).headers.get("content-type", ""):
        raise RuntimeError("MapLibre module worker must be served as JavaScript")
    if (
        not get("/brand/ovg-mark.png")
        .headers.get("content-type", "")
        .startswith("image/")
    ):
        raise RuntimeError("Studio branding asset missing or invalid")
    get("/assets/nonexistent-release-fixture.js", 404)
    get("/brand/nonexistent-release-fixture.png", 404)
    get("/api/v1/me", 401)
    ready = get("/api/v1/ops/health/ready").json()
    if (
        ready.get("status") != "ready"
        or ready.get("scope") != "dependency_probe_only"
        or ready.get("production_ready") is not False
    ):
        raise RuntimeError("Unexpected dependency or release-reporting state")
    releases = get("/api/v1/ops/release-capabilities").json()
    if (
        releases.get("total_releases") != 15
        or releases.get("accepted_release_count") != 0
        or releases.get("production_ready") is not False
    ):
        raise RuntimeError("Unaccepted fixture must not advertise certified releases")
    for test_origin, expected_status in (
        (origin, 200),
        ("https://foreign.invalid", 400),
    ):
        response = client.options(
            "/api/v1/me",
            headers={"Origin": test_origin, "Access-Control-Request-Method": "POST"},
        )
        if response.status_code != expected_status:
            raise RuntimeError("Unexpected CORS status")
        if (
            expected_status == 200
            and response.headers.get("access-control-allow-origin") != origin
        ):
            raise RuntimeError("Configured origin not permitted")
        if expected_status == 400 and "access-control-allow-origin" in response.headers:
            raise RuntimeError("Foreign origin unexpectedly permitted")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--origin", required=True)
    parser.add_argument("--ca-cert", type=Path, required=True)
    args = parser.parse_args()
    origin = check_origin(args.origin)
    tls = ssl.create_default_context(cafile=str(args.ca_cert))
    with httpx.Client(
        base_url=origin, verify=tls, follow_redirects=False, trust_env=False, timeout=20
    ) as client:
        check(client, origin)
    print(
        "Self-host fixture HTTPS, Studio assets/worker, CORS, readiness and release truth: PASS"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
