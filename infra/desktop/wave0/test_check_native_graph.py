import unittest
from check_native_graph import violations

def graph(*items):
    return {"packages": [{"id": i, "name": n} for i, n in items],
            "resolve": {"nodes": [{"id": i} for i, _ in items]}}

class GraphTests(unittest.TestCase):
    def test_shared_identity(self):
        self.assertEqual(violations(graph(("git#1", "ovg-spatial"), ("path#2", "ovg-spatial"))),
                         ["duplicate shared crate: ovg-spatial"])
    def test_forbidden(self):
        self.assertEqual(violations(graph(("1", "axum"), ("2", "ovg-spatial"))), ["axum"])
    def test_clean(self):
        self.assertEqual(violations(graph(("1", "ovg-core"), ("2", "ovg-spatial"))), [])

if __name__ == "__main__":
    unittest.main()
