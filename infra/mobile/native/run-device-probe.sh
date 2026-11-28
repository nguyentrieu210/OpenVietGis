#!/usr/bin/env bash
set -euo pipefail
# Use an explicitly selected non-production Android test device.
: "${ANDROID_SERIAL:?Set ANDROID_SERIAL to the authorized test device serial}"
artifact="${1:?native probe artifact directory}"
expected="${2:?expected page size 4096 or 16384}"
case "$expected" in 4096|16384) ;; *) exit 2;; esac
adb shell getconf PAGE_SIZE | tr -d '\r' | grep -Fx "$expected"
adb shell getprop ro.product.cpu.abi | tr -d '\r' | grep -Fx arm64-v8a
remote="$(adb shell mktemp -d /data/local/tmp/ovg073.XXXXXX | tr -d '\r')"
[[ "$remote" =~ ^/data/local/tmp/ovg073\.[A-Za-z0-9]+$ ]]
trap 'adb shell rm -rf "$remote"' EXIT
adb push "$artifact/ovg_native_probe" "$remote/ovg_native_probe"
adb push "$artifact/libc++_shared.so" "$remote/libc++_shared.so"
adb push "$artifact/proj" "$remote/proj"
adb shell chmod 700 "$remote/ovg_native_probe"
adb shell "LD_LIBRARY_PATH='$remote' '$remote/ovg_native_probe' '$remote/proj' '$remote/probe.gpkg'" | tee "probe-${expected}.log"
grep -Fx NATIVE_PROBE_PASS "probe-${expected}.log"
adb pull "$remote/probe.gpkg" "probe-${expected}.gpkg"
