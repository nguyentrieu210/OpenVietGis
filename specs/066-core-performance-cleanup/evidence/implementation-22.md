# 066 implementation slice 22 — bounded cache and live authorization gates

Candidate source SHA: d4bb428dbca36fd99ad3e19fe353a94b39c02d60 (T023 stacked on PR #85 integration head).

## T023 acceptance evidence

GitHub Actions run 46148518776 completed all relevant gates successfully on the candidate SHA:

- Rust Platform CI: Format, Clippy, workspace tests and architecture isolation PASS.
- PostGIS parity: complete parity job PASS, including the exact route log `PASS E05 cached tile rechecks revoked project grant`.
- Cargo Audit: PASS.

Cache capacity and singleflight Rust tests passed in the workspace-test log:

- `tile_cache::tests::bytes_entries_flights_and_oversized_values_are_bounded`
- `tile_cache::tests::cancelled_leader_releases_all_waiters_and_slot`
- `tile_cache::tests::delayed_poll_observes_completion_and_shares_bytes`
- `tile_cache::tests::identity_source_revision_and_address_are_isolated`

The PostGIS job's `Verify private tile cache authorization and current revisions` step passed. It exercises a concurrent cold burst and checks exactly one cache miss/leader, identical private tile responses, anonymous/foreign/bearer denial, stale-revision rejection, retirement, and revoked/expired session denial.

The added route regression uses a newly seeded project-viewer identity with no prior GIS ACL state, warms the same tile URL twice, revokes its project grant, then repeats that URL with the same session. The route responds 403 and returns neither MVT content type nor cached feature bytes.

T023's stated acceptance—memory cap, stampede protection, and authorization before cached reads—is covered by passing Rust unit and PostGIS route/security checks on the pinned candidate. This does not complete the broader SPEC 066 acceptance gates.