# Plan — R026 / Slice 1

1. Use verified main 5baade9 in an isolated worktree.
2. Add pure domain manifest/catalog and permission gate, reusing R002 Permission
   and existing AuthorizationService. No arbitrary SQL or vendor SDK.
3. Add authenticated, project-scoped read endpoint with prepared RLS scope.
4. Unit tests cover provider inventory, read/write gates and disabled claims.
5. Validate python lint/types/tests and API OpenAPI generation; review production
   authorization boundaries before promotion.
6. No production deployment before green release gates and a tested rollback.
