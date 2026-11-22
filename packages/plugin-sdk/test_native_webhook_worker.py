"""Transactional journal delivery behavior with synthetic events."""
import json
import sqlite3
import socket
import unittest
from unittest.mock import patch
from uuid import UUID

from native_webhook_worker import NativeWorker, public_target


def uid(n):
    return str(UUID(int=n))


def item(seq):
    return {"sequence":seq,"id":uid(seq),"event_kind":"changeset.committed",
            "resource_type":"changeset","resource_id":uid(seq),"revision_ref":"abc",
            "created_at":"2026-11-23T00:00:00Z","actor_display":"PRIVATE"}


class NativeWorkerTest(unittest.TestCase):
    def test_checkpoint_queue_retry_order(self):
        db=sqlite3.connect(":memory:")
        events=[item(1)]
        sent=[]
        failure=False
        def page(after):
            selected=[e for e in events if e["sequence"]>after][:2]
            return {"items":selected,"head_sequence":events[-1]["sequence"],
                    "next_after_sequence":selected[-1]["sequence"] if selected else after}
        def deliver(id,body):
            if failure:
                raise OSError("unavailable")
            sent.append((id,json.loads(body)))
        w=NativeWorker(db,uid(999),page,deliver)
        self.assertEqual(w.poll(),0)
        events.extend([item(3),item(6),item(9)])
        self.assertEqual(w.poll(),2)
        failure=True
        with self.assertRaises(OSError):
            w.flush()
        w=NativeWorker(db,uid(999),page,deliver)
        self.assertEqual(w.poll(),1)
        failure=False
        self.assertEqual(w.flush(),3)
        self.assertEqual([e[1]["sequence"] for e in sent],[3,6,9])
        self.assertEqual(w.poll(),0)
        self.assertEqual(w.flush(),0)
        self.assertNotIn("PRIVATE",json.dumps(sent))
        events.clear()
        events.append(item(1))
        with self.assertRaises(ValueError):
            w.poll()

    def test_deny_nonpublic_or_mixed_dns(self):
        private=(socket.AF_INET,socket.SOCK_STREAM,6,"",("127.0.0.1",443))
        public=(socket.AF_INET,socket.SOCK_STREAM,6,"",("8.8.8.8",443))
        with patch("socket.getaddrinfo",return_value=[public,private]):
            with self.assertRaises(ValueError):
                public_target("https://webhooks.example.com/events")
        with patch("socket.getaddrinfo",return_value=[public]):
            self.assertEqual(public_target("https://webhooks.example.com/events"),("webhooks.example.com","8.8.8.8","/events"))

    def test_replay(self):
        db=sqlite3.connect(":memory:")
        p=lambda after:{"items":[item(4)] if after<4 else [],
                        "head_sequence":4,"next_after_sequence":4}
        w=NativeWorker(db,uid(999),p,lambda *_:None,replay=True)
        self.assertEqual(w.poll(),1)
        self.assertEqual(w.flush(),1)


if __name__=="__main__":
    unittest.main()
