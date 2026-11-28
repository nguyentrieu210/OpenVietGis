# Generic reference contribution
Only declarative JSON. No dependencies on native core, credentials, HTTP or filesystem. The existing registry must create a v1 manifest first; publisher and operator sign a linked v2 envelope. The host supplies the allowlisted core.read command and checks current data permission. Disable closes the runtime, not the underlying GIS data.
This sample is intentionally not an installed release and ships no signing key. Use the SDK fixture harness to validate a signed sample. See packages/flutter-plugin-sdk/README.md.
