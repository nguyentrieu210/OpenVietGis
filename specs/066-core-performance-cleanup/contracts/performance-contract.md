# Behavioral contracts
Direct route: resolve URL and approved saved context before engine creation; auth gate before protected request; unauthorized destination returns existing safe flow.
Bootstrap: same scoped request in flight is shared; stale responses cannot mutate newly active workspace; cancellation propagates.
Cache: authorize before access and conditional response; scope/version mismatch is miss; invalidation idempotent; no public sharing of protected entries.
Realtime: sequence monotonic per stream; duplicate no-op; gap resumes or fetches bounded scoped snapshot; acknowledged edits persisted before success UI.
Rendering: display LOD never replaces source geometry; inactive engine stops loop, releases listeners/workers/GPU subject to safe draft retention.
Release: commit SHA + digest + SBOM + tests -> immutable artifact; staging and release consume identical digest; rollback retains compatible database and old assets.
No API schema or URL changes by default. Any necessary change requires SDK/contract versioning and compatibility tests.
