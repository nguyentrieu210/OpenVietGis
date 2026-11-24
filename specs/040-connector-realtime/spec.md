# Spec 040 — Connector & Realtime
Parent R006/R011/R013/R015; stabilization program Spec 030.

Coalesce same-source changes at final verified revision; deliver to active aliases; cap delta work; validate bootstrap/delta pins and distinguish delivery from caller acknowledgement. QGIS validates full envelope before provider updates and blocks saving partially applied remote state.

Acceptance evidence: Managed delta tests, isolated R029 PostGIS, QGIS stdlib tests and deterministic ZIP.

Remaining release obligations: Native Windows QGIS, background network I/O/UI latency, real reconnect/conflict, snapshot consistency under concurrent activity, legacy connector parity.
