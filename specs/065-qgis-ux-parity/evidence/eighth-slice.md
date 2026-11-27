# Eighth implementation slice
Base fd3111d6fd82851e2a4809541fc35788d9151290.

- TABLE-04/05: shared canonical attribute parser validates int32, exact decimal precision, explicit nullable fields, boolean and RFC3339 calendar dates. ATTR_EDIT uses the existing advanced editor and Rust changeset endpoint for one field on a selected feature; bulk dialog also gains typed validation. No new inline grid or full-form editor.
- EDIT-03/08: active/read-write layer and revision checks before first submission, local layer lock, beforeunload protection for attribute/bulk dialog, verify changeset receipt status/count/revision. Ambiguous retry retains immutable planned payload and operation ID; definitive first-attempt rejection permits cancellation/correction.
- TypeScript PASS, 70 Vitest and 36 Node tests PASS, production build exit 0, targeted synthetic Chromium exit 0 (invalid int blocked; intercepted 503 then retry with identical body and operation ID).
- Real authenticated Rust/PostGIS changeset rollback, two-user conflicts, and QGIS Desktop paired evidence remain unexecuted. No acceptance promotion.
