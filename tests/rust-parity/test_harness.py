"""Regression tests for the differential gate; no database needed."""

import contextlib
import io
import unittest
from unittest.mock import MagicMock, Mock, patch

import httpx
from run import Differential, baseline, database, difference, require


def response(body, status=200, cache="no-store"):
    return httpx.Response(status, json=body, headers={"cache-control": cache})


class ParityDatabaseTests(unittest.TestCase):
    @patch("run.psycopg.connect")
    def test_generated_databases_satisfy_benchmark_seed_guard(self, connect):
        admin = MagicMock()
        connect.return_value.__enter__.return_value = admin
        admin_url = baseline.DEFAULT_ADMIN_PG_URL

        with database(admin_url) as (owner_url, runtime_url):
            name = baseline.assert_safe_bench_db(owner_url)
            self.assertRegex(name, r"^openvietgis_bench_parity_test_[0-9a-f]{32}$")
            self.assertEqual(baseline.assert_safe_bench_db(runtime_url), name)
            self.assertEqual(admin.execute.call_count, 1)

        connect.assert_called_once_with(admin_url, autocommit=True)
        self.assertEqual(admin.execute.call_count, 2)

    @patch("run.psycopg.connect")
    def test_invalid_admin_port_is_rejected_before_connecting(self, connect):
        admin_url = baseline.DEFAULT_ADMIN_PG_URL.replace(":54330/", ":5432/")

        with self.assertRaisesRegex(RuntimeError, "dedicated loopback PostGIS"), database(
            admin_url
        ):
            self.fail("connected to a non-test database")

        connect.assert_not_called()


class DifferentialTests(unittest.TestCase):
    def compare(self, expected, actual, **kwargs):
        oracle = Mock()
        oracle.get.return_value = expected
        rust = Mock()
        rust.get.return_value = actual
        suite = Differential(oracle, rust)
        with contextlib.redirect_stdout(io.StringIO()):
            suite.get("case", "/test", **kwargs)
        return suite

    def test_matching_response_passes(self):
        suite = self.compare(response({"id": 1}), response({"id": 1}))
        self.assertEqual((suite.passed, suite.failures), (1, []))

    def test_candidate_missing_route_fails(self):
        suite = self.compare(response({"id": 1}), response({}, 404))
        self.assertEqual((suite.passed, suite.failures), (0, ["case"]))

    def test_identical_insecure_responses_still_fail(self):
        def no_secret(body):
            require("secret" not in body, "secret exposed")

        suite = self.compare(
            response({"secret": "fake"}), response({"secret": "fake"}), check=no_secret
        )
        self.assertEqual((suite.passed, suite.failures), (0, ["case"]))

    def test_independent_assertion_checks_candidate_even_with_normalization(self):
        def no_secret(body):
            require("secret" not in body, "secret exposed")

        suite = self.compare(
            response({}),
            response({"secret": "fake"}),
            normalize=lambda body: {},
            check=no_secret,
        )
        self.assertEqual((suite.passed, suite.failures), (0, ["case"]))

    def test_cache_mismatch_fails(self):
        suite = self.compare(response({}), response({}, cache="public"))
        self.assertEqual((suite.passed, suite.failures), (0, ["case"]))

    def test_type_mismatch_does_not_print_values(self):
        self.assertEqual(
            difference({"token": "fake"}, {"token": 1}),
            "$.token: value or JSON type differs from FastAPI",
        )


if __name__ == "__main__":
    unittest.main()
