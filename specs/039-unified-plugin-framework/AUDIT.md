# Plugin contract hardening evidence
Baseline 86a58b3. No production change.

## Findings and implementation
- Frontend previously selected an official module by extension_key alone and could reuse a different registered version. Activation response now carries manifest_version and runtime_compatible for shipped executable modules. Frontend requires exact installed version and positive runtime compatibility. Mismatch falls back to Core and disables opening that module with explicit explanation.
- Official discovery now validates manifest contract before importing module and rejects partner namespace. Generic HTTP registration cannot claim a shipped executable runtime key. Existing same-version registration must match actual shipped contract; conflicting legacy metadata returns conflict rather than silently overwriting history.
- Activation and transition HTTP adapters provide the repository a runtime validator, after authorization/metadata checks and before mutation. Domain helper matches key/version/API compatibility, permission identifiers, command/capability declarations, schemas and UI contributions; enabled required. These are metadata contract comparisons, not code-signing.
- Shared module SDK checks active tenant/project manifest join against shipped contract. House Numbering uses this check after its existing scope authorization. Metadata-only Land Asset is not an executable runtime and retains its generic path. External partner sandbox, approvals and marketplace remain separate.
- Synthetic browser fixtures updated to represent explicit compatibility pins; benchmark seed uses shipped manifest when creating a new fixture registration. Existing benchmark DB rows were not modified.

## Verification
- 35 focused Python unit/architecture tests passed (discovery, extension metadata boundary/history and architecture fixtures).
- Three integration tests passed on fresh loopback-only PostGIS 18-3.7 with repository test role initialization, Core migration through 0042 and module migration: generic lifecycle authority/retention, external partner manifest security/activation, official runtime exact contract including legacy same-version collision, denial of reactivation and disable.
- Chromium spatial-mode smoke on canonical local app with synthetic API passed existing toolbar/GIS/CAD/3D/plugin/revocation paths and added incompatible plugin fallback plus disabled open button. No captured page errors.
- web-next production build/typecheck, targeted Ruff, architecture and documentation checks passed. Other adjusted browser fixtures were not individually executed.

## Remaining
A production compatibility audit of existing manifest rows and a reviewed data-preserving migration are required before rollout. Strict matching can intentionally make a mismatched legacy official module unavailable until remediated; no automatic overwrite, transition or deletion. Only shipped executable module integration was exercised, not complete community plugin compatibility. Runtime contract is metadata, not signed immutable code or a proof of permission enforcement for every command. Full Marketplace/iframe/webhook security and plugin migration/rollback, all module workflows and release certification remain open.
