# Spec 042 — Quality Gate
Parent R015; stabilization program Spec 030.

Run canonical web-next workspace and core-toolkit smoke using a loopback preview and synthetic API; include MCP tests in CI. Local candidate gate records each step exit code, log hash and exact clean commit; missing or changed evidence blocks local gate.

Acceptance evidence: Local gate, canonical smoke, full CI configuration and negative gate tests.

Remaining release obligations: Hosted CI on exact promoted SHA, independent review and real browser/database end-to-end.
