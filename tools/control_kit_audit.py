"""Read-only OpenVietGIS release/control-plane audit.

Validates the pinned git source, published release manifest and frontend bits.
Does not claim to certify schema migrations, OAuth, real GIS workflows or tests.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
from pathlib import Path
from typing import Any
from urllib.error import URLError
from urllib.request import Request, urlopen


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def git(repo: Path, *args: str) -> str:
    result = subprocess.run(
        ["git", "-C", str(repo), *args], check=True, text=True, capture_output=True
    )
    return result.stdout.strip()


def check_assets(web_dir: Path, web_files: dict[str, str]) -> list[str]:
    errors: list[str] = []
    root = web_dir.resolve()
    for relative, expected in web_files.items():
        path = (root / relative).resolve()
        if not path.is_relative_to(root):
            errors.append(f"Asset escapes web root: {relative}")
        elif not path.is_file():
            errors.append(f"Missing asset: {relative}")
        elif sha256(path) != expected:
            errors.append(f"Asset checksum mismatch: {relative}")
    return errors


def strip_cloudflare_beacon(html: bytes) -> bytes:
    """Ignore only the known Cloudflare-injected analytics script.

    Compare every other byte, including the shipped bundle references.
    """
    pattern = rb'(?:\r?\n)?<script\b(?=[^>]*\bsrc=["\']https://static\.cloudflareinsights\.com/beacon\.min\.js/)[^>]*>\s*</script>(?:\r?\n)?'
    return re.sub(pattern, b"", html)


def check_index_assets(web_dir: Path) -> list[str]:
    index = web_dir / "index.html"
    if not index.is_file():
        return ["Missing index.html"]
    content = index.read_text(encoding="utf-8")
    paths = re.findall(
        r"""(?:src|href)=["'](/rewrite/(?:assets|brand)/[^"']+)["']""", content
    )
    errors = []
    for path in paths:
        target = (web_dir / path.removeprefix("/rewrite/")).resolve()
        if not target.is_relative_to(web_dir.resolve()) or not target.is_file():
            errors.append(f"HTML references unavailable file: {path}")
    return errors


def audit(
    repo: Path,
    release_root: Path,
    public_root: Path,
    service: str | None = None,
    public_url: str | None = None,
) -> dict[str, Any]:
    rows: list[dict[str, str]] = []

    def record(code: str, outcome: str, detail: str) -> None:
        rows.append({"check": code, "outcome": outcome, "detail": detail})

    head = git(repo, "rev-parse", "refs/heads/main")
    remote = git(repo, "rev-parse", "refs/remotes/origin/main")
    record(
        "github_main_vs_local_main",
        "PASS" if head == remote else "FAIL",
        f"local main={head[:12]}, origin/main cached={remote[:12]} (not a fresh fetch)",
    )
    dirty = git(repo, "status", "--porcelain")
    record(
        "working_tree",
        "PASS" if not dirty else "WARN",
        "clean" if not dirty else f"{len(dirty.splitlines())} changed paths",
    )
    nonmerged = [
        x.strip()
        for x in git(repo, "branch", "-r", "--no-merged", "main").splitlines()
        if x.strip() and "->" not in x
    ]
    record(
        "remote_branches_unmerged",
        "WARN" if nonmerged else "PASS",
        f"{len(nonmerged)} cached remote branches not merged into main; review, do not delete",
    )
    record(
        "duplicate_frontends",
        "WARN"
        if (repo / "apps/web").is_dir() and (repo / "apps/web-next").is_dir()
        else "PASS",
        "apps/web and apps/web-next present"
        if (repo / "apps/web").is_dir() and (repo / "apps/web-next").is_dir()
        else "single frontend directory",
    )

    link = public_root / "rewrite"
    if not link.is_symlink():
        record("public_release_link", "FAIL", f"Missing release symlink: {link}")
        return {
            "source_sha": head,
            "checks": rows,
            "nonmerged_remote_branches": nonmerged,
        }
    web_dir = link.resolve()
    release_id = web_dir.name
    record("public_release_link", "PASS", f"{link} -> {web_dir}")
    release_dir = release_root / release_id
    manifest_path = release_dir / "RELEASE_MANIFEST.json"
    if not manifest_path.is_file():
        record("release_manifest", "FAIL", f"Missing {manifest_path}")
        return {
            "source_sha": head,
            "checks": rows,
            "nonmerged_remote_branches": nonmerged,
        }
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    manifest_sha = manifest.get("git_commit", "")
    source_marker = release_dir / "RELEASE_COMMIT"
    marker_sha = (
        source_marker.read_text(encoding="utf-8").strip()
        if source_marker.exists()
        else ""
    )
    consistent = (
        manifest_sha == marker_sha == head and manifest.get("release_id") == release_id
    )
    record(
        "release_source_pin",
        "PASS" if consistent else "FAIL",
        f"manifest={str(manifest_sha)[:12]}, marker={marker_sha[:12]}, main={head[:12]}, release={release_id}",
    )
    record(
        "release_frontend",
        "PASS" if manifest.get("frontend") == "apps/web-next" else "WARN",
        f"declared frontend: {manifest.get('frontend')}",
    )
    files = manifest.get("web_files", {})
    if not isinstance(files, dict):
        record("release_web_hashes", "FAIL", "web_files entry invalid")
    else:
        errors = check_assets(web_dir, files)
        record(
            "release_web_hashes",
            "FAIL" if errors else "PASS",
            f"{len(files)} checked; "
            + ("; ".join(errors[:10]) if errors else "all hashes match"),
        )
    index_errors = check_index_assets(web_dir)
    record(
        "release_html_assets",
        "FAIL" if index_errors else "PASS",
        "; ".join(index_errors[:10])
        if index_errors
        else "HTML entrypoint references resolve",
    )

    backend_path = manifest.get("backend", "")
    if backend_path == "apps/api":
        source_api = repo / "apps/api/openvietgis_api/main.py"
        release_api = release_dir / "apps/api/openvietgis_api/main.py"
        both_match = (
            source_api.is_file()
            and release_api.is_file()
            and sha256(source_api) == sha256(release_api)
        )
        record(
            "release_api_source",
            "PASS" if both_match else "FAIL",
            "main.py matches source main"
            if both_match
            else "main.py differs from source main",
        )
    else:
        record(
            "release_api_source", "WARN", f"Unexpected backend path {backend_path!r}"
        )

    if service:
        result = subprocess.run(
            ["systemctl", "show", service, "-p", "WorkingDirectory", "--value"],
            text=True,
            capture_output=True,
            check=False,
        )
        running = subprocess.run(
            ["systemctl", "is-active", service],
            text=True,
            capture_output=True,
            check=False,
        )
        expected = str(release_dir.resolve())
        correct = (
            result.returncode == 0
            and result.stdout.strip() == expected
            and running.stdout.strip() == "active"
        )
        record(
            "runtime_service_pinned",
            "PASS" if correct else "FAIL",
            f"{service}: cwd={result.stdout.strip()}, expected={expected}, active={running.stdout.strip()}",
        )
    else:
        record(
            "runtime_service_pinned",
            "NOT_VERIFIED",
            "Pass --service to check API deployment",
        )

    if public_url:
        base = public_url.rstrip("/")
        for route in ("/", "/app"):
            try:
                with urlopen(
                    Request(
                        base + route,
                        headers={"User-Agent": "OpenVietGIS-ControlKit/1.0"},
                    ),
                    timeout=8,
                ) as response:
                    data = response.read()
                    expected_html = (web_dir / "index.html").read_bytes()
                    remote_html = strip_cloudflare_beacon(data) == expected_html
                    record(
                        "public_web" + route,
                        "PASS" if remote_html else "FAIL",
                        f"HTTP {response.status}; release index after optional Cloudflare beacon={'yes' if remote_html else 'no'}",
                    )
            except (OSError, URLError) as error:
                record("public_web" + route, "FAIL", str(error))
    else:
        record("public_web", "NOT_VERIFIED", "Pass --public-url to compare public HTML")

    record(
        "database_migration_runtime",
        "NOT_VERIFIED",
        "Database schema migration/version is not checked by this read-only audit",
    )
    record(
        "functional_acceptance",
        "NOT_VERIFIED",
        "Functional browser, permissions and GIS workflow tests must run separately",
    )
    return {
        "source_sha": head,
        "release_id": release_id,
        "nonmerged_remote_branches": nonmerged,
        "checks": rows,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, default=Path("/opt/openvietgis"))
    parser.add_argument(
        "--release-root", type=Path, default=Path("/opt/openvietgis-releases")
    )
    parser.add_argument(
        "--public-root", type=Path, default=Path("/var/www/openvietgis-hn019")
    )
    parser.add_argument("--service", default=None)
    parser.add_argument("--public-url", default=None)
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()
    report = audit(
        args.repo, args.release_root, args.public_root, args.service, args.public_url
    )
    if args.json:
        print(json.dumps(report, ensure_ascii=False, indent=2))
    else:
        for row in report["checks"]:
            print(f"[{row['outcome']:<12}] {row['check']}: {row['detail']}")
        print("NOTE: PASS is structural alignment only, not production acceptance.")
    return 1 if any(row["outcome"] == "FAIL" for row in report["checks"]) else 0


if __name__ == "__main__":
    raise SystemExit(main())
