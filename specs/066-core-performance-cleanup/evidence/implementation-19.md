# 066 implementation slice 19 — MCP protocol, schema and multi-workspace convergence

Parent candidate: 5eb782194085d47f5f3f4cf709d16737457b32b2.

## T009 — converge retained and Rust MCP implementations
- The retained Python MCP adapter and Rust candidate now expose the same 39 tool names in full-workspace mode: 32 typed workspace tools plus live view/selection/navigation, operation status, workspace listing and two CAD tools.
- Every workspace-bound tool accepts an optional root session_id. One connected workspace remains backward-compatible without an explicit ID; multiple workspaces fail closed with 409 until the caller selects a session. Per-subject live sessions are bounded to eight.
- Python browser session IDs are UUIDs, matching the Rust schema. Session ownership is still revalidated against the current GIS cookie/project authorization before every browser poll/ping/prepare/ack/close.
- Server-side workspace mode is refreshed from browser heartbeat traffic. map_* tools require GIS mode, cad_* tools require CAD mode and other workspace tools are rejected while CAD is active. This prevents a stale AI call from crossing a mode boundary.
- cad_state and cad_select_handle are validated and bounded at both command and result boundaries. Document IDs are UUIDs, version is 1..20, handles are 1..16 hex characters and the selected document/version must be rechecked by the browser CAD adapter before execution.
- Rust MCP now negotiates the current 2026-07-28 protocol and retains 2025-11-25 compatibility. OAuth/JWKS tests continue to reject wrong audience, scope, client, signature and cross-subject workspace access.
- Machine-readable parity report: Python tools 39, Rust registry tools 39, checked-in workspace contracts 32, workspace schema mismatches 0 after removing the shared optional root session_id field. The seven additional shared tools are recorded explicitly.
- Validation after formatting: complete retained MCP suite 46/46 PASS; Rust MCP 7/7 PASS on pinned Rust 1.85.1 offline container; Ruff PASS; Python bytecode compile PASS; git diff check PASS.
- Python remains the retained SDK/transport compatibility implementation until final ADR/release review. This task does not claim backend retirement or production cutover.

This closes T009. QGIS synchronization/tool result batching and cancellation remain T036.
