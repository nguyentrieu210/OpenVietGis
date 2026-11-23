# R011-2 V2 — Real-time collaboration
- Replace client polling with Server-Sent Events. PostgreSQL NOTIFY is delivered on commit; API revalidates authenticated session and project scope before each event or heartbeat; no message body, task fields or private conversation metadata sent to unrelated participants. Reconnect causes authoritative refresh. Bounded per-process connection count; no polling fallback.
- Project-scoped private chat (only eligible full-scope editors can send; SELECT RLS limits private messages to participants even for owners/admins). Explicit peer required for private history; project history excludes DMs.
- Assignment inbox with persistent per-recipient read receipt; only assigned member can mark received tasks read; unread count and message/task changes refresh through SSE.
- Kanban with three columns and drag/drop status mutation, retained revision checks.
- Convert public project messages to tasks with explicit confirmation/source link; private message text is never copied into project tasks by this action.
- Release gate: real DB isolation/commit notification, browser flow and build; further production test through proxy and merge migration ancestry before deployment.
