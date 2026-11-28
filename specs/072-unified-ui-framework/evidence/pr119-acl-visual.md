# ACL and repeatable visual suite
Candidate `99183ce2b5c154cd9d72022ce844a750211e528f` (production UI `e0283f32395639b89ae2a3585219f932ae61e792`, Vite/offline build `1147d992baf48b0c966d8129`).
- Shared ACL form and tab primitives, scroll regions, semantic team colors.
- Synthetic browser verifies scoped batch grant, cancel without mutation, required read permission, deduplicated feature IDs, exact grant payload and batch revoke.
- ACL responsive bounds verified at 390/1280 in light/dark.
- Two independent runs of the single visual runner captured 22/22 expected images each. All corresponding images had changed pixel ratio 0 at channel threshold 8.
- Local captures: `/var/tmp/ovg072-full.rKGDyY/visual-acl-a.vjr2mb` and `visual-acl-b.9amgDy`.
- Typecheck/build/offline shell PASS; default keyboard smoke also exercised by both visual runs.
- Dedicated CI workflow uses Node 26 and frozen dependencies, captures and uploads evidence without overwriting a reference baseline.
Limitations: local Node 24, synthetic API only, backend authorization not qualified, no human-approved visual reference or complete legacy migration. CI workflow execution unverified. No acceptance checkbox closed, merge or deployment.
