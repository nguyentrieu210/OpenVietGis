# Native geometry kernel
Rust 1.85.1, wasm32-unknown-unknown, no third-party dependencies. Rebuild:
`rustup target add wasm32-unknown-unknown --toolchain 1.85.1` then
`bash tools/geometry-wasm/build.sh`. The checked-in artifact lets existing
Node-only frontend images build offline. Verify via the geometry-wasm parity test.
The kernel computes the existing approximate spherical distance and polygon area,
not survey-grade geodesics or a replacement for authoritative PostGIS operations.
The Web Worker falls back to the prior JavaScript implementation if loading or
validation fails.
