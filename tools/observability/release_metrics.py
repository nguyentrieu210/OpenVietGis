"""Validate and aggregate privacy-safe immutable release observations.

Inputs are GitHub Actions artifacts; this module never contacts GitHub and never
stores raw identifiers in its summary output.
"""
from __future__ import annotations

import argparse
import json
import math
import os
import re
import sys
import tempfile
from datetime import datetime
from pathlib import Path
from typing import Any

_ROLES = ("api", "web", "migrate", "postgres")
_TOP_LEVEL = {
    "schema",
    "source_sha",
    "run_id",
    "build_started_at",
    "build_completed_at",
    "build_seconds",
    "deploy_status",
    "deploy_environment",
    "deploy_started_at",
    "deploy_completed_at",
    "deploy_seconds",
    "images",
}
_IMAGE_FIELDS = {
    "ref",
    "digest",
    "build_seconds",
    "build_started_at",
    "build_completed_at",
}
_SHA_RE = re.compile(r"^(?:[0-9a-f]{40}|[0-9a-f]{64})$")
_DIGEST_RE = re.compile(r"^sha256:[0-9a-f]{64}$")
_IMAGE_REF_RE = re.compile(r"^ghcr\.io/[a-z0-9][a-z0-9._/-]*@sha256:[0-9a-f]{64}$")
_RUN_ID_RE = re.compile(r"^[0-9]+$")
_DEPLOY_STATUSES = {"not_run", "completed", "failed"}


class ObservationError(ValueError):
    """Raised when an observation is malformed or includes unapproved fields."""


def _timestamp(value: Any, label: str) -> datetime:
    if not isinstance(value, str) or not value:
        raise ObservationError(f"{label} must be an ISO-8601 timestamp")
    try:
        parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError as exc:
        raise ObservationError(f"{label} must be an ISO-8601 timestamp") from exc
    if parsed.tzinfo is None:
        raise ObservationError(f"{label} must include a timezone")
    return parsed


def _seconds(value: Any, label: str, *, nullable: bool = False) -> float | None:
    if nullable and value is None:
        return None
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise ObservationError(f"{label} must be a non-negative number")
    result = float(value)
    if not math.isfinite(result) or result < 0:
        raise ObservationError(f"{label} must be a non-negative finite number")
    return result


