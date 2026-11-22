# Plan — R014 child 021
1. Separate Python package apps/mcp; no dependency on Core implementation or database.
2. Read-only client using configured project service credential and existing REST routes.
3. Official MCP Python SDK stdio tool surface; isolated tests and lockfile.
4. Browser session pairing via existing approved Partner SDK host bridge; short-lived capability relay with explicit consent. Isolated single-process candidate, no Core changes.
5. Later: authenticated public issuance, remote OAuth, deployment and security review. No production change in either candidate slice.
