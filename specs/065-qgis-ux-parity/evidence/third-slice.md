# Third implementation slice
Base c742a0c648b291a91d2af0dede4e340493cbdc89.

- DOCK-05/06: Shift ranges exclude collapsed/filtered rows; keyboard navigation and multi-selection; mixed group checkbox shows hidden layers without hiding visible ones.
- DOCK-06/COLLAB-07: context menu keyboard navigation, focus restoration, bounded menu, F2 layer rename vs command history; Escape cancels rename.
- DOCK-10: distinguish layer removal from deleting source data; gate edit menu by layer capability and lock.
- EDIT-01/SHELL-05: GIS hotkeys do not intercept CAD/3D or Space typed into form controls; guard repeated destructive/save keys.
- TABLE-02: support up to six typed conditions combined by AND/OR and optional NOT with existing Rust query contract; source/offset limits remain.
- TypeScript PASS; Vitest 59/59, Node 32/32. Vite build and offline shell generation observed. Synthetic Chromium (qgis-parity-keyboard-smoke.mjs) PASS against built canonical frontend with intercepted API fixture. Not evidence of authenticated Rust/PostGIS integration or QGIS paired parity.
