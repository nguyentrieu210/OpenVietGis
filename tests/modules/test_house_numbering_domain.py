import unittest

from modules.house_numbering.domain import (
    Address,
    DomainError,
    EvidenceCandidate,
    Rule,
    SurveyResult,
    certificate_issuance,
    classify_reconciliation,
    locate_on_route,
    match_certificate,
    preview_shift,
    proposal_transition,
    public_export_record,
    suggest_number,
    summarize_progress,
    validate_import_counts,
)


class NumberingTests(unittest.TestCase):
    def setUp(self):
        self.rule = Rule("v1")
        self.a = Address("a", "100", 10, "right", "approved")
        self.b = Address("b", "102", 30, "right", "approved")
        self.c = Address("c", "106", 30, "right", "approved")

    def suggestion(self, rows, measure=20, side="right", **kw):
        return suggest_number(
            rows, measure=measure, side=side, rule=self.rule, **kw
        ).number

    def test_gap_before_suffix(self):
        self.assertEqual(self.suggestion([self.a, self.c]), "102")

    def test_suffix_and_existing(self):
        self.assertEqual(self.suggestion([self.a, self.b]), "100A")
        self.assertEqual(
            self.suggestion([self.a, self.b, Address("x", "100A", 15, "right")]), "100B"
        )
        self.assertEqual(self.suggestion([self.a, self.b], existing_id="a"), "100")

    def test_suffix_dash(self):
        self.assertEqual(
            suggest_number(
                [self.a, self.b],
                measure=20,
                side="right",
                rule=Rule("v2", suffix_style="dash"),
            ).number,
            "100-1",
        )

    def test_alley(self):
        rows = [
            Address("x", "155/3/2", 10, "right", "approved"),
            Address("y", "155/3/4", 30, "right", "approved"),
        ]
        self.assertEqual(self.suggestion(rows, prefix="155/3"), "155/3/2A")

    def test_parity_direction_bounds(self):
        self.assertEqual(self.suggestion([], side="left"), "1")
        self.assertEqual(self.suggestion([], side="right"), "2")
        self.assertEqual(self.suggestion([self.a], measure=40), "102")
        self.assertEqual(self.suggestion([self.a], measure=0), "98")
        self.assertIsNone(
            self.suggestion([Address("first", "2", 10, "right", "approved")], measure=0)
        )
        self.assertIsNone(
            suggest_number(
                [], measure=20, side="right", rule=self.rule, distance=151
            ).number
        )
        self.assertEqual(
            suggest_number(
                [], measure=20, side="left", rule=Rule("v3", left_odd=False)
            ).number,
            "2",
        )

    def test_reserved_and_other_side(self):
        rows = [
            self.a,
            self.c,
            Address("r", "102", 25, "right", "reserved"),
            Address("l", "103", 18, "left", "approved"),
        ]
        self.assertEqual(self.suggestion(rows), "104")

    def test_ambiguous_geometry_and_invalid_input(self):
        self.assertIsNone(
            self.suggestion([self.a, Address("tie", "104", 10, "right", "approved")])
        )
        self.assertIsNone(
            self.suggestion([Address("raw", "C5/2", 10, "right", "approved")])
        )
        with self.assertRaises(DomainError):
            self.suggestion([], side="middle")
        with self.assertRaises(DomainError):
            self.suggestion([], measure=float("nan"))

    def test_nonmonotonic_route(self):
        self.assertIsNone(
            self.suggestion(
                [
                    Address("x", "106", 10, "right", "approved"),
                    Address("y", "100", 30, "right", "approved"),
                ]
            )
        )

    def test_shift_protects_locked_and_is_pure(self):
        rows = [
            Address("x", "100", 10, "right", "proposed"),
            Address("y", "102", 20, "right", "proposed"),
            Address("z", "106", 50, "right", "approved"),
        ]
        self.assertEqual(
            preview_shift(rows, start_measure=15, side="right", rule=self.rule),
            {"y": "104"},
        )
        self.assertEqual(rows[1].number, "102")
        with self.assertRaises(DomainError):
            preview_shift(
                [rows[0], rows[1], Address("z", "104", 50, "right", "approved")],
                start_measure=15,
                side="right",
                rule=self.rule,
            )


class ReconciliationTests(unittest.TestCase):
    def test_all_categories(self):
        cases = [
            (
                {"certificate": "100", "cad": "100", "survey": "100", "surveyed": True},
                "matched",
            ),
            (
                {"certificate": "100", "cad": "100", "survey": "102", "surveyed": True},
                "number_mismatch",
            ),
            (
                {"certificate": "100", "cad": "102", "survey": "100", "surveyed": True},
                "cad_certificate_mismatch",
            ),
            (
                {"certificate": None, "cad": None, "survey": None, "surveyed": True},
                "new_building",
            ),
            (
                {
                    "certificate": "100",
                    "cad": None,
                    "survey": None,
                    "surveyed": True,
                    "exists": False,
                },
                "no_longer_exists",
            ),
            (
                {"certificate": "100", "cad": None, "survey": None, "surveyed": False},
                "not_surveyed",
            ),
            (
                {
                    "certificate": "100",
                    "cad": None,
                    "survey": None,
                    "surveyed": False,
                    "building_linked": False,
                },
                "certificate_unmatched",
            ),
        ]
        for args, expected in cases:
            with self.subTest(expected=expected):
                self.assertEqual(classify_reconciliation(**args), expected)

    def test_no_mutation_or_guess(self):
        with self.assertRaises(DomainError):
            classify_reconciliation(
                certificate=None,
                cad=None,
                survey=None,
                surveyed=False,
                building_linked=False,
            )


