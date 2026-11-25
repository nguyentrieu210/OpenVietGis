"""Regression tests for the differential gate; no database needed."""

import contextlib
import io
import unittest
from unittest.mock import Mock

import httpx
from run import Differential, difference, require


def response(body, status=200, cache="no-store"):
    return httpx.Response(status, json=body, headers={"cache-control": cache})


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
