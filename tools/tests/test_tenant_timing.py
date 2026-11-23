import base64
import json

import pytest

from tools.check_tenant_timing import (
    BOOTSTRAP_LIMITS,
    P1_LIMITS,
    TimingEvidenceError,
    collect,
)


def _attachment(name: str, payload: dict[str, object]) -> dict[str, str]:
    return {
        "name": name,
        "contentType": "application/json",
        "body": base64.b64encode(json.dumps(payload).encode()).decode(),
    }


def _report(attachments: list[dict[str, str]], status: str = "passed") -> dict:
    return {
        "suites": [
            {
                "suites": [
                    {
                        "specs": [
                            {
                                "tests": [
                                    {
                                        "expectedStatus": "passed",
                                        "results": [
                                            {
                                                "status": status,
                                                "attachments": attachments,
                                            }
                                        ],
                                    }
                                ]
                            }
                        ]
                    }
                ]
            }
        ]
    }


def _p1() -> list[dict[str, str]]:
    return [
        *[
            _attachment(f"sc-001-switch-{i}", {"switchNumber": i, "elapsedMs": 42})
            for i in range(1, 4)
        ],
        _attachment("sc-004-invitation-acceptance", {"elapsedMs": 99}),
    ]


def test_collects_exact_passing_measurements() -> None:
    assert collect(_report(_p1()), P1_LIMITS) == {
        "sc-001-switch-1": 42,
        "sc-001-switch-2": 42,
        "sc-001-switch-3": 42,
        "sc-004-invitation-acceptance": 99,
    }
    assert collect(
        _report([_attachment("sc-011-bootstrap-timing", {"elapsedMs": 120})]),
        BOOTSTRAP_LIMITS,
    ) == {"sc-011-bootstrap-timing": 120}


@pytest.mark.parametrize(
    "attachments,status",
    [
        (_p1()[:-1], "passed"),
        (_p1() + [_p1()[0]], "passed"),
        (_p1(), "failed"),
        (_p1() + [_attachment("sc-011-bootstrap-timing", {"elapsedMs": 1})], "passed"),
        (
            _p1()[:-1]
            + [_attachment("sc-004-invitation-acceptance", {"elapsedMs": 180_000})],
            "passed",
        ),
        (
            _p1()[:-1]
            + [_attachment("sc-004-invitation-acceptance", {"elapsedMs": True})],
            "passed",
        ),
        (
            _p1()[:-1]
            + [
                _attachment(
                    "sc-004-invitation-acceptance", {"elapsedMs": 1, "extra": "x"}
                )
            ],
            "passed",
        ),
        (
            [
                _attachment("sc-001-switch-1", {"switchNumber": 2, "elapsedMs": 42}),
                *_p1()[1:],
            ],
            "passed",
        ),
    ],
)
def test_rejects_incomplete_or_invalid_evidence(
    attachments: list[dict[str, str]], status: str
) -> None:
    with pytest.raises(TimingEvidenceError):
        collect(_report(attachments, status), P1_LIMITS)