class ProposalTests(unittest.TestCase):
    def test_transitions(self):
        self.assertEqual(proposal_transition("draft", "submit"), "submitted")
        self.assertEqual(
            proposal_transition("submitted", "reject", reviewer=True, comment="fix"),
            "draft",
        )
        self.assertEqual(
            proposal_transition(
                "submitted",
                "approve",
                reviewer=True,
                rule=Rule("official", authority="approved reference"),
            ),
            "approved",
        )

    def test_authority_and_permission(self):
        with self.assertRaises(DomainError):
            proposal_transition(
                "submitted", "approve", reviewer=True, rule=Rule("draft")
            )
        with self.assertRaises(DomainError):
            proposal_transition(
                "submitted",
                "approve",
                reviewer=False,
                rule=Rule("official", authority="ref"),
            )
        with self.assertRaises(DomainError):
            proposal_transition("approved", "submit")
        with self.assertRaises(DomainError):
            proposal_transition("submitted", "reject", reviewer=True)
        with self.assertRaises(DomainError):
            proposal_transition(
                "submitted",
                "approve",
                reviewer=True,
                rule=Rule("official", authority="ref"),
                eligible=False,
            )


class AdditionalDomainTests(unittest.TestCase):
    def test_oriented_measure(self):
        a = locate_on_route((5, 3), [(0, 0), (10, 0)], crs="verified-meter")
        self.assertEqual((a.measure, a.distance, a.side), (5, 3, "left"))
        b = locate_on_route((5, 3), [(10, 0), (0, 0)], crs="verified-meter")
        self.assertEqual(b.side, "right")
        self.assertIsNone(
            locate_on_route((5, 0), [(0, 0), (10, 0)], crs="verified-meter").side
        )
        self.assertIsNone(
            locate_on_route((2, 2), [(0, 0), (4, 0), (4, 4)], crs="verified-meter").side
        )
        with self.assertRaises(DomainError):
            locate_on_route((5, 0), [(0, 0), (10, 0)], crs="")
        with self.assertRaises(DomainError):
            locate_on_route((5, 0), [(0, 0), (0, 0)], crs="verified-meter")

    def test_certificate_unique_matching(self):
        candidates = [
            EvidenceCandidate("a", "r", "old1", "100"),
            EvidenceCandidate("b", "r", "old2", "102"),
            EvidenceCandidate("foreign", "other", "old1", "100"),
        ]
        self.assertEqual(
            match_certificate(
                candidates, route_id="r", old_number="old1", new_number="102"
            ).building_id,
            "a",
        )
        self.assertEqual(
            match_certificate(
                candidates, route_id="r", old_number=None, new_number="102"
            ).building_id,
            "b",
        )
        self.assertIsNone(
            match_certificate(
                candidates + [EvidenceCandidate("c", "r", "old1", "104")],
                route_id="r",
                old_number="old1",
                new_number="100",
            ).building_id
        )
        self.assertIsNone(
            match_certificate(
                candidates, route_id="r", old_number="none", new_number="999"
            ).building_id
        )

    def test_progress_and_import_accounting(self):
        surveys = [
            SurveyResult("a", "house", "accepted"),
            SurveyResult("b", "vacant", "accepted"),
            SurveyResult("c", "house", "draft"),
            SurveyResult("a", "house", "accepted"),
        ]
        self.assertEqual(
            summarize_progress(surveys, target=4),
            {"positions": 2, "houses": 1, "vacant": 1, "other": 0, "completion": 50.0},
        )
        self.assertIsNone(summarize_progress(surveys, target=None)["completion"])
        with self.assertRaises(DomainError):
            summarize_progress(
                surveys + [SurveyResult("a", "vacant", "accepted")], target=4
            )
        self.assertEqual(validate_import_counts(10, 7, 3), 10)
        with self.assertRaises(DomainError):
            validate_import_counts(10, 7, 2)

    def test_safe_exports_and_certificate_gate(self):
        record = {
            "house_number": "100",
            "route_name": "Đường A",
            "name": "secret",
            "cccd": "secret",
            "phone": "secret",
            "notes": "secret",
            "nested": {"name": "secret"},
        }
        self.assertEqual(
            public_export_record(record),
            {"house_number": "100", "route_name": "Đường A"},
        )
        self.assertEqual(public_export_record({"house_number": {"cccd": "secret"}}), {})
        self.assertEqual(
            certificate_issuance(
                "approved", template_verified=True, rule=Rule("v", authority="ref")
            ),
            "official",
        )
        self.assertEqual(
            certificate_issuance(
                "approved", template_verified=False, rule=Rule("v", authority="ref")
            ),
            "draft",
        )
        with self.assertRaises(DomainError):
            certificate_issuance(
                "proposed", template_verified=True, rule=Rule("v", authority="ref")
            )


if __name__ == "__main__":
    unittest.main()
