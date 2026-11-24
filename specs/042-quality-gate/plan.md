# Plan
1. Inspect inherited candidate and relevant contracts.
2. Implement bounded scope: Run canonical web-next workspace and core-toolkit smoke using a loopback preview and synthetic API; include MCP tests in CI. Local candidate gate records each step exit code, log hash and exact clean commit; missing or changed evidence blocks local gate.
3. Run and record: Local gate, canonical smoke, full CI configuration and negative gate tests.
4. Preserve blockers: Hosted CI on exact promoted SHA, independent review and real browser/database end-to-end.
