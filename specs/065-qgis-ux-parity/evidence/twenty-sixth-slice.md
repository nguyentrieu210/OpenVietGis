# Twenty-sixth implementation slice (GitHub only)
Base 7c821046f6e52f4be617538003b6e3e1083db9b9.
- DOCK-03: source browser now uses actual server cursor pagination (100/page, max 1000 active rows), verifies org/project and item scope, rejects duplicate source IDs and repeated cursors, suppresses stale responses on panel close/access/scope change, reloads on reopening; read-only per-source metadata and linked layer navigation. Retired records are excluded from display without dropping cursor. No attach/drop of an unlinked source, no provider-specific error acceptance.
- DOCK-08: capture default view when no override exists so theme reapplication can restore defaults, while preserving current edit locks and saved style library. Unit regression added.
- Source and test changes written directly through GitHub. No build, test or browser run claimed for this commit. Matrix status unchanged; no merge/deploy.
