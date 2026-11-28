# Wave 2 — native agent runtime

Depends on Wave 1 PR #105 at b50914b30dbedb920fc2a3f3dca9e917a83a1d24. Upstream RustGIS assistant/agent.rs at aea84131974b57ff6359666dbe79f88054a64bf1, Apache-2.0. The conversation loop and retry limits are adapted; desktop Session is not copied.

- W2-01: provider-independent request/reply and typed tool observations.
- W2-02: 16 model steps, up to 3 retries per model request only, 32 calls, 4-call batches, bounded context/time and cancellation.
- W2-03: trusted host-pinned workspace and explicit tool allowlist, full batch preflight using registry; no model-selected session.
- W2-04: existing OAuth and browser dispatcher, serial calls, no automatic replay of failed/uncertain operations; exact-ticket status polling.
- W2-05: distinct complete, failed, pending, cancelled and timeout results; preserve uncertainty.
- W2-06: selected Wave 1 skill, model-visible schemas, regression tests and signed OAuth/browser bridge fixture.

Acceptance: cargo test --locked --manifest-path apps/mcp-rust/Cargo.toml and release build on approved runner. No provider, credential, public run route or UI entry until Wave 3. No live deployment claimed.
