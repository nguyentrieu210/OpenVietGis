"""Verify only the synthetic, scalar browser timing evidence from Playwright reports."""

import argparse
import base64
import binascii
import json
from pathlib import Path
from typing import Any

P1_LIMITS = {
    "sc-001-switch-1": 10_000,
    "sc-001-switch-2": 10_000,
    "sc-001-switch-3": 10_000,
    "sc-004-invitation-acceptance": 180_000,
}
BOOTSTRAP_LIMITS = {"sc-011-bootstrap-timing": 300_000}
ALL_NAMES = P1_LIMITS.keys() | BOOTSTRAP_LIMITS.keys()


class TimingEvidenceError(ValueError):
    """Missing, ambiguous, malformed or over-budget timing evidence."""


def _results(suites: list[dict[str, Any]]) -> Any:
    for suite in suites:
        for spec in suite.get("specs", []):
            for test in spec.get("tests", []):
                # A failed or expected-to-fail result is not acceptance evidence.
                if test.get("expectedStatus") == "passed":
                    yield from test.get("results", [])
        yield from _results(suite.get("suites", []))


def collect(report: dict[str, Any], limits: dict[str, int]) -> dict[str, int]:
    measured: dict[str, int] = {}
    for result in _results(report["suites"]):
        if result.get("status") != "passed":
            continue
        for attachment in result.get("attachments", []):
            name = attachment.get("name")
            if name not in ALL_NAMES:
                continue
            if name not in limits or name in measured:
                raise TimingEvidenceError("Unexpected or duplicate timing attachment")
            try:
                if attachment.get("contentType") != "application/json":
                    raise ValueError
                payload = json.loads(
                    base64.b64decode(attachment["body"], validate=True)
                )
                if not isinstance(payload, dict):
                    raise TypeError
                expected_keys = {"elapsedMs"}
                if name.startswith("sc-001-switch-"):
                    expected_keys.add("switchNumber")
                    if type(payload.get("switchNumber")) is not int or payload[
                        "switchNumber"
                    ] != int(name[-1]):
                        raise ValueError
                elapsed = payload["elapsedMs"]
                if set(payload) != expected_keys or type(elapsed) is not int:
                    raise ValueError
            except (KeyError, ValueError, TypeError, binascii.Error) as exc:
                raise TimingEvidenceError("Invalid scalar timing attachment") from exc
            if not 0 <= elapsed < limits[name]:
                raise TimingEvidenceError(f"{name}: timing threshold not met")
            measured[name] = elapsed
    if measured.keys() != limits.keys():
        raise TimingEvidenceError("Required successful timing evidence missing")
    return measured


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("p1_report", type=Path)
    parser.add_argument("bootstrap_report", type=Path)
    args = parser.parse_args()
    try:
        p1 = collect(json.loads(args.p1_report.read_text(encoding="utf-8")), P1_LIMITS)
        bootstrap = collect(
            json.loads(args.bootstrap_report.read_text(encoding="utf-8")),
            BOOTSTRAP_LIMITS,
        )
    except (OSError, ValueError, KeyError, TypeError, AttributeError):
        # Do not print arbitrary report contents or parsing exception payloads.
        print("Tenant timing evidence: missing, invalid, duplicate or over budget")
        return 1
    for name, elapsed in (p1 | bootstrap).items():
        print(f"Tenant timing evidence: {name}: {elapsed} ms")
    print("SC-013 requires separate uncoached human evidence")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