def validate_observation(raw: Any) -> dict[str, Any]:
    """Return a validated allow-listed observation with normalized numbers."""
    if not isinstance(raw, dict):
        raise ObservationError("observation must be a JSON object")
    unknown = set(raw) - _TOP_LEVEL
    if unknown:
        raise ObservationError(f"unapproved top-level fields: {', '.join(sorted(unknown))}")
    missing = _TOP_LEVEL - set(raw)
    if missing:
        raise ObservationError(f"missing required fields: {', '.join(sorted(missing))}")
    if isinstance(raw["schema"], bool) or raw["schema"] != 1:
        raise ObservationError("unsupported observation schema")
    if not isinstance(raw["source_sha"], str) or not _SHA_RE.fullmatch(raw["source_sha"]):
        raise ObservationError("source_sha must be a full Git commit SHA")
    run_id = str(raw["run_id"])
    if not _RUN_ID_RE.fullmatch(run_id):
        raise ObservationError("run_id must be numeric")

    build_started = _timestamp(raw["build_started_at"], "build_started_at")
    build_completed = _timestamp(raw["build_completed_at"], "build_completed_at")
    build_seconds = _seconds(raw["build_seconds"], "build_seconds")
    assert build_seconds is not None
    if build_completed < build_started:
        raise ObservationError("build_completed_at precedes build_started_at")

    status = raw["deploy_status"]
    if not isinstance(status, str) or status not in _DEPLOY_STATUSES:
        raise ObservationError("deploy_status must be not_run, completed, or failed")
    deploy_seconds = _seconds(raw["deploy_seconds"], "deploy_seconds", nullable=True)
    deploy_environment = raw["deploy_environment"]
    deploy_started_at = raw["deploy_started_at"]
    deploy_completed_at = raw["deploy_completed_at"]
    if status == "not_run":
        if any(value is not None for value in (
            deploy_environment, deploy_started_at, deploy_completed_at, deploy_seconds
        )):
            raise ObservationError("not_run deployment must have null timing and environment")
    else:
        if not isinstance(deploy_environment, str) or deploy_environment not in {"ephemeral-github-actions-staging", "staging", "production"}:
            raise ObservationError("attempted deployment must name an approved environment")
        started = _timestamp(deploy_started_at, "deploy_started_at")
        completed = _timestamp(deploy_completed_at, "deploy_completed_at")
        if completed < started:
            raise ObservationError("deploy_completed_at precedes deploy_started_at")
        if deploy_seconds is None:
            raise ObservationError("attempted deployment must report observed deploy_seconds")

    images = raw["images"]
    if not isinstance(images, dict) or set(images) != set(_ROLES):
        raise ObservationError("images must contain exactly api, web, migrate, and postgres")
    normalized_images: dict[str, dict[str, Any]] = {}
    for role in _ROLES:
        image = images[role]
        if not isinstance(image, dict) or set(image) != _IMAGE_FIELDS:
            raise ObservationError(f"images.{role} must contain only the approved release fields")
        digest = image["digest"]
        ref = image["ref"]
        if not isinstance(digest, str) or not _DIGEST_RE.fullmatch(digest):
            raise ObservationError(f"images.{role}.digest must be a sha256 digest")
        if not isinstance(ref, str) or not _IMAGE_REF_RE.fullmatch(ref) or not ref.endswith("@" + digest):
            raise ObservationError(f"images.{role}.ref must be a digest-pinned GHCR reference")
        image_started = _timestamp(image["build_started_at"], f"images.{role}.build_started_at")
        image_completed = _timestamp(image["build_completed_at"], f"images.{role}.build_completed_at")
        if image_completed < image_started:
            raise ObservationError(f"images.{role} build completion precedes start")
        image_seconds = _seconds(image["build_seconds"], f"images.{role}.build_seconds")
        assert image_seconds is not None
        normalized_images[role] = {
            "ref": ref,
            "digest": digest,
            "build_seconds": image_seconds,
            "build_started_at": image_started.isoformat(),
            "build_completed_at": image_completed.isoformat(),
        }

    return {
        "schema": 1,
        "source_sha": raw["source_sha"],
        "run_id": run_id,
        "build_started_at": build_started.isoformat(),
        "build_completed_at": build_completed.isoformat(),
        "build_seconds": build_seconds,
        "deploy_status": status,
        "deploy_environment": deploy_environment,
        "deploy_started_at": deploy_started_at,
        "deploy_completed_at": deploy_completed_at,
        "deploy_seconds": deploy_seconds,
        "images": normalized_images,
    }


def _percentile(values: list[float], p: float) -> float | None:
    if not values:
        return None
    ordered = sorted(values)
    index = max(0, math.ceil(p * len(ordered)) - 1)
    return ordered[index]


def _distribution(values: list[float]) -> dict[str, int | float | None]:
    return {
        "sample_count": len(values),
        "p50_seconds": _percentile(values, 0.50),
        "p95_seconds": _percentile(values, 0.95),
        "p99_seconds": _percentile(values, 0.99),
    }


def summarize_observations(observations: list[dict[str, Any]]) -> dict[str, Any]:
    validated = [validate_observation(item) for item in observations]
    started_deploys = [item for item in validated if item["deploy_status"] != "not_run"]
    successful_deploys = [item for item in started_deploys if item["deploy_status"] == "completed"]
    failed_deploys = [item for item in started_deploys if item["deploy_status"] == "failed"]
    return {
        "schema": 1,
        "observation_count": len(validated),
        "build_seconds": _distribution([item["build_seconds"] for item in validated]),
        "deployment_seconds": _distribution([
            item["deploy_seconds"] for item in started_deploys
            if item["deploy_seconds"] is not None
        ]),
        "deploy_attempts": len(started_deploys),
        "deploy_successes": len(successful_deploys),
        "deploy_failures": len(failed_deploys),
        "image_build_seconds": {
            role: _distribution([
                item["images"][role]["build_seconds"] for item in validated
            ])
            for role in _ROLES
        },
        "privacy": {
            "tenant_or_user_fields_included": False,
            "raw_commit_and_run_ids_included": False,
        },
    }


