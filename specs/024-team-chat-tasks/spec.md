# R011-2 — Project Chat & Tasks
Parent: [R011](../../ROADMAP.md#r011). Dependencies: R002 project permissions, R011 collaboration schema and audit, GIS access restrictions from the pinned integrated candidate.

## Requirements
- Project chat: authenticated full-scope human members read; editors post plain text. Latest 100 messages, manual refresh and 5-second polling while panel open. Messages are append-only and attributed.
- Project tasks: editors create tasks and assign eligible active editors; all full-scope readers can view project tasks, filter My tasks, and see due date, priority, status and creator.
- Creator or assignee can change status using expected revision; audit every creation/status change. No destructive deletion in V1.
- Optionally attach an explicitly confirmed WGS84 map position to a task; navigate to that position without changing spatial data.
- Project/organization isolation at API and FORCE RLS; restricted GIS members and service principals denied. CSRF for writes. No DMs, file uploads, push notifications or unrestricted map feature sharing in this slice.
- Actual public launch requires integration with concurrent CAD migration and release gates.

## Acceptance
Two editors see messages and tasks in the same project, another tenant and a scoped user cannot; invalid assignee and stale status change rejected; chat text rendered as text; map pin navigates; refresh survives page reload.
