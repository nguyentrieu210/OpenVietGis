# OpenVietGIS Desktop — Wave 1 candidate
Standalone desktop host under SPEC 074, with the existing web-next Vite/React application as the sole frontend build source. The desktop entry is an additive local workspace panel and empty MapLibre map, not full shared GIS workbench parity. Web entry and HTTP transport are unchanged when VITE_OVG_DESKTOP is unset.

Build from repository root with the project Node/pnpm versions:
- pnpm install --frozen-lockfile
- Set VITE_OVG_DESKTOP=1 in your shell, then pnpm --filter @openvietgis/web-next build
- cargo generate-lockfile --manifest-path apps/desktop/src-tauri/Cargo.toml
- cargo test --locked --manifest-path apps/desktop/src-tauri/Cargo.toml
- cargo run --locked --manifest-path apps/desktop/src-tauri/Cargo.toml

Tauri runtime/toolchain resolution and lockfile still exploratory until W0 qualification. The shared engine and LocalStore use one exact 073 git revision (2acf58d897bb093966f18639d087ef904a72a720) while PR 103 remains unmerged. Replace the temporary git dependency with the integrated path dependency after ownership review, not a copied engine.

The host creates its own app-local standalone projects, using UUID-based directories and real GeoPackage store; no arbitrary path, Cloud credentials, managed data, shell or network API. Create/open/close and native CRS jobs are typed IPC. W1 data plane is fail-closed pending W2 real tile provider. No remote access, installer, arbitrary local file import, protected sync, full rendering or performance claims. Do not use with real protected data. No release until platform and security qualification.
