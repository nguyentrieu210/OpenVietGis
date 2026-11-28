# Immutable candidate images (Spec 066)
Status: proposed candidate pipeline; no production authorization, independent
release certification or actual GHCR build has yet been recorded.

## Build once
Run the "Build immutable release candidates" workflow manually on main at the
reviewed commit. It builds four linux/amd64 images, publishes them to GHCR with
unique source/run/attempt tags, attaches BuildKit provenance and SBOM, and emits
a manifest containing digest references. It does not deploy. Configure registry
package access separately; a registry write permission failure blocks the run.
Use the full Rust profile for the Rust-live API; core-only remains the default
for the historical hybrid staging fixture. The Python migration image is still
required. This workflow is not a replacement for existing CI or release gates.

## Exact candidate / same image for staging and deployment
Obtain the receipt from the trusted GitHub Actions run for the independently
reviewed candidate SHA. Verify its origin, SBOM and provenance and complete
security, release and staging gates. The JSON is an identification record, not
an approval or signature. Never obtain the expected SHA from an untrusted
receipt. Keep the previous receipt and previous images for rollback.
The script validates repository, exact source, architecture, image role and
digest shape. It does not authenticate the receipt itself.

Example commands for an authorized operator in an isolated staging checkout,
with approved registry read access (no credentials in command history):
```sh
set -eu
repo=nguyentrieu210/openvietgis
# Set SHA independently to the exact approved source commit.
: "${SHA:?Set independently verified candidate SHA}"
python3 infra/release/image_manifest.py release-images.json \
  --repository "$repo" --candidate "$SHA" > release-images.override.yml
python3 infra/release/image_manifest.py release-images.json \
  --repository "$repo" --candidate "$SHA" --format refs > image-refs.txt
while IFS= read -r ref; do docker pull "$ref" || exit 1; done < image-refs.txt
python3 infra/release/image_manifest.py release-images.json \
  --repository "$repo" --candidate "$SHA" --format verify-local
```

The override requires Compose >=2.24.4 (verified with 2.26.1). It is for the
Rust-live service layout (api/web/migrate/postgres), not the historical
infra/prod/docker-compose.prod.yml hybrid fixture. The observed VPS live
Compose is separately managed and is not in Git; do not copy credentials,
rendered config or deployment files into this repository. An operator must
review the live profile, port 8000 Rust API, volumes, health checks, networking,
identity and migration service before adopting the override.

Use the existing approved project name, environment and base Compose arguments
as appropriate; pipe resolved config directly into the verifier, never log it.
```sh
set -o pipefail
docker compose -p "$PROJECT" --env-file "$ENV_FILE" \
  -f "$BASE_COMPOSE" -f release-images.override.yml config --format json |
  python3 infra/release/image_manifest.py release-images.json \
    --repository "$repo" --candidate "$SHA" --format verify-compose
```
The verifier refuses hybrid profiles and any remaining local build. Override
sets pull_policy: never and removes build with !reset; explicit digest pull and
local verification are separate. No application startup command is executed by
this tool.

## Rollback and approval gates
Before rollout: record prior digest receipt and deployment configuration; review
schema and backward compatibility; verify backup restore on disposable data;
qualify the identical digests in isolated staging, including auth/permissions,
GIS/CAD/offline, health and migrations. Do not run migrations automatically as
part of an app-only deployment, recreate postgres/identity, or delete volumes.
After authorized rollout use no-build/no-pull, verify actual service image
IDs/digests and run post-release checks. If checks fail, stop and return to prior
app images/config only when schema compatible. Otherwise follow tested database
recovery with explicit approval. No automatic database rollback. Keep previous
image artifacts and backups until acceptance. Rollback rehearsal on these exact
images and live-profile adoption are still outstanding.

## Verify an authorized app rollout or rollback without rebuilding

Use the trusted receipt for the exact SHA after the authorized API/web operation.
The read-only verifier requires exactly one healthy API/web container in the
explicit existing Compose project and compares actual image IDs, digest references,
architecture and OCI revision. It never pulls, builds, starts, stops or migrates
services, and never prints container environment variables.

```sh
python3 infra/release/image_manifest.py release-images.json \
  --repository "$repo" --candidate "$SHA" --format verify-running --project "$PROJECT"
```

Use the retained previous receipt/SHA for an authorized app rollback. Schema
compatibility and authentication/GIS/CAD/offline smoke remain separate gates.
Missing health, duplicate app containers and digest mismatch fail closed.

Per-role qualification.json and combined release-qualification.json record one
build/publish interval with exact source, digest, run and attempt. These records
grant no approval. See [observability](../observability/README.md).

Execution is restricted to VPS1 and GitHub. VPS1 is deploy-only: compilation,
development dependencies and final builds belong to GitHub Actions. The VPS
connector is not an authorized build runner.
