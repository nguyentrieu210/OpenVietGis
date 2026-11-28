#!/usr/bin/env bash
# Fetch immutable dated Geofabrik extract; never promote partial/mismatched bytes.
set -euo pipefail
umask 077
date_id=${1:?use YYMMDD}
root=${2:?absolute new snapshot directory}
[[ "$date_id" =~ ^[0-9]{6}$ && "$root" = /* ]] || exit 2
mkdir -p "$root"
cd "$root"
test ! -e vietnam.osm.pbf
url="https://download.geofabrik.de/asia/vietnam/vietnam-${date_id}.osm.pbf"
tmp=$(mktemp .vietnam.XXXXXX)
trap 'rm -f "$tmp"' EXIT
expected=$(curl -fsSL --proto '=https' --max-time 30 "$url.md5" | awk 'NR==1{print $1}')
[[ "$expected" =~ ^[a-fA-F0-9]{32}$ ]]
# No resume/retry that can append inconsistent content. Retry whole script instead.
curl -fsSL --proto '=https' --connect-timeout 15 --max-time 900 -o "$tmp" "$url"
printf '%s  %s\n' "$expected" "$tmp" | md5sum -c
mv "$tmp" vietnam.osm.pbf
sha256sum vietnam.osm.pbf > SHA256SUMS
printf '%s\n' "$url" > SOURCE
date -u +%FT%TZ > FETCHED_AT
