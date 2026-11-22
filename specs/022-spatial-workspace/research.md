# Research
- Decision: keep MapLibre; repository ADR-006 and web-next LiveApp import confirm it. Reject assumptions of OpenLayers and replacing the 2D map.
- Decision: use one shared mode session and scoped async load request. React host retains camera and selection; do not remount GIS unnecessarily.
- Decision: separate real CAD and geographic 3D adapters. Candidate CesiumJS for rich geographic 3D after version/license/asset validation. CAD bridge verifies the deployed native engine and uses explicit resource authorization. Reject hand-built renderer or untrusted iframe/token forwarding.
- Decision: no GIS/CAD conversion as part of mode change; georeferencing requires verified CRS, units, control points and vertical reference. Follow R003/R005/R012.
- Constraint: shared main is dirty with active spec 020 and app web-next wholly untracked, so use source-verified main commit e0db0e2 and separate worktree. Actual host wiring awaits reviewed 020 baseline.
