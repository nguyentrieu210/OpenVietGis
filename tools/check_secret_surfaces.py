"""Scan only explicitly selected synthetic-test text artifacts for credential leaks.

This is a defense-in-depth test checker, not a DLP or general repository scanner.
The caller must inventory the approved test outputs. Live secrets are not accepted
as CLI args, file paths and matching values are never emitted, and binary outputs
are rejected rather than silently reported as clean. Synthetic tests may supply
exact in-memory canaries to scan_text without persisting them in evidence files.
"""

from __future__ import annotations

import argparse
import html
import re
from bisect import bisect_left
from collections.abc import Iterable, Sequence
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
APPROVED_ROOTS = (
    Path("test-results"),
    Path("playwright-report"),
    Path("dist/security-evidence"),
)
TEXT_SUFFIXES = {".txt", ".log", ".json", ".jsonl", ".ndjson", ".xml", ".html", ".har"}
MAX_ARTIFACT_BYTES = 32 * 1024 * 1024

# Full generated values, not persisted digests, identifiers or 12-char safe prefixes.
_PATTERNS = {
    "application-session": re.compile(
        r"(?<![A-Za-z0-9_-])ovgs_[0-9a-fA-F]{32}\.s_[A-Za-z0-9_-]{43}(?![A-Za-z0-9_-])"
    ),
    "raw-session-secret": re.compile(
        r"(?<![A-Za-z0-9_-])s_[A-Za-z0-9_-]{43}(?![A-Za-z0-9_-])"
    ),
    "invitation-secret": re.compile(
        r"(?<![A-Za-z0-9_-])inv_[A-Za-z0-9_-]{43}(?![A-Za-z0-9_-])"
    ),
    "service-credential": re.compile(
        r"(?<![A-Za-z0-9_-])ovgsvc[0-9a-fA-F]{32}_[A-Za-z0-9_-]{43}(?![A-Za-z0-9_-])"
    ),
    "provider-jwt": re.compile(
        r"\beyJ[A-Za-z0-9_-]{5,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}(?![A-Za-z0-9_-])"
    ),
    "bearer-credential": re.compile(
        r"\bBearer\s+[A-Za-z0-9._~+/=-]{16,}", re.IGNORECASE
    ),
    "oidc-query-credential": re.compile(
        r"[?&](?:code|state|nonce|session_state|code_verifier|access_token|id_token|refresh_token)"
        r"=[A-Za-z0-9._~+/%=-]{8,}",
        re.IGNORECASE,
    ),
}

# Secret-bearing key/value surfaces (JSON, text headers, form/query fields).
# Match exact names: *_hash, *_prefix, and code_challenge are not credentials.
_SECRET_KEYS = (
    r"access_token|refresh_token|id_token|client_secret|code_verifier|"
    r"bootstrap_secret|invitation_secret|invite_secret|service_secret|session_secret|"
    r"csrf_secret|csrf_token|x-openvietgis-csrf|token|secret"
)
_COOKIE_KEYS = (
    r"ovg_session|ovg_oidc_state|AUTH_SESSION_ID|KC_RESTART|KEYCLOAK_IDENTITY|"
    r"KEYCLOAK_SESSION|KEYCLOAK_REMEMBER_ME"
)
_VALUE = r"[A-Za-z0-9._~+/%=-]{8,}"


def _key_value(keys: str) -> re.Pattern[str]:
    return re.compile(
        rf"(?<![A-Za-z0-9_-])(?:[\"']?)(?:{keys})(?:[\"']?)"
        rf"\s*[:=]\s*[\"']?(?P<value>{_VALUE})",
        re.IGNORECASE,
    )


def _header_array(keys: str) -> re.Pattern[str]:
    # Common HAR/JSON header representation, in which names and values are
    # distinct keys rather than a single object key.
    return re.compile(
        rf"[\"']name[\"']\s*:\s*[\"'](?:{keys})[\"']\s*,\s*"
        rf"[\"']value[\"']\s*:\s*[\"'](?P<value>{_VALUE})",
        re.IGNORECASE,
    )


_VALUE_PATTERNS = {
    "secret-field": _key_value(_SECRET_KEYS),
    "secret-cookie": _key_value(_COOKIE_KEYS),
    "secret-header-array": _header_array(_SECRET_KEYS + "|" + _COOKIE_KEYS),
    "authorization-credential": re.compile(
        r"[\"\']?authorization[\"\']?\s*:\s*[\"\']?(?:Bearer|Basic)\s+"
        rf"(?P<value>{_VALUE})",
        re.IGNORECASE,
    ),
    "authorization-header-array": re.compile(
        r"[\"\']name[\"\']\s*:\s*[\"\']authorization[\"\']\s*,\s*"
        r"[\"\']value[\"\']\s*:\s*[\"\'](?:Bearer|Basic)\s+"
        rf"(?P<value>{_VALUE})",
        re.IGNORECASE,
    ),
}
_REDACTED = {
    "redacted",
    "masked",
    "removed",
    "filtered",
    "sanitized",
    "omitted",
    "null",
}


