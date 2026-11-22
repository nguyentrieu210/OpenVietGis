# R011-2 / V5: feature discussion and assignments

- Comments are immutable project messages keyed by source and feature ID, separate from general project chat. Full-project users can read; unrestricted editors can post.
- Mention picker selects actual active project editors by user ID; server validates and stores name snapshots, personal inbox receipts and unread count. No text parsing or arbitrary addresses.
- Existing task form accepts a selected feature or converts its comment to an assigned task. Server derives location in WGS84 and persists feature ID and snapshot coordinates; comment-linked task retains the comment and spatial anchor even if the feature is later deleted.
- Latest 200 feature comment pins are projected from WGS84 snapshot coordinates, with comment count and click-to-open thread; can hide pins in comments tab.
- SSE project invalidation refreshes comments, tasks and personal notifications; receipt changes target only the recipient. RLS tenant/scope restrictions remain.
- No external messages or emails. No feature edits or auto-locks are performed by commenting. Deployment requires migration convergence and release gates.

## Evidence

- Real PostgreSQL integration validated project isolation, mentions and receipts, comment-to-task conversion, direct feature assignment, existing message isolation, historical comments after deletion, pin count and scoped-user denial. Existing lease and realtime tests passed; final real SSE network test verified committed feature comments broadcast.
- TypeScript/Vite production build succeeded. Previous collaboration browser smoke passed; final extended smoke covers selected feature discussion, mention picker, pin reopening and linked task/notification flow. The collaboration panel uses a body portal to avoid inspector stacking conflicts.
