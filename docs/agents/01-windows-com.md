# Agent windows — Windows COM/V8i Adapter

Read `specs/025-microstation-v8i-bridge/` and `integrations/microstation-v8i-bridge/`.

Owned scope: Windows adapter and COM capability matrix only. Develop conservative safe read-only extraction; no MicroStation save/write/close commands. Probe currently only attaches to running V8i and reads drawing/model metadata.

Tasks: T004–T006. Diagnose COM ProgID, element enumerator, ID stability; robust fallback/error handling. Do not claim Windows proof without actual licensed V8i runtime. Do not bundle proprietary Bentley binaries. Use sample fixture for logic only. Provide `docs/qa/r025-windows.md` evidence/limits.

Do not edit shared Core, database or production. Respect explicit source CRS and unsupported entity accounting.