@dataclass(frozen=True, slots=True)
class Finding:
    line: int
    category: str


class ArtifactError(ValueError):
    """Fixed-message scan failure; no path or file data belongs in the message."""


def scan_text(content: str, *, canaries: Iterable[str] = ()) -> tuple[Finding, ...]:
    exact = tuple(canaries)
    if any(not isinstance(canary, str) or len(canary) < 12 for canary in exact):
        raise ValueError(
            "in-memory synthetic canaries must have at least 12 characters"
        )
    findings: set[Finding] = set()
    # HTML reports and nested JSON often escape query separators or quotes.
    # Do not persist the decoded content. Line numbers refer to the scanned
    # representation (normally unchanged).
    for representation in {
        content,
        html.unescape(content).replace(r"\"", '"').replace(r"\/", "/"),
    }:
        newlines = [match.start() for match in re.finditer("\n", representation)]

        def record_at(
            index: int, category: str, newlines: list[int] = newlines
        ) -> None:
            findings.add(Finding(bisect_left(newlines, index) + 1, category))

        for category, pattern in _PATTERNS.items():
            for match in pattern.finditer(representation):
                record_at(match.start(), category)
        for category, pattern in _VALUE_PATTERNS.items():
            for match in pattern.finditer(representation):
                if match.group("value").lower() not in _REDACTED:
                    record_at(match.start(), category)
        for canary in exact:
            start = representation.find(canary)
            while start >= 0:
                record_at(start, "synthetic-canary")
                start = representation.find(canary, start + len(canary))
    return tuple(sorted(findings, key=lambda finding: (finding.line, finding.category)))


def _approved_artifact(root: Path, requested: str) -> Path:
    relative = Path(requested)
    if (
        not relative.parts
        or relative.is_absolute()
        or relative.drive
        or relative.anchor
        or any(
            part in {".", ".."} or part.startswith(".") or ":" in part
            for part in relative.parts
        )
    ):
        raise ArtifactError("artifact must be a visible repo-relative test output")
    if not any(relative.is_relative_to(base) for base in APPROVED_ROOTS):
        raise ArtifactError("artifact is outside approved test-output roots")
    if relative.suffix.lower() not in TEXT_SUFFIXES:
        raise ArtifactError("artifact is not a supported text output")

    # Refuse all symlink/junction components, even if they resolve inside the
    # repo. Do not let an artifact path redirect the scanner into user data.
    root = root.resolve(strict=True)
    path = root
    for part in relative.parts:
        path = path / part
        if path.is_symlink() or path.is_junction():
            raise ArtifactError("redirected artifact path is not allowed")
    try:
        resolved = path.resolve(strict=True)
        if not resolved.is_relative_to(root) or not resolved.is_file():
            raise ArtifactError("artifact must be a regular test-output file")
        if resolved.stat().st_size > MAX_ARTIFACT_BYTES:
            raise ArtifactError("artifact exceeds supported text scan size")
    except OSError:
        raise ArtifactError("artifact is missing or unreadable") from None
    return resolved


def scan_artifact(root: Path, requested: str) -> tuple[Finding, ...]:
    try:
        path = _approved_artifact(root, requested)
        with path.open("rb") as stream:
            raw = stream.read(MAX_ARTIFACT_BYTES + 1)
        if len(raw) > MAX_ARTIFACT_BYTES:
            raise ArtifactError("artifact exceeds supported text scan size")
        if b"\x00" in raw:
            raise ArtifactError("artifact is not supported UTF-8 text")
        return scan_text(raw.decode("utf-8-sig", errors="strict"))
    except ArtifactError:
        raise
    except (OSError, UnicodeError, ValueError):
        raise ArtifactError(
            "artifact is missing, unreadable, or not UTF-8 text"
        ) from None


class _SafeParser(argparse.ArgumentParser):
    def error(self, message: str) -> None:
        # argparse's default error echoes unknown argument values.
        self.exit(
            2, "secret-surface scan requires approved repo-relative artifact paths\n"
        )


def main(argv: Sequence[str] | None = None, *, root: Path = ROOT) -> int:
    parser = _SafeParser(description=__doc__)
    parser.add_argument("artifacts", nargs="+", metavar="TEST_ARTIFACT")
    args = parser.parse_args(argv)
    failed = False

    for index, requested in enumerate(args.artifacts, start=1):
        try:
            findings = scan_artifact(root, requested)
        except ArtifactError:
            failed = True
            print(f"SECRET SCAN ERROR: artifact #{index} rejected or unreadable")
            continue
        for finding in findings:
            failed = True
            print(
                f"SECRET SURFACE VIOLATION: artifact #{index}, line {finding.line}, "
                f"category {finding.category}"
            )
    if failed:
        return 1
    print(
        f"Secret surface scan: {len(args.artifacts)} explicitly selected text artifact(s) clean"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
