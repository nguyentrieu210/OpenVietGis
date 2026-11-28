# Spec 066 source closure before the final GitHub build

Source baseline: f93a6b2a3daac3ffcce6c40971bd7d212e0e7815.
This is implementation evidence, not release certification or ADR acceptance.

## Implemented source

- T023: private MVT cache with 64 entries / 32 MiB, 60-second TTL and 64 active
  keys. Storage checks current authorization/RLS, source binding, readiness,
  revisions and render support before providing a scope. Restricted GIS actors
  bypass it. Waiters release transactions and reauthorize after retained
  completion. Shared immutable Bytes avoid hit copies; cancelled leaders wake
  waiters. Rust tests cover bounds, expiration, identity/revision isolation and
  delayed polling. The prebuilt HTTP candidate also has a disposable-DB gate for
  warm entries, foreign/anonymous access, invalid bearer precedence, stale pins
  and expired sessions.
- T039/T045: R029 executes in QGIS pytest, with a test inspecting that exact step.
  MCP discovery exercises its CLI entry point in-process, keeping strict schema
  and exact-source assertions without a second RSA/MCP runtime.
- T040/T043: per-role and combined build timing receipts reject mixed source,
  role, digest, run or attempt. Aggregate cache metrics and latency buckets
  through 30 seconds feed p95/p99, 5xx, RSS and cache recording rules. No scope
  IDs, URLs, names or tokens are labels.
- T048: running-image verification reads only metadata and requires healthy,
  unique API/web containers matching immutable digests, image IDs and OCI source.
  It cannot build, pull, start services, migrate or change DB/identity.
- T044: cold GIS uses the requested sample count. Continuous warm-context soak
  records minute DOM/JS-heap samples. The raw-sample gate rejects source mismatch,
  nonfinite values, fewer than 30 samples, exceeded p95 budgets or discontinuous
  one-hour coverage. API/GPU/RSS/RUM and reviewed exceptions remain separate.
- Docker bootstrap approves installation scripts only for the explicitly pinned
  pnpm package. npm's default script blocking had prevented the pnpm launcher
  from being created. No blanket script approval is enabled.

## Observed checks before the GitHub-only build

49 focused manifest/running-image/workflow/metrics/sample-gate Python tests passed.
Ruff, Rust 1.85.1 rustfmt parsing/formatting, benchmark syntax and whitespace checks
passed. The two MCP parity tests passed in 1.74 seconds after the in-process fix.
The additional pnpm guard awaits the final GitHub checks. No final application
artifact was compiled or certified by those source checks.

## Allowed execution and cleanup

The user restricts this task to VPS1 and GitHub. VPS1 is deploy-only. Compilation,
dependency setup and final build/test jobs must run in GitHub Actions; the VPS
connector is forbidden. Temporary preparation on that connector was stopped and
removed: dist/spec066-final, the task-created API/MCP virtual environments and
/tmp/ovg066-final-transfer. No task-owned build containers remain. The attempted
final build stopped at source cleanliness before compiling any application.

Original source history and unrelated dirty CAD files on VPS1 were preserved.
GitHub publication uses the current public main as parent and imports the source
snapshot; it does not force-update main or rewrite existing GitHub history.

## Gates still requiring actual evidence

Finish all source, freeze a candidate, build once on GitHub and reuse artifacts.
Backend retirement needs complete parity and owner ratification of ADR-015;
Python stays retained. Exact CI, full load/soak, independent review, registry
SBOM/provenance, same-digest staging and authorized live release need actual
receipts before their task boxes are checked. Historical rollback images do not
qualify a new registry candidate. No merge, deployment or release approval is claimed.
