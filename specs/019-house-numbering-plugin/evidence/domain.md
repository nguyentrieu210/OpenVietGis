# Pure domain implementation evidence — 2026-11-21
- Branch: spec/019-house-numbering-plugin, main baseline c5cc5a9.
- Implementation: modules/house_numbering/domain.py; tests/modules/test_house_numbering_domain.py.
- First red test: ModuleNotFoundError for absent domain. Second red test: ImportError for additional absent functions. Green: 17 unit test methods passed (with subcases) on VPS Python 3.13.5.
- Commands:
  - `python3 -m unittest discover -s tests/modules -p 'test_house_numbering_domain.py' -v` — Ran 17 tests, OK.
  - `/opt/openvietgis/apps/api/.venv/bin/ruff check modules/house_numbering/domain.py tests/modules/test_house_numbering_domain.py` — all checks passed.
- Proven local rules: directional parity, empty/start/end/inside gap, reserved number occupancy, suffix alphabet/dash, nested alley prefix, existing number preservation, distance/ambiguity/malformed rejection, nonmonotonic sequence refusal, pure shift and collision, seven category classes, reviewer/authority/eligibility policy, projected route location with ambiguity, old-before-new exact scoped certificate match, accepted deduplicated progress, import accounting, flat standard-field whitelist, verified-template issuance gate.
- Limits: all inputs are synthetic or caller-provided scoped values. There is no runtime endpoint, storage, browser, actual CRS transform, authentication, RLS, document retrieval, privacy scan, legal verification or module activation. This does not pass SC-001–011 in full. Raw string authority is an input hint only; integration must verify authority and authorization. No PII or real source file imported.
