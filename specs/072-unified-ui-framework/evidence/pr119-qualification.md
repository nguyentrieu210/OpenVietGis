# PR 119 — local qualification evidence

Candidate: `9dcd992c117957fea1c30323e1246e79f64698a5`. Production source last changed at `95b2eb6cbbdc6f60674b56fa3f2fa6e4bc11ff6b`; subsequent commits change test runners only.

Environment: isolated development VPS checkout, preinstalled dependencies, Node 24.22.2, system Chromium, synthetic loopback API fixtures. This is not the required Node 26 frozen-install CI, paired QGIS, or dev/live deployment evidence.

| Gate | Local evidence | Remaining |
| --- | --- | --- |
| E02 semantic tokens | Shared runtime theme and export portal palette exercised | Audit remaining legacy tokens |
| E03 primitives | 4 UI unit tests and integrated web tests | Remaining first-party consumers |
| E04 dialogs | Export five repeated focus cycles, performance dialog true keyboard boundaries, responsive plugin manager | All legacy dialogs and themes |
| E05 docks | Plugin and Layer Styling docking/resizing | Full regression |
| E06 responsive | Export 12 theme/width cases and plugin manager six widths on prior candidate; shell resize on current candidate | Repeat full matrix on final artifact |
| E07 menus | Exact command search, duplicate registration regression, keyboard ribbon | All menu variants |
| E08 forms | Attribute edit, read-only form, retry and validation | All legacy forms |
| E09 tables | Typed sorting, selection, width preferences, attribute-only query | Full virtual-scroll evidence |
| E10 theme | Dark portal and plugin manager on prior candidate | Complete dark/light legacy coverage |
| E11 plugin UI | Explicit lazy plugin opening, CAD/3D switching and permission revocation | First-party contract review |
| E12 legacy migration | Shared controls and CSS in modified panels | Remaining CSS inventory and behavior-equivalent removal |
| E13 visual regression | Layout assertions and browser smoke | Reviewed screenshot baselines across all required surfaces |

Current results: web TypeScript and Vite production build + offline shell PASS (production build ID `23b7250570958a918dda4f81`); Vitest 316/316 and Node 53/53 PASS on `0d64c6f` (source unchanged); canonical browser runner all five scripts PASS, 48 PASS log lines on `9dcd992c117957fea1c30323e1246e79f64698a5`. Earlier isolated runs included export and plugin responsive tests. These are synthetic tests; no acceptance checkbox is closed by this record. No merge or deployment.
