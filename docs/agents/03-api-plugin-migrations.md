# Agent 03 — Backend contracts, DB migration and plugin boundary

## Scope and ownership
Own: documentation report docs/qa/agent-03-backend.md only.
Read: apps/api, modules, packages/plugin-sdk, migration scripts, tests.
No DB writes, schema upgrades, production API writes or credentials.

## Task
1. Map API ownership and plugin registration/activation for modules/house_numbering.
2. Trace how core domains avoid dependence on business plugins.
3. Review the migration graph statically and flag potential merge/multiple-head concerns.
4. Identify tenant/auth isolation checks, user/project sharing risks, and tests already present.
5. Classify issues VERIFIED_FROM_CODE vs NEEDS_RUNTIME_TEST.

## Deliverable
Produce docs/qa/agent-03-backend.md with module contract map, migration audit and priority tests.
