# Unified encrypted media — native key boundary

The Android host wraps a random 32-byte domain key with an AndroidKeyStore
AES-256-GCM master. Wrapped keys live in the application no-backup directory,
bound to the domain UUID via AAD. The iOS provider stores the domain key as a
generic-password item, WhenUnlockedThisDeviceOnly. Neither implementation
exports raw key bytes to Dart, logs, backup files, or SQLite.

The legacy vault did not implement persistent native key enrollment or an
authoritative attachment manifest. This integration does not recover those
missing keys and does not infer associations by enumerating files. Existing
scope keys must already be available to the trusted platform provider.

Manifest v1 example (replace all placeholders with verified UUIDs and SHA-256):
```json
{"version":1,"source_scope":"SOURCE-WORKSPACE-UUID","items":[{"layer":"DESTINATION-LAYER-UUID","feature":"DESTINATION-FEATURE-UUID","blob_id":"LEGACY-BLOB-UUID","ciphertext_sha256":"64-HEX-DIGEST"}]}
```
The native host resolves the legacy vault as
`<private local-workspaces>/<source_scope>/media/<blob_id>.enc`.
This is the new explicit migration profile, not a claim that legacy installs
already have this directory/manifest. No keys belong in the manifest.
The operator must reconcile each owner using trusted source records.
At most 16 records per atomic import. A destination domain with existing
ciphertext and missing key must fail; never regenerate a replacement key.
Restored backup may remain locked on a different device; authorized key
transfer is a separate unimplemented protocol.

Qualification required before enabling production:
- Android physical-device restart, app backup/restore, Keystore loss, corrupt
  wrapped key, incorrect scope, release JNI symbol retention.
- iOS signed-device lock/unlock, accessibility class, reinstall/restore,
  concurrent provisioning and app sandbox access.
- Validate real legacy manifest and source key provenance; AEAD tamper and
  all-or-nothing migration. No native image display or capture is provided.
- Host Linux tests only verify fail-closed unsupported behavior, not mobile
  platform security.
