"""Offline relay contract tests: no network or real credentials."""
import hashlib
import hmac
import json
import sqlite3
import unittest
from uuid import UUID

from webhook_relay import Relay, signature


def uid(n):
    return str(UUID(int=n))


def item(n):
    return {"id": uid(n), "event_kind": "changeset.committed", "resource_type": "changeset",
            "resource_id": uid(n), "revision_ref": "abc", "created_at": "2026-11-23T00:00:00Z",
            "actor_display": "PRIVATE", "metadata": {"secret": "PRIVATE"}}


class RelayTest(unittest.TestCase):
    def test_checkpoint_durable_queue_retry_and_redaction(self):
        db = sqlite3.connect(":memory:")
        pages = {None: {"items": [item(1)], "next_cursor": None}}
        sent = []
        fail = False

        def deliver(event_id, body):
            if fail:
                raise OSError("fixture outage")
            sent.append((event_id, body))

        relay = Relay(db, uid(999), pages.__getitem__, deliver)
        self.assertEqual(relay.poll(), 0)
        self.assertEqual(relay.flush(), 0)
        pages[None] = {"items": [item(4), item(3)], "next_cursor": "next"}
        pages["next"] = {"items": [item(2), item(1)], "next_cursor": None}
        self.assertEqual(relay.poll(), 3)
        fail = True
        with self.assertRaises(OSError):
            relay.flush()
        # Restart and recover without duplicating queued entries.
        relay = Relay(db, uid(999), pages.__getitem__, deliver)
        self.assertEqual(relay.poll(), 0)
        fail = False
        self.assertEqual(relay.flush(), 3)
        self.assertEqual([event_id for event_id, _ in sent], [uid(2), uid(3), uid(4)])
        for _, body in sent:
            self.assertNotIn(b"PRIVATE", body)
            self.assertNotIn("metadata", json.loads(body))
        self.assertEqual(relay.flush(), 0)
        pages[None] = {"items": [item(6)], "next_cursor": None}
        with self.assertRaises(RuntimeError):
            relay.poll()
        self.assertEqual(relay.flush(), 0)

    def test_signature_and_empty_initial_feed(self):
        self.assertEqual(signature(b"key", "123", b"{}"),
                         "sha256=" + hmac.new(b"key", b"123.{}", hashlib.sha256).hexdigest())
        db = sqlite3.connect(":memory:")
        page = {"items": [], "next_cursor": None}
        sent = []
        relay = Relay(db, uid(999), lambda _: page, lambda *args: sent.append(args))
        self.assertEqual(relay.poll(), 0)
        page["items"] = [item(1)]
        self.assertEqual(relay.poll(), 1)
        self.assertEqual(relay.flush(), 1)
        self.assertEqual(len(sent), 1)


if __name__ == "__main__":
    unittest.main()
