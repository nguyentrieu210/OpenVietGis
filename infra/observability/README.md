# Spec 066 aggregate operations metrics

Load spec066.rules.yml in the existing Prometheus installation after operator review.
No monitoring service is started by this repository. Scrape /api/v1/ops/metrics
from the internal API endpoint. Keep the deployment's existing access controls.

The recording rules expose p95/p99 request latency, 5xx rate, resident memory,
private tile-cache occupancy and repeat-request hit ratio. Histogram buckets
extend through 30 seconds; values above that retain the +Inf bucket. Quantiles
are estimates over the last five minutes, not exact individual-request timings.
No route, account, organization, project, feature ID, token or URL is a metric label.

Cache hit ratio counts hits / (hits + misses + waits + capacity bypasses).
Only eligible unscoped native-4326 tile requests enter those counters.
Restricted GIS scopes bypass this cache and still execute authorized RLS reads.
Each lookup revalidates access, source binding, source readiness, revisions and
render support; a waiting request releases its DB transaction and reauthorizes.
The cache retains at most 64 entries / 32 MiB for 60 seconds and 64 active keys.
A separate response holds shared immutable bytes, so a hit does not copy an MVT.

The RSS warning covers the API processes selected by the scrape job. It does not
certify the complete deployment's memory budget or a 60-minute soak. Keep separate
jobs for different environments; do not aggregate staging and production together.

Build immutable release candidates also records qualification.json for each role
and a combined release-qualification.json for the exact source/run/attempt. The
duration covers one BuildKit build-and-publish action, including SBOM/provenance.
It contains identification/timing fields only and is not deployment approval.
Deployment duration and live verification belong to the authorized deployment
receipt; no production deployment has been inferred from build timing.
