# Performance — evidence policy

Implementation scope: Skip quiet delta event scan, coalesce bounded source events, cap operation and alias-delivery counts. Preserve existing performance thresholds and measure on isolated PostGIS fixtures without production data.

Run evidence: Existing authorization and spatial HTTP benchmark profiles; record actual output and failures. See [candidate runbook](../../docs/community/CANDIDATE_RUNBOOK.md) for exact candidate logs. Local checks are not release approval.

Open: Full 10k dataset performance if not completed, native QGIS UI latency and production-sized CAD/3D, host budget and repeated load testing.
