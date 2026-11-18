from __future__ import annotations

import json
from pathlib import Path

import pytest

from tools.check_secret_surfaces import (
    ArtifactError,
    Finding,
    main,
    scan_artifact,
    scan_text,
)

# Deliberately synthetic values, never real credentials or fixture passwords.
CANARY = "test-canary-unique-not-valid-auth"
SESSION = "ovgs_" + ("a" * 32) + ".s_" + ("A" * 42) + "-"
INVITATION = "inv_" + ("B" * 42) + "-"
JWT = "eyJsynthetic." + ("C" * 12) + "." + ("D" * 11) + "-"


@pytest.mark.parametrize(
    ("sample", "expected"),
    [
        (f"session {SESSION}", "application-session"),
        (f"raw {SESSION.split('.')[1]}", "raw-session-secret"),
        (f"invite {INVITATION}", "invitation-secret"),
        (f"E       assert '{INVITATION}' not in raw_metadata", "invitation-secret"),
        (f"E       +  where {SESSION} = cookie_value", "application-session"),
        (f"provider {JWT}", "provider-jwt"),
        ("service ovgsvc" + "a" * 32 + "_" + "Z" * 43, "service-credential"),
        (f"Authorization: Bearer {CANARY}", "authorization-credential"),
        ("Authorization: Basic c3ludGhldGljOnRlc3Q=", "authorization-credential"),
        (
            f'{{"name":"authorization","value":"Bearer {CANARY}"}}',
            "authorization-header-array",
        ),
        (f'{{"access_token":"{CANARY}"}}', "secret-field"),
        (f'{{"refresh_token":"{CANARY}"}}', "secret-field"),
        (f'{{"client_secret":"{CANARY}"}}', "secret-field"),
        (f'{{"code_verifier":"{CANARY}"}}', "secret-field"),
        (f'{{"csrf_token":"{CANARY}"}}', "secret-field"),
        (f'{{"token":"{CANARY}"}}', "secret-field"),
        (f"Set-Cookie: ovg_oidc_state={CANARY}; HttpOnly", "secret-cookie"),
        (f"Cookie: AUTH_SESSION_ID={CANARY}", "secret-cookie"),
        (
            f'{{"name":"x-openvietgis-csrf","value":"{CANARY}"}}',
            "secret-header-array",
        ),
        ("GET /api/v1/auth/callback/keycloak?code=" + CANARY, "oidc-query-credential"),
        ("Location: /callback?foo=1&amp;state=" + CANARY, "oidc-query-credential"),
        (
            json.dumps({"message": json.dumps({"refresh_token": CANARY})}),
            "secret-field",
        ),
    ],
)
def test_detects_synthetic_credential_shapes(sample: str, expected: str) -> None:
    assert expected in {finding.category for finding in scan_text(sample)}


def test_exact_canaries_are_found_without_key_names() -> None:
    findings = scan_text(f"first line\nmessage = {CANARY}", canaries=(CANARY,))
    assert findings == (Finding(2, "synthetic-canary"),)
    assert CANARY not in repr(findings)


def test_does_not_flag_hashes_safe_prefixes_or_redacted_values() -> None:
    sample = (
        '{"token_hash":"' + ("a" * 64) + '", "token_prefix":"' + ('inv_' + '12345678') + '", '
        '"provider_subject":"synthetic-subject", "secret":"[REDACTED]", '
        '"id_token":"redacted", "password_redacted": true}\n'
        "location: /auth?code_challenge=synthetic-public-challenge\n"
        "set-cookie: ovg_session=; Path=/; HttpOnly\n"
        '{"name":"x-openvietgis-csrf","value":"[REDACTED]"}\n'
    )
    assert scan_text(sample) == ()


def _artifact(root: Path, relative: str, content: str | bytes) -> Path:
    path = root / relative
    path.parent.mkdir(parents=True, exist_ok=True)
    if isinstance(content, str):
        path.write_text(content, encoding="utf-8")
    else:
        path.write_bytes(content)
    return path


