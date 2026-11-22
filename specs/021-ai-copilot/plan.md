# Plan — R014 child 021
1. Separate Python package apps/mcp; no dependency on Core implementation or database.
2. Read-only client using configured project service credential and existing REST routes.
3. Official MCP Python SDK stdio tool surface; isolated tests and lockfile.
4. Later: browser session pairing and view-only command bridge; use existing host commands.
5. Later: remote OAuth, deployment and security review. No production change in slice 1.
