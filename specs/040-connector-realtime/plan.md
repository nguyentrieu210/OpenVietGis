# Plan
1. Inspect inherited candidate and relevant contracts.
2. Implement bounded scope: Coalesce same-source changes at final verified revision; deliver to active aliases; cap delta work; validate bootstrap/delta pins and distinguish delivery from caller acknowledgement. QGIS validates full envelope before provider updates and blocks saving partially applied remote state.
3. Run and record: Managed delta tests, isolated R029 PostGIS, QGIS stdlib tests and deterministic ZIP.
4. Preserve blockers: Native Windows QGIS, background network I/O/UI latency, real reconnect/conflict, snapshot consistency under concurrent activity, legacy connector parity.