def test_cli_scans_only_selected_artifacts_and_never_prints_value_or_path(
    tmp_path: Path,
    capsys: pytest.CaptureFixture[str],
) -> None:
    selected = f"test-results/{CANARY}.log"
    _artifact(tmp_path, selected, f"1\nAuthorization: Bearer {CANARY}\n")
    _artifact(tmp_path, "test-results/not-selected.log", f"secret={CANARY}")
    assert main([selected], root=tmp_path) == 1
    output = capsys.readouterr()
    assert "artifact #1, line 2" in output.out
    assert "authorization-credential" in output.out
    assert selected not in output.out
    assert CANARY not in output.out + output.err


def test_clean_explicit_artifact_does_not_scan_unrelated_data(
    tmp_path: Path,
    capsys: pytest.CaptureFixture[str],
) -> None:
    _artifact(tmp_path, "test-results/clean.log", "synthetic job succeeded\n")
    _artifact(tmp_path, ".env", f"secret={CANARY}")
    _artifact(tmp_path, "test-results/not-selected.log", f"secret={CANARY}")
    assert main(["test-results/clean.log"], root=tmp_path) == 0
    assert "1 explicitly selected text artifact(s) clean" in capsys.readouterr().out


@pytest.mark.parametrize(
    "relative",
    [
        ".env",
        "private.log",
        "test-results/.env.log",
        "test-results/../private.log",
        "dist/toolchains/fixture.log",
        "test-results/not-supported.zip",
        "test-results/missing.txt",
        "test-results/fixture.log:stream.log",
    ],
)
def test_rejects_unapproved_or_missing_paths(tmp_path: Path, relative: str) -> None:
    with pytest.raises(ArtifactError):
        scan_artifact(tmp_path, relative)


def test_rejects_absolute_path_even_if_it_points_inside_approved_root(
    tmp_path: Path,
) -> None:
    path = _artifact(tmp_path, "test-results/clean.txt", "")
    with pytest.raises(ArtifactError):
        scan_artifact(tmp_path, str(path))


def test_rejects_link_into_private_tree(tmp_path: Path) -> None:
    source = _artifact(tmp_path, "private.txt", f"secret={CANARY}")
    link = tmp_path / "test-results" / "linked.txt"
    link.parent.mkdir()
    try:
        link.symlink_to(source)
    except OSError:
        pytest.skip("host does not allow creation of test-only symlinks")
    with pytest.raises(ArtifactError):
        scan_artifact(tmp_path, "test-results/linked.txt")


@pytest.mark.parametrize("raw", [b"\xff\xff", b"a\x00b"])
def test_rejects_unreadable_text_encoding(
    tmp_path: Path,
    capsys: pytest.CaptureFixture[str],
    raw: bytes,
) -> None:
    _artifact(tmp_path, "playwright-report/result.txt", raw)
    assert main(["playwright-report/result.txt"], root=tmp_path) == 1
    assert "rejected or unreadable" in capsys.readouterr().out


def test_read_error_is_fail_closed_and_never_echoes_exception(
    tmp_path: Path,
    capsys: pytest.CaptureFixture[str],
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    _artifact(tmp_path, "dist/security-evidence/api.log", "")

    def fail_open(*args: object, **kwargs: object) -> None:
        raise PermissionError(CANARY)

    monkeypatch.setattr(Path, "open", fail_open)
    assert main(["dist/security-evidence/api.log"], root=tmp_path) == 1
    output = capsys.readouterr()
    assert "rejected or unreadable" in output.out
    assert CANARY not in output.out + output.err


def test_cli_rejects_empty_or_invalid_args_without_echo(
    capsys: pytest.CaptureFixture[str],
) -> None:
    with pytest.raises(SystemExit) as error:
        main(["--" + CANARY])
    assert error.value.code == 2
    output = capsys.readouterr()
    assert CANARY not in output.out + output.err
