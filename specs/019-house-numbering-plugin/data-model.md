# Data model
All mutable entities carry organization/project ownership, stable ID, revision, actor and timestamps; all references resolve inside same scope. Each state-changing operation records audit/history. Geometry is a reference to a spatial resource with explicitly registered CRS, never silently guessed.

| Entity | Fields and invariants |
|---|---|
| Activation | manifest version, project, state enabled/disabled; disable preserves data |
| Area | stable ID, name, kind (current/historic), parent/geometry ref |
| Route | name, type street/alley, parent, prefix, geometry ref, direction, source, status; acyclic ancestry, nonempty oriented geometry; cannot remove if referenced; direction locked by submitted/approved numbers |
| Building | geometry ref, route/area/parcel ref, eligibility, reason, state, revision; no official number if ineligible |
| Evidence | building/route, origin certificate/CAD/survey, raw number, date/source ID, confidence, private metadata reference; raw immutable |
| AddressNumber | route/building, number string, kind old/certificate/CAD/survey/proposed/submitted/approved/reserved, rule version; protect submitted/approved/reserved; approved uniqueness within scoped route, preserve raw text |
| Rule | immutable version, direction/parity, suffix alphabet or dash, max distance, authority citation nullable; only authoritative rule permits official approval |
| Preview | operation, base revision(s), candidate changes, warnings, expiry, token; read-only, single atomic apply |
| Proposal | draft/submitted/rejected/approved status, items, comments, rule version, actor; approval gated, no duplicate official number |
| ImportBatch | source fingerprint, CRS, mapping, counts, original artifact ref, status; accepted+rejected=read, idempotency |
| ReconcileCase | evidence refs, class, estimate flag, resolution, actor; exactly one class per building, unmatched cert separate |
| Survey | client UUID, building, point/CRS, photos, state, recorded by, submitted by; unique client key, idempotent attachment content hash |
| Assignment/Target | scope, actor, date interval, target nullable; no percentage if absent/zero |
| RestrictedHousehold | purpose, consent/basis, protected identity fields, access scope and expiry; absent CCCD valid, no generic list exposure |
| Inspection/Certificate | scope/building, number, source/case, template revision, status, artifact, issue/reissue link; official only approved, draft watermark otherwise |
| Export/Audit | scope, explicit fields/format, artifact and manifest, actor/action/time; standard whitelist excludes PII, restricted read/export separately logged |
