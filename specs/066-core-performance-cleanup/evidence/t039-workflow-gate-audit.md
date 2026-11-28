# T039 workflow-gate audit — in progress

Audit base: `3644db354c38f62974549f44246ad811d76ba525` (main at inspection time).

Changes in this branch:
- Normalize every GitHub Actions workflow to one top-level concurrency policy. Only superseded pull-request runs are cancelled; push and manual runs use unique run-id groups.
- Remove duplicate top-level `concurrency` mappings from CI, E17 and Rust MCP workflows. Duplicate YAML keys had been silently collapsed by the existing PyYAML tests.
- Run the E06/E08 and E11 PostGIS qualification workflows on main and when root Cargo manifests, toolchain configuration, Rust API, crates, or parity test sources change.
- Add static regression tests for unique concurrency policy, unfiltered root CI coverage, and Rust-trigger coverage for those PostGIS workflows.

Validation limitation:
- The connected GitHub integration returned HTTP 403 for `branches/main/protection`; the repository ruleset endpoint returned no rulesets. The required-check list therefore could not be verified from this account, and T039 remains open until a maintainer confirms the branch-protection required status checks and exact-candidate workflows pass.
