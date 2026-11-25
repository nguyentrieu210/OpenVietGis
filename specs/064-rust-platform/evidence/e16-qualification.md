# E16 qualification — 2026-11-26

Branch: 064-e16; parent 1a5cb7b40a8a0c8117b8aa56c1522e35d01a5b3b;
baseline 3e78ac142acfc050ec22d30da5c44ad0e6b8459a.

## Decision and scope

Preserve the current MCP SDK and OAuth/HTTP/stdio adapters, without changing public
tool names, routes, role checks, revision pins or browser consent. The isolated
dependency-free Rust policy candidate implements navigation bounds and bridge
state transitions. It is NOT linked to the running adapter and is not an
authorization engine. The hybrid boundary in engine-scope.md retains the deployed Python SDK and
adapters. The Rust policy experiment made no case for changing that boundary.
No production deployment or new permission grant.

## Evidence and task status

- T01 / AC1: existing inventory maps 44 tool entries, 19 bridge endpoints and
  29 mapped test IDs. Marked complete.
- T02 / AC2: shared navigation fixture (9 cases), Rust lifecycle cases, and
  Python malformed JSON/JWT regression fixtures. Existing tests exercise
  HTTP/stdio discovery, consent, ownership isolation, stale/expiry and advanced
  tool validation. Full operation-by-operation differential coverage remains open.
- T03 / AC2: hardened retained adapter against duplicate JSON members,
  non-finite values (including overflow), excessive nesting and invalid JWT
  NumericDate types; locked subject lookup and relay submission/status together.
  Native policy candidate is independently compilable and remains an
  experiment. Retained adapter CI runs the MCP contracts on Spec 064 branches.
- T04 / AC3: measured retained adapter against the pinned baseline, three fresh
  processes each, 100 samples per operation/concurrency (1 and 4), 3,600 total
  warm samples, zero errors. Raw latencies, process cold-start and peak RSS in
  e16-benchmark.json. Cold startup includes import, fixture RSA generation and
  first operations; peak RSS is whole process, not per operation. These are
  in-process synthetic operations, not external OAuth/Keycloak or browser RTT.
  An independent, isolated 4 GiB profile for the retained adapter remains
  open. VPS shared load varies markedly, so no speedup is claimed.
- T05 / AC4: existing MCP HTTP/stdio tests plus native tests passed. No independent
  review, real Keycloak/browser consumer smoke, E01/E02/E03/E11 integrated
  permission/revision/realtime verification or release rollback rehearsal.
  Acceptance remains blocked.

## Validation

- `uv sync --offline --frozen --project apps/mcp --python /usr/bin/python3`
- `apps/mcp/.venv/bin/python -m pytest -q apps/mcp --disable-warnings`
  (44 passed; baseline before changes 34 passed).
- Rust 1.85.1 offline compiler in cached Docker image, 384 MiB RAM, 0.5 CPU:
  `rustc --edition=2021 --test crates/ovg-mcp-policy/src/lib.rs`
  (2 tests passed). rustfmt is absent from cached image and was not run.
- No new dependencies (Rust candidate std only); existing Python lockfile unchanged.
  No supply-chain vulnerability audit was performed.
- Rollback: revert this branch commit in integration, retaining unchanged Python
  public contracts; candidate has no runtime linkage or migration. No live services
  or credentials were modified.

## Next gate

Verify all 44 retained tool registrations and their payload/error contracts
against the hybrid API with E01/E02/E03/E11. Exercise real browser consent,
OAuth and interoperability flows with the retained SDK, then record independent
review and representative cold/warm resource evidence before accepting E16.
