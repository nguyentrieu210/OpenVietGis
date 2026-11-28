# Review resolution — Revision 2
Addresses PR #103 review 6099075983 at e01975890d7479afea2786088bc274e19460f69b. Design changes only; implementation and empirical closure remain open.
| Finding | Design resolution | Verification remaining |
|---|---|---|
| P0 auth | mobile-auth-contract | route inventory and identity/security tests |
| P0 manifest | plugin-runtime-contract | v1/v2 compatibility, signed asset tests |
| P0 storage | offline-sync-contract | encryption, key and crash proof |
| P0 sync | offline-sync-contract | actual server revision/receipt mapping and fault injection |
| P0 portable/build | portable-kernel-contract, native-build-contract | dependency graph, native build/16KB device |
| P1 release profiles | acceptance-matrix | physical device gates |
| P1 format/CRS | format-capability-matrix | independent fixtures and tolerances |
| P1 resources | mobile-render-contract | frozen profiles, real measurements |
| P1 render pipeline | mobile-render-contract | native adapter/FFI copy proof |
| P1 tasks | execution task matrix | exact owner path/fixtures on W0 freeze |
| P1 commerce | plugin/native-build/acceptance contracts | policy/legal and entitlement tests |
Revision 2 contracts supersede ambiguous v1 wording. Proposed budgets and interfaces are not evidence of implementation. No production changes or scope waiver.
