# Spec 039 — Unified Plugin Framework
Parent R014/R015; depends on Specs 033–038.

- Validate official manifests before importing shipped runtime; reject partner namespace from trusted executable discovery.
- Official runtime keys must be registered through official registration API. Existing same-version registration is idempotent only if the stored runtime contract matches shipped metadata.
- Activation and transition targets for official runtime must match shipped version, API compatibility, permissions, commands, capabilities, entity schemas and UI contributions. Recheck active contract at runtime entry.
- Activation list provides manifest version and official runtime compatibility. UI opens installed module only on exact compatible activation; report mismatch and retain GIS Core.
- Metadata-only modules and sandboxed partner registration remain distinct. Preserve existing lifecycle authorization, transition and retention rules.
- Validate focused domain, architecture, fresh PostGIS integration and browser regressions. No production data repair, deployment, new runtime or certification of all plugins.
