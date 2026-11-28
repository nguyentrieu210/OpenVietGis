# SPEC 072 — visual repeatability evidence

Candidate `f1099af78ffd001f9cc15caa4b26df52b0bf8afa`; production CSS last changed at `0a990deb05e79c1161bb4e8f48d6d33c0115769a`, build `cfc9c272b05ab2b696084637`.

- Processing Toolbox, Processing Modeler and ACL chrome migrated to shared semantic tokens. Map imagery, print content and workflow geometry are not recolored by this migration.
- TypeScript, Vite production build and offline shell: PASS.
- Vitest 316/316, Node 53/53, all five canonical browser scripts, 49 PASS log lines: PASS.
- Export: 6 widths (360,390,640,768,1280,1920) x 2 themes = 12 captures.
- Plugin Manager: 6 widths checked for layout, 2 theme captures at desktop width.
- Modeler: 390 and 1280 x 2 themes = 4 captures, background and bounds checked.
- All 18 corresponding images from independent browser executions compare successfully: changed pixel ratio 0 at channel threshold 8; acceptance tolerance 0.001.
- Raw images and repeatability log are retained in isolated development checkout under `/var/tmp/ovg072-full.rKGDyY/visual-072`, `visual-072-repeat`, `visual-diff.jsonl`. Not a permanent published image baseline.

Reproduction: create a screenshot directory, run `qgis-parity-keyboard-smoke.mjs` with `OVG_UI_SCREENSHOTS` set, once with `OVG_EXPORT_ONLY=1`, once with `OVG_PLUGIN071_ONLY=1 OVG_DIALOG072_ONLY=1`, and once with neither mode. Repeat into a different directory. Run `python3 scripts/compare-ui-screenshots.py BASELINE CANDIDATE` (Pillow required). Missing/extra images and size mismatches fail. Compare only on equivalent browser/font/device conditions.

Limitations: synthetic API, Node 24 rather than repository Node 26, reused installed dependencies. Repeatability is not human visual approval. No approved reference baseline, all-legacy-dialog coverage, ACL end-to-end authorization verification, fresh frozen install, CI integration or dev/live deployment. E12/E13 remain open.
