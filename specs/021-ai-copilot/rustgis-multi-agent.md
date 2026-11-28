# Wave 4 — bounded multi-agent GIS

Base Wave 3 PR #107 at 3f006d94c259d37528e12042035e494718654c93; upstream RustGIS assistant/agent.rs at aea84131974b57ff6359666dbe79f88054a64bf1 (Apache-2.0). Adapt delegation, separate conversation and shared budget; no desktop engine import.

## Requirements
- Explicit team-mode opt-in, same consented provider/model, no external ACP (Wave 7).
- Parent model can call internal delegate_gis with up to three distinct role tasks. Four roles: data, spatial, cartography, quality. Children use role skill and intersection of parent permissions with fixed read-only capability list; never delegate further.
- Model requests for children run concurrently via join, separate provider-native histories. All workspace dispatch uses one paced async mutex and existing browser identity/approval; no child asynchronous processing jobs or writes. Parent may propose approved edits only.
- Atomic shared limits: 16 logical model steps, 64 attempts, 32 runtime calls (including delegate), 3 children total, 300-second deadline. Cancellation propagates to child futures, no detached child workers.
- Child reports treated as untrusted observations. Parent stops if a child fails or has uncertain receipt. Report UI includes role, stop, result and uncertainty; counts aggregate all agents.
- Existing MCP external registry remains 39 tools; internal delegation never routed through MCP dispatcher.
- Run native tests and browser smoke. Real multi-agent/provider integration and deployment remain gated; no production claims.
