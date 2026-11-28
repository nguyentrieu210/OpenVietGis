# W1 implementation checkpoint

Source work proceeds at owner request while W0 remains open. No W0 or W1 acceptance is claimed.
Native source baseline: PR #103 head 2acf58d897bb093966f18639d087ef904a72a720, not merged. Temporary exact git dependencies to ovg-mobile-engine and ovg-local-storage; no second storage implementation. Root workspace unchanged.

| Task | Candidate implementation | Outstanding |
|---|---|---|
| T005 | apps/desktop/src-tauri Tauri 2 host; desktop build flag selects additive entry in apps/web-next; one Vite source/build pipeline | Lock Tauri/toolchain, compile all target OSes, actual shared workbench integration and launch |
| T006 | Typed desktop capabilities, create/list/open/close/transform/poll/cancel IPC; main local window restriction; bounded in-flight work | Transport end-to-end, payload size before deserialization and malicious renderer tests |
| T007 | Real 073 engine session/workspace, job ownership and cancellation; LocalStore, stale handle rejection; host lifecycle test | Compile/run evidence, panic/close/race/recovery tests |
| T008 | Standalone project selector, native File menu and shortcuts, blank offline MapLibre view; app-private UUID GeoPackage projects | Actual OS keyboard/menu/launch, picker external file grants in W2 |
| T035 | Typed IPC control and separate ovg URI handler; read plane returns only 403/404 until qualified provider; existing Web entry remains default | W0 URL mapping qualification, W2 binary tile provider and end-to-end Web/desktop regression |

Workflow spec074-wave1.yml builds original Web and desktop variant, runs transport tests, resolves graph and tests/builds desktop candidate on Windows/macOS/Linux. Lockfiles exploratory; no successful results asserted. W0 and W1 remain open. No production changes, managed storage, Cloud or installer claimed. Root license unchanged.
