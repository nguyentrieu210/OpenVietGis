# Twenty-seventh implementation slice (GitHub only)
Base 38a363c81ea61c9bcc6b59f66b20dbf2ec612385.
- TABLE-06: replace floating point calculator with bounded exact rational arithmetic, explicit 18-place final rounding, integer exactness, explicit NULL, typed numeric field references. Full-source attribute-only paging with revision/fingerprint, source count and duplicate FID checks, 2000 record cap for atomic edit. Incomplete unselected loaded population blocked.
- TABLE-08: statistics can load all source attributes up to 5000 records; exact decimal sums/means/min/max, distinguish null from empty grouping, null counts, disclose 100-group display cap. Loaded/selected scope labeled separately. No server aggregate or >5000 source support.
- Added exact arithmetic unit tests. GitHub source only, no confirmed build/CI/browser result. No status promotion, merge or deploy.
