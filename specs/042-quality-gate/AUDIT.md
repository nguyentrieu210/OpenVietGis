# Quality Gate — evidence policy

Implementation scope: Run canonical web-next workspace and core-toolkit smoke using a loopback preview and synthetic API; include MCP tests in CI. Local candidate gate records each step exit code, log hash and exact clean commit; missing or changed evidence blocks local gate.

Run evidence: Local gate, canonical smoke, full CI configuration and negative gate tests. See [candidate runbook](../../docs/community/CANDIDATE_RUNBOOK.md) for exact candidate logs. Local checks are not release approval.

Open: Hosted CI on exact promoted SHA, independent review and real browser/database end-to-end.
