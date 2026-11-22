"""Unit-level authority envelope and fail-closed restore contracts."""

from __future__ import annotations

from copy import deepcopy
from uuid import UUID

import pytest

from tools.recovery_authority import reconciliation_sql, safe_human_authority


def _id(value: int) -> str:
    return str(UUID(int=value))


def _baseline() -> dict:
    return {
        "users": [{"id": _id(1), "links": ["a"]}, {"id": _id(2), "links": ["b"]}],
        "organizations": [
            {
                "id": _id(3),
                "workspaces": [[_id(4), "active"]],
                "projects": [[_id(5), _id(4), "active"]],
            },
            {"id": _id(6), "workspaces": [], "projects": [[_id(7), None, "active"]]},
        ],
        "memberships": [
            {
                "id": _id(8),
                "user_id": _id(1),
                "organization_id": _id(3),
                "role": "owner",
                "grants": [],
            },
            {
                "id": _id(9),
                "user_id": _id(2),
                "organization_id": _id(6),
                "role": "member",
                "grants": [[_id(10), "project", None, _id(7), "editor"]],
            },
        ],
    }


def test_default_is_deny_all() -> None:
    assert safe_human_authority(_baseline(), None) == ([], [])
    sql = reconciliation_sql([], [])
    assert "ARRAY[]::uuid[]" in sql
    assert "UPDATE identity.web_sessions SET revoked_at=now()" in sql
    assert "UPDATE org.service_credentials SET revoked_at=now()" in sql
    assert "UPDATE org.invitations SET status='revoked'" in sql
    assert "UPDATE field.offline_packages SET revoked_at=now()" in sql
    assert "RECOVERY_QUARANTINED" in sql
    assert "RAISE EXCEPTION 'Recovery reconciliation invariant failed'" in sql


def test_only_unchanged_human_authority_retained() -> None:
    historical = _baseline()
    users, members = safe_human_authority(historical, deepcopy(historical))
    assert users == [_id(1), _id(2)]
    assert members == [_id(8), _id(9)]


@pytest.mark.parametrize(
    "mutation",
    [
        "revoked",
        "downgrade",
        "grant_revoked",
        "grant_added",
        "scope_archived",
        "scope_changed",
    ],
)
def test_changed_or_revoked_scope_does_not_preserve_access(mutation: str) -> None:
    historical = _baseline()
    current = deepcopy(historical)
    if mutation == "revoked":
        current["memberships"].pop()
    elif mutation == "downgrade":
        current["memberships"][1]["role"] = "guest"
    elif mutation == "grant_revoked":
        current["memberships"][1]["grants"] = []
    elif mutation == "grant_added":
        current["memberships"][1]["grants"].append(
            [_id(11), "organization", None, None, "manager"]
        )
    elif mutation == "scope_archived":
        current["organizations"][1]["projects"][0][2] = "archived"
    elif mutation == "scope_changed":
        current["organizations"][1]["projects"][0][1] = _id(4)
    users, members = safe_human_authority(historical, current)
    assert users == [_id(1), _id(2)]
    assert members == [_id(8)]


@pytest.mark.parametrize("change", ["disabled", "unlink", "reassign"])
def test_missing_or_changed_identity_denied(change: str) -> None:
    historical = _baseline()
    current = deepcopy(historical)
    if change == "disabled":
        current["users"].pop()
    elif change == "unlink":
        current["users"][1]["links"] = []
    else:
        current["users"][1]["links"] = ["new"]
    users, members = safe_human_authority(historical, current)
    assert users == [_id(1)]
    assert members == [_id(8)]


def test_current_only_grant_or_user_not_imported() -> None:
    historical = _baseline()
    current = deepcopy(historical)
    current["users"].append({"id": _id(12), "links": ["new"]})
    current["memberships"].append(
        {
            "id": _id(13),
            "user_id": _id(12),
            "organization_id": _id(3),
            "role": "owner",
            "grants": [],
        }
    )
    assert safe_human_authority(historical, current) == (
        [_id(1), _id(2)],
        [_id(8), _id(9)],
    )


@pytest.mark.parametrize("current_policy", [None, "changed-policy"])
def test_changed_or_missing_gis_policy_quarantines_all(
    current_policy: str | None,
) -> None:
    historical = _baseline() | {"gis_access": "old-policy"}
    current = _baseline()
    if current_policy is not None:
        current["gis_access"] = current_policy
    assert safe_human_authority(historical, current) == ([], [])
    assert safe_human_authority(current, historical) == ([], [])
    assert safe_human_authority(historical, deepcopy(historical)) == (
        [_id(1), _id(2)],
        [_id(8), _id(9)],
    )


def test_duplicate_invalid_ids_fail_closed() -> None:
    current = _baseline()
    current["users"].append(deepcopy(current["users"][0]))
    with pytest.raises(RuntimeError, match="Duplicate authority"):
        safe_human_authority(_baseline(), current)
    with pytest.raises(ValueError):
        reconciliation_sql(["'; DELETE FROM identity.users; --"], [])
