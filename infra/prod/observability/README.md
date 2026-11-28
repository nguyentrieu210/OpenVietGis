# Spec 066 operational metrics (candidate)
 
Load `runtime.rules.yml` into an already authorized private Prometheus instance
after validating it with `promtool check rules infra/prod/observability/runtime.rules.yml`.
No scraper, public endpoint, credentials, or production configuration is installed
by this change. Use an OpenVietGIS-only scrape target set; do not combine unrelated
applications or environments.

| Panel | Series | Units |
| --- | --- | --- |
| API p95 and p99 | ovg:http_request_duration_seconds:p95_5m, ovg:http_request_duration_seconds:p99_5m | seconds |
| API traffic | ovg:http_requests:rate5m | requests/s |
| API server errors | ovg:http_errors:ratio5m | fraction |
| Peak current per-instance RSS | ovg:process_resident_memory_bytes:max | bytes |
| Tile cache decisions | ovg:tile_cache_hits:ratio5m | fraction |
| Tile cache utilization | ovg:tile_cache_bytes:sum / ovg:tile_cache_capacity_bytes:sum | fraction |
| Abandoned cache leaders | ovg:tile_cache_abandoned:rate5m | flights/s |

A zero denominator is no traffic/no sample, not a zero error rate or a successful
cache workload. Histogram quantiles are approximate aggregate HTTP percentiles,
not the small-authorized-read lab budget. Cache ratio counts hit, leader-miss and
bypass decisions; intermediate waits are deliberately excluded and noncacheable
paths are not counted. RSS is current per-instance memory, not historical peak;
use max_over_time for historical observation. No tenant, actor, project, source,
path or route labels may be added.

For build timing, retain the existing exact-source release-qualification.json
from the immutable GitHub workflow. Deployment duration must be measured by the
authorized operator around the actual rollout and paired with the independently
verified source SHA and running digest receipt. Do not infer deployment timing
from build duration or local health-check timing. Do not retain rendered Compose,
environment values, credentials or raw requests.

These rules are candidate code only: T043 remains pending until the private
scrape, rule validation, release build and rollout timing evidence are recorded.
