import unittest
from reference_semantics import Cursor, Scope, ResetRequired, advance, receipt_match, save_state

class ContractTests(unittest.TestCase):
    def setUp(self):
        self.scope = Scope("org", "project", "session", 1)
        self.cursor = Cursor(self.scope, 4)

    def test_hidden_events_advance_after_apply(self):
        self.assertEqual(advance(self.cursor, self.scope, 0, 10, 9, True).position, 9)

    def test_failed_apply_keeps_cursor(self):
        self.assertEqual(advance(self.cursor, self.scope, 0, 10, 9, False), self.cursor)

    def test_epoch_and_tenant_rejected(self):
        for scope in (Scope("org", "project", "session", 2), Scope("other", "project", "session", 1)):
            with self.assertRaises(ResetRequired):
                advance(self.cursor, scope, 0, 10, 9, True)

    def test_retention_and_out_of_range(self):
        with self.assertRaises(ResetRequired):
            advance(self.cursor, self.scope, 5, 10, 9, True)
        with self.assertRaises(ValueError):
            advance(self.cursor, self.scope, 0, 10, 11, True)

    def test_replay_binding(self):
        self.assertTrue(receipt_match("a", "d", "a", "d"))
        for actor, digest in (("b", "d"), ("a", "other")):
            with self.assertRaises(ValueError):
                receipt_match("a", "d", actor, digest)

    def test_save_state(self):
        self.assertEqual(save_state(False, True, "durable"), "denied")
        self.assertEqual(save_state(True, False, None), "unconfirmed")
        self.assertEqual(save_state(True, True, None), "unconfirmed")
        self.assertEqual(save_state(True, True, "stale"), "conflict")
        self.assertEqual(save_state(True, True, "durable"), "accepted")

if __name__ == "__main__":
    unittest.main()
