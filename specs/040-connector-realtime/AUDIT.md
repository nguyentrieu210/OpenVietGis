# Connector & Realtime — evidence policy

Implementation scope: Coalesce same-source changes at final verified revision; deliver to active aliases; cap delta work; validate bootstrap/delta pins and distinguish delivery from caller acknowledgement. QGIS validates full envelope before provider updates and blocks saving partially applied remote state.

Run evidence: Managed delta tests, isolated R029 PostGIS, QGIS stdlib tests and deterministic ZIP. See [candidate runbook](../../docs/community/CANDIDATE_RUNBOOK.md) for exact candidate logs. Local checks are not release approval.

Open: Native Windows QGIS, background network I/O/UI latency, real reconnect/conflict, snapshot consistency under concurrent activity, legacy connector parity.
