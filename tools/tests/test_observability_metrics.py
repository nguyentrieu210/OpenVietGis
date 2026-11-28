from __future__ import annotations

import json
import tempfile
import unittest
from contextlib import redirect_stdout
from io import StringIO
from pathlib import Path

from tools.observability.release_metrics import (
    ObservationError,
    main,
    record_deployment,
    summarize_observations,
    validate_observation,
)


def observation(
    *,
    build_seconds: float = 120.0,
    deploy_status: str = "not_run",
    deploy_seconds: float | None = None,
    run_id: int = 123456,
) -> dict:
    image = {
        "ref": "ghcr.io/nguyentrieu210/openvietgis-api@sha256:" + "a" * 64,
        "digest": "sha256:" + "a" * 64,
        "build_seconds": build_seconds / 4,
        "build_started_at": "2026-11-28T10:00:00Z",
        "build_completed_at": "2026-11-28T10:00:30Z",
    }
    return {
        "schema": 1,
        "source_sha": "b" * 40,
        "run_id": run_id,
        "build_started_at": "2026-11-28T10:00:00Z",
        "build_completed_at": "2026-11-28T10:02:00Z",
        "build_seconds": build_seconds,
        "deploy_status": deploy_status,
        "deploy_environment": "ephemeral-github-actions-staging" if deploy_status != "not_run" else None,
        "deploy_started_at": "2026-11-28T10:03:00Z" if deploy_status != "not_run" else None,
        "deploy_completed_at": "2026-11-28T10:03:20Z" if deploy_status != "not_run" else None,
        "deploy_seconds": deploy_seconds,
        "images": {role: {**image, "ref": image["ref"].replace("-api@", f"-{role}@")}
                   for role in ("api", "web", "migrate", "postgres")},
    }


class ReleaseMetricsTests(unittest.TestCase):
    def test_validates_digest_pinned_release_with_no_deployment(self):
        item = validate_observation(observation())
        self.assertEqual(item["build_seconds"], 120)
        self.assertEqual(item["deploy_status"], "not_run")
        self.assertIsNone(item["deploy_seconds"])
        self.assertEqual(set(item["images"]), {"api", "web", "migrate", "postgres"})

    def test_summarizes_quantiles_and_omits_raw_ids(self):
        items = [
            observation(build_seconds=100, deploy_status="completed", deploy_seconds=20, run_id=111),
            observation(build_seconds=200, deploy_status="failed", deploy_seconds=40, run_id=222),
            observation(build_seconds=300, deploy_status="not_run", deploy_seconds=None, run_id=333),
        ]
        result = summarize_observations(items)
        self.assertEqual(result["build_seconds"], {
            "sample_count": 3,
            "p50_seconds": 200,
            "p95_seconds": 300,
            "p99_seconds": 300,
        })
        self.assertEqual(result["deployment_seconds"], {
            "sample_count": 2,
            "p50_seconds": 20,
            "p95_seconds": 40,
            "p99_seconds": 40,
        })
        self.assertEqual(result["deploy_attempts"], 2)
        self.assertEqual(result["deploy_successes"], 1)
        self.assertEqual(result["deploy_failures"], 1)
        self.assertEqual(result["image_build_seconds"]["api"]["sample_count"], 3)
        encoded = json.dumps(result)
        self.assertNotIn('"source_sha":', encoded)
        self.assertNotIn('"run_id":', encoded)
        self.assertNotIn("111", encoded)
        self.assertFalse(result["privacy"]["tenant_or_user_fields_included"])

    def test_records_observed_deployment_without_mutating_build_artifact(self):
        original = observation()
        recorded = record_deployment(
            original,
            status="completed",
            environment="ephemeral-github-actions-staging",
            started_at="2026-11-28T10:03:00Z",
            completed_at="2026-11-28T10:03:20Z",
            seconds=20,
        )
        self.assertEqual(recorded["deploy_status"], "completed")
        self.assertEqual(recorded["deploy_seconds"], 20)
        self.assertEqual(original["deploy_status"], "not_run")
        with self.assertRaisesRegex(ObservationError, "already recorded"):
            record_deployment(
                recorded,
                status="failed",
                environment="ephemeral-github-actions-staging",
                started_at="2026-11-28T10:04:00Z",
                completed_at="2026-11-28T10:04:05Z",
                seconds=5,
            )

    def test_record_deploy_cli_writes_copy_and_preserves_build_artifact(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "release-observability.json"
            target = Path(directory) / "deployment-observability.json"
            source.write_text(json.dumps(observation()), encoding="utf-8")
            with redirect_stdout(StringIO()):
                exit_code = main([
                    "record-deploy",
                    str(source),
                    "--output",
                    str(target),
                    "--environment",
                    "ephemeral-github-actions-staging",
                    "--status",
                    "completed",
                    "--started-at",
                    "2026-11-28T10:03:00Z",
                    "--completed-at",
                    "2026-11-28T10:03:20Z",
                    "--seconds",
                    "20",
                ])
            self.assertEqual(exit_code, 0)
            build = json.loads(source.read_text(encoding="utf-8"))
            deployment = json.loads(target.read_text(encoding="utf-8"))
            self.assertEqual(build["deploy_status"], "not_run")
            self.assertEqual(deployment["deploy_status"], "completed")
            self.assertEqual(deployment["deploy_seconds"], 20)

    def test_rejects_unapproved_privacy_fields(self):
        item = observation()
        item["project_id"] = "private-project"
        with self.assertRaisesRegex(ObservationError, "unapproved top-level fields"):
            validate_observation(item)

    def test_rejects_unpinned_image_reference(self):
        item = observation()
        item["images"]["web"]["ref"] = "ghcr.io/nguyentrieu210/openvietgis-web:latest"
        with self.assertRaisesRegex(ObservationError, "digest-pinned GHCR"):
            validate_observation(item)

    def test_not_run_cannot_claim_deployment_time_or_environment(self):
        item = observation()
        item["deploy_seconds"] = 5
        with self.assertRaisesRegex(ObservationError, "not_run deployment"):
            validate_observation(item)

    def test_started_deployment_requires_measured_duration(self):
        item = observation(deploy_status="completed", deploy_seconds=None)
        with self.assertRaisesRegex(ObservationError, "observed deploy_seconds"):
            validate_observation(item)


if __name__ == "__main__":
    unittest.main()
