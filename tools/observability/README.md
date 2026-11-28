# OpenVietGIS operational observability

This profile separates measurements by source and keeps telemetry free of tenant,
user, project, feature, request-path, URL, and attribute values. It is a
structured measurement contract; it does not claim that provisional performance
budgets have passed.

## Current runtime measurements

The Rust API's `/metrics` endpoint exposes an unlabeled HTTP duration histogram,
request and 5xx counters, database readiness, processing-job count, process RSS,
and uptime. The histogram supports Prometheus quantiles without route labels:

```promql
histogram_quantile(
  0.95,
  sum(rate(ovg_http_request_duration_seconds_bucket[5m])) by (le)
)
```

```promql
histogram_quantile(
  0.99,
  sum(rate(ovg_http_request_duration_seconds_bucket[5m])) by (le)
)
```

```promql
ovg_process_resident_memory_bytes
```

```promql
sum(rate(ovg_http_requests_5xx_total[5m]))
/
clamp_min(sum(rate(ovg_http_requests_total[5m])), 1)
```

Do not add URL, route, actor, organization, project, layer, feature, or request
labels to these metrics. The histogram is intentionally global to the process
to bound cardinality and avoid disclosing request context.

The GIS performance panel is local to the current browser workspace, keeps at
most 60 in-memory samples, and reports p50/p95/p99 latency, cache hit rate,
cache bytes, invalidations, and failures. It is not sent to the API. A browser
cache rate must not be presented as a server-cache rate.

## Immutable build and deployment observations

The T043 release-observation artifact contract is `release-observability.json`
for an exact source SHA and workflow run. The T040 workflow wiring will record
build timing and per-image digest/timing. Deployment status is explicit:
`not_run` uses null deployment timestamps, environment, and duration; an
attempted deployment records its observed duration and whether it completed or
failed. A build-only workflow must never synthesize a deployment duration.

Validate one artifact before using it:

```sh
python3 tools/observability/release_metrics.py validate release-observability.json
```

Aggregate retained artifacts to calculate nearest-rank p50/p95/p99 durations:

```sh
python3 tools/observability/release_metrics.py summarize artifacts/*/release-observability.json
```

After an exact-digest deployment attempt, write a separate deployment observation
without mutating the build artifact:

```sh
python3 tools/observability/release_metrics.py record-deploy release-observability.json \
  --output deployment-observability.json \
  --environment ephemeral-github-actions-staging \
  --status completed --started-at "$DEPLOY_STARTED_AT" \
  --completed-at "$DEPLOY_COMPLETED_AT" --seconds "$DEPLOY_SECONDS"
```

The aggregate contains sample counts, build/deployment quantiles, and success
and failure counts; it omits raw commit and run IDs. Deployment quantiles include
every started deployment, including failed attempts, to avoid success-only
timing bias. Keep build and deployment populations separate. Do not call a
quantile a performance-budget pass unless the sample count and workload meet
`specs/066-core-performance-cleanup/performance-budget.md`.

The parser allow-lists operational fields and rejects unapproved fields,
malformed durations, unpinned images, and inconsistent deployment states.
Release observations may contain only the source commit, workflow run ID,
image references/digests, environment, timestamps, and measured durations.
Never include environment variables, credentials, tenant data, user data,
request paths, or rendered deployment configuration.