def record_deployment(
    raw: Any,
    *,
    status: str,
    environment: str,
    started_at: str,
    completed_at: str,
    seconds: float,
) -> dict[str, Any]:
    """Return a validated copy with the measured deployment attempt attached."""
    observation = validate_observation(raw)
    if observation["deploy_status"] != "not_run":
        raise ObservationError("deployment is already recorded; refusing to overwrite it")
    if status not in {"completed", "failed"}:
        raise ObservationError("record-deploy status must be completed or failed")
    updated = {
        **observation,
        "deploy_status": status,
        "deploy_environment": environment,
        "deploy_started_at": started_at,
        "deploy_completed_at": completed_at,
        "deploy_seconds": seconds,
    }
    return validate_observation(updated)


def _write_json_new(path: str, value: dict[str, Any]) -> None:
    target = Path(path)
    target.parent.mkdir(parents=True, exist_ok=True)
    if target.exists():
        raise ObservationError(f"output already exists: {target.name}")
    temporary_name: str | None = None
    try:
        with tempfile.NamedTemporaryFile(
            "w", encoding="utf-8", dir=target.parent, prefix=".release-metrics-", delete=False
        ) as handle:
            temporary_name = handle.name
            json.dump(value, handle, ensure_ascii=False, indent=2)
            handle.write("\n")
        os.replace(temporary_name, target)
    except OSError as exc:
        if temporary_name is not None:
            try:
                os.unlink(temporary_name)
            except OSError:
                pass
        raise ObservationError(f"cannot write deployment observation {target.name}: {exc}") from exc


def _read_json(path: str) -> Any:
    try:
        return json.loads(Path(path).read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise ObservationError(f"cannot read JSON observation {path}: {exc}") from exc


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    validate = subparsers.add_parser("validate", help="validate one release-observability.json")
    validate.add_argument("path")
    summarize = subparsers.add_parser("summarize", help="aggregate release observations")
    summarize.add_argument("paths", nargs="+")
    record = subparsers.add_parser("record-deploy", help="write a measured deployment copy")
    record.add_argument("path", help="release-observability.json input")
    record.add_argument("--output", required=True, help="new deployment-observability.json path")
    record.add_argument("--status", choices=("completed", "failed"), required=True)
    record.add_argument("--environment", required=True)
    record.add_argument("--started-at", required=True)
    record.add_argument("--completed-at", required=True)
    record.add_argument("--seconds", required=True, type=float)
    args = parser.parse_args(argv)
    try:
        if args.command == "validate":
            observation = validate_observation(_read_json(args.path))
            print(json.dumps({
                "valid": True,
                "source_sha": observation["source_sha"],
                "run_id": observation["run_id"],
                "build_seconds": observation["build_seconds"],
                "deploy_status": observation["deploy_status"],
                "deploy_seconds": observation["deploy_seconds"],
                "image_count": len(observation["images"]),
            }, sort_keys=True))
        elif args.command == "summarize":
            summary = summarize_observations([_read_json(path) for path in args.paths])
            print(json.dumps(summary, sort_keys=True, indent=2))
        else:
            recorded = record_deployment(
                _read_json(args.path),
                status=args.status,
                environment=args.environment,
                started_at=args.started_at,
                completed_at=args.completed_at,
                seconds=args.seconds,
            )
            _write_json_new(args.output, recorded)
            print(json.dumps({
                "recorded": True,
                "deploy_status": recorded["deploy_status"],
                "deploy_environment": recorded["deploy_environment"],
                "deploy_seconds": recorded["deploy_seconds"],
            }, sort_keys=True))
    except ObservationError as exc:
        print(f"release metrics rejected: {exc}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
