# Plan
1. Freeze and fetch main; start from Spec 031 branch.
2. Compare active deployment references and unreleased branch ancestry.
3. Consolidate selected QGIS, spatial workspace and MCP commits in an isolated candidate.
4. Validate offline frozen dependency install, web-next build, focused Python tests, MCP tests, migration graph and QGIS package.
5. Add repeatable read-only refs snapshot and candidate manifest/verification command.
6. Review and commit local candidate. Keep all other refs and live deployments intact.
7. Follow-up: exact-candidate CI, real browser + PostGIS/OIDC, Windows QGIS, migration/recovery rehearsal, independent review and owner-approved rollout.
