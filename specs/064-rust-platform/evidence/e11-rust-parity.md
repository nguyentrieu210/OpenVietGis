# E11 Rust-native realtime / sync / collaboration parity

Branch: `064-e11-rust-native`, synchronized with `064-convergence` on 2026-11-26.

## Implemented Rust surface

- Collaboration activity cursor paging and project SSE.
- Managed QGIS sessions, bootstrap, delta replay, commits/receipts, desktop commands and SSE.
- Project collaboration REST: chat, feature comments/mentions, tasks, notifications, edit leases, reviews and conflict-rebase inspection.
- Organization chat: rooms, messages, attachments, reactions, pins, contacts, people search and SSE.
- Presence WebSocket with origin/session/project authorization, bounded peer budgets, 5 s reauthorization, control-frame validation, keepalive and bounded writes.
- Dedicated PostgreSQL LISTEN pools retain the convergence runtime-role fail-fast check.

## Differential parity

FastAPI remains the oracle. The following suites pass against the same disposable PostGIS fixture:

- `check_outbox.py`: durable staging outbox visibility/retention/RLS, Rust replay integration and rollback.
- `check_realtime.py`: activity cursor, human/service auth, project SSE and organization-chat SSE.
- `check_managed.py`: R029 managed QGIS session/bootstrap/delta/commit/receipt/command/SSE semantics.
- `check_team.py`: project collaboration REST, leases, reviews and rebase semantics.
- `check_org_chat.py`: organization chat and people/contact semantics.
- `check_listener_cap.py`: project/chat/managed LISTEN caps.
- `check_presence.py`: FastAPI/Rust close-code parity and the 18-event presence scenario.

Additional takeover regressions:
- WebSocket transport accepts extended frames above 64 KiB up to the ASGI-compatible 16 MiB receive ceiling; the existing 2048-character presence policy is then applied at the message layer.
- Organization people search now uses full non-Turkic Unicode casefold before Vietnamese `đ -> d` and NFD accent stripping, matching Python `str.casefold()` for expansions such as `ß -> ss` and final sigma normalization.
- The E11 dedicated LISTEN connection and the convergence runtime-role readiness hardening are both retained in `DbPool::dedicated`.

## Local verification

- `cargo check -p openvietgis-api-rust`: pass.
- E11 presence unit suite: 4/4 pass.
- Organization-chat fold regression: pass.
- Clippy for `openvietgis-api-rust` with `-D warnings`: pass.
- E11 staged replay route test with `e11-candidate`: pass.
- All seven E11 differential parity scripts above: pass.

## Remaining release gates

E11-AC3 still requires recorded representative cold/warm p50/p95/p99, peak RSS and error-rate measurements. E11-AC4 independent review / dependency-license-security review also remains a release gate. These are evidence/release gates, not missing Rust route implementations.
