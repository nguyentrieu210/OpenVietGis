# Spec 064 finalize rollback/restore rehearsal — 2026-11-26

The `064-finalize` worktree ran the fail-closed backup/restore and post-write rollback rehearsal against the isolated `dev-postgres-test-1` database. No production database or service was changed.

## Result

- Source database size: 293 MB (307,623,615 bytes).
- Dump: 15,601,729 bytes; SHA-256 `3f9ab500429b8eda932ad7428866be0afea3b032e9e9a600827508a294b0c97f`.
- Source/restored schema SHA-256: `eb6acecdbc525f12ccd933782eef80a2f8f0a1582b1ba7f5d5ed35031d86cc34` — exact match.
- Source/restored row-count SHA-256: `055d33b48bde8be71a995fae9130b3e226a435ec3295b94a9fafd115fcb65449` — exact match.
- Source/restored content SHA-256: `9b319f3fb9aeb5b5dd9ab8a61f58a86b8119b87e10d050340f883c012cab8909` — exact match.
- Dump/restore/verification: 4 s / 40 s / 42 s; total restore verification 122 s.
- Synthetic acknowledged write survived rollback verification with identical SHA-256 `f71cac6360ba310394885173745a3e2cddba1230eb53f5acfd97ad3ba98d537a`.
- Cross-tenant runtime read after rollback: 0 rows.
- Alembic marker was unchanged by the rehearsal (`0019_extension_lifecycle`).
- Overall synthetic rollback rehearsal: **PASS** in 125 s.

The rehearsal exposed an operations bug: `rollback-rehearsal.sh` always delegated backups to the default `/tmp` tmpfs, so its capacity guard rejected a valid 293 MB source even though `/opt` had sufficient disk. The script now accepts `--backup-dir` and forwards it to both dry-run and real backup/restore calls; the capacity guard itself remains unchanged.

## Limitation

This proves database backup/restore parity, additive schema safety, acknowledged-write persistence and tenant RLS on an isolated database. It still reports `application_rollback_verified=no`: booting the exact baseline application binary against the post-candidate database is a separate release gate and must not be inferred from this rehearsal.
