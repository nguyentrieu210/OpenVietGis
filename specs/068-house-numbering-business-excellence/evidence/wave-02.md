# Wave 02 — signoff integrity and permission boundaries

- Core guard migration preserves historical signoffs and locks both old and new feature scopes; verified concurrent signoff wins against a waiting writer. Existing disable/re-enable semantics retained intentionally for backward compatibility, not an administrative issuance policy.
- Assign/numbering, survey, review/approval, private records and sensitive exports now check the named module permission with Core project-role authorization. Private upsert requires manage plus private_read; delegated survey recording also requires manage. Authoritative rule creation requires a nonblank authority reference; manager role remains mandatory. API compile passed. Need full HTTP permission and proposal acceptance on isolated full schema and UI controls.
- No Bình Hưng restore, legal rule validation, production issuance or live database migration in this wave. E02/E10/E11 advanced but not complete.
