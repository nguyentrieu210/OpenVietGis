# E15 retention — owner hybrid decision

Base: 1a5cb7b40a8a0c8117b8aa56c1522e35d01a5b3b; authority: 6a448a7 and handoff-2026-11-26.md.

E15 is retained, not migrated. The frozen e15.json inventory records 72 HTTP routes, 79 files, 11 module migrations and 101 mapped test IDs. All historical E15 migrate labels are superseded by retain. Do not remove or reimplement endpoints. Land asset continues to use generic extension records; no Core business dependency. No change to production schema, module implementation or data.

| Task | State |
|---|---|
| E15-T01 | Inventory inherited; retention classification approved by owner |
| E15-T02 | Existing unit and real PostGIS fixtures retained; exact mounted-route and project guard regression passed on isolated PostGIS; hybrid gateway parity pending |
| E15-T03 | Existing FastAPI engine retained unchanged; hybrid gateway wiring is E18/E14 integration dependency |
| E15-T04 | No Rust comparison required; Python-through-gateway 30k benchmark not yet verified |
| E15-T05 | Pending independent review, hosted CI, E18 gateway smoke and client tests |

## Gateway handoff (blocking)
The current staging tile gateway defaults all /api/ to Rust. The hybrid integration must explicitly retain FastAPI for scoped /modules/house-numbering/ and /extensions/ routes, including exact path variants and all HTTP methods, not just GET. Keep generic official registration, module activation, land_asset records and marketplace dependencies on the existing API. Do not implement fallback on a Rust 404; dispatch before forwarding and preserve auth, cookies, CSRF, bodies, streaming and audit semantics. Test project A vs B, inactive module, private export permissions, stale revision, normal client downloads, and both official modules. Gateway config is shared with E18; no overlapping modifications here.

## Verification
Dedicated GitHub Actions job e15-retention.yml runs the retained Python module tests and tests/modules on disposable PostGIS. No build/test on VPS after reading handoff. An earlier experiment was performed against the stale local base before the owner decision was fetched: 17 Python domain tests and 9 experimental Rust tests passed, but those candidate changes are intentionally excluded from this branch; their debug benchmark is not acceptance evidence. The extended host suite was unavailable because dependencies were missing. CI evidence, gateway parity and review remain required. No live changes.

## Integration branch verification

Validated `064-clients-completion` at `cfe03f0` with the repository-pinned PostGIS
image on a disposable loopback-only database. Core and House Numbering migrations
completed. All 150 selected tests passed, covering the E15 module unit and
integration suites and coexistence with land assets, plus E13 Python reference
and E14 SDK/extension regressions. The exact mounted contract matched all 72
inventory method/path/handler tuples and required the project access guard.

Locked Rust 1.85.1 checks passed for `ovg-sync`, `ovg-module-sdk`, and
`ovg-module-house-numbering` (19 tests). Targeted rustfmt and all-targets Clippy
with warnings denied passed. Regenerating the E13 validation fixture from the
Python oracle produced 29 cases and matched the committed fixture byte for byte.
These results cover the assembled engine branch. The 30k HTTP benchmark through
the hybrid gateway and staging gateway/client qualification remain open.
