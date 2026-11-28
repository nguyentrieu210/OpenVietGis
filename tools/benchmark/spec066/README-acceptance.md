# T044 exact baseline acceptance protocol

The manual workflow '.github/workflows/spec066-acceptance.yml' takes an explicit 40-character candidate SHA and checks it out detached beside the pinned baseline '5bfc83826161acd83b98a925e85690cebde53ae1'. It runs on the same 'ubuntu-24.04' GitHub Actions runner with Node 26 and pnpm 12.21.0, records OS/CPU/memory and exact source revisions, and uploads the raw artifact directory even when a profile fails.

## Profiles

- Frozen install and canonical web build for baseline and candidate.
- Existing spatial-workspace and web-next test suites plus E17 contract tests for each source tree.
- E17 cold/warm microbenchmarks; these retain 30 independent cold subprocess samples and 100 warm samples.
- ModeCoordinator lifecycle soak for each source tree: 5 phases x 1,000 switches, explicit GC, retained-heap checks.
- Browser shell, GIS and initialized CAD/3D transition profile for each built source tree. Every phase has 30 independent samples for p95; p99 is descriptive only at this sample size. The raw arrays, failures, OS, Node, Chromium version and 1280x850 viewport are retained. API and assets are served from a local synthetic fixture; external provider traffic is blocked.
- Candidate-only disposable PostGIS SQL experiment (the pinned baseline does not contain this fixture script), network-isolated with 2 CPU/1 GiB, synthetic tenant rows only. Every measured query uses 30 EXPLAIN ANALYZE runs and records raw times, p50/p95/p99, representative plans, result hashes and parity/security-fixture checks. With 30 samples, p99 is descriptive only; use at least 100 samples before treating it as a p99 estimate. The image tag and observed image ID/repository digest are artifacts.

Artifacts are evidence, not an automatic budget verdict. The targets in 'performance-budget.md' remain provisional; the workflow never labels them as passed. The final task remains partial until an exact merged candidate has run through this workflow and any unmet target has a reviewed exception. It does not deploy or contact production.

To dispatch, select **Spec 066 exact baseline acceptance** and pass the exact candidate commit SHA. For a final run, dispatch the workflow from the merged candidate's workflow definition and use that same merged SHA as 'candidate_sha'. Review the JSON and logs from 'spec066-acceptance-<sha>-<run_id>'.
