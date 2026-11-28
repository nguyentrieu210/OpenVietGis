#!/usr/bin/env bash
set -euo pipefail
root=${1:?snapshot directory}
avail=$(awk '/MemAvailable:/{print $2}' /proc/meminfo)
disk=$(df -Pk "$root" | awk 'END{print $4}')
# Conservative staging thresholds, not measured import requirements.
(( avail >= 12*1024*1024 && disk >= 60*1024*1024 )) || { echo "BLOCKED: need >=12 GiB available RAM and >=60 GiB free disk"; exit 3; }
(cd "$root" && sha256sum -c SHA256SUMS)
for key in NOMINATIM_IMAGE JAVA_IMAGE VALHALLA_IMAGE; do
  value=${!key:-}
  [[ "$value" =~ @sha256:[a-f0-9]{64}$ ]] || { echo "Missing digest pin: $key"; exit 4; }
done
test -f "${PHOTON_JAR:?verified jar required}"
printf '%s  %s\n' "${PHOTON_SHA256:?verified release digest required}" "$PHOTON_JAR" | sha256sum -c
