# Agent release — Integration and release discipline

Governing spec R025, task T014. Review base main SHA, agent branch ancestry, CIs, migrations and read-only capability registry. Integrate only reviewed commits, run gates, and propose rollback with source-of-truth release commit.

No unauthorized push/merge/deploy; no schema or production changes until independent Windows V8i smoke, PostGIS tenant isolation, review and signoff. Produce `docs/qa/r025-release.md` with exact commands and observed outcomes.
