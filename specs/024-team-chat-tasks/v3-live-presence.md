# V3: Live map collaboration

- Opt-in per session: WebSocket shares project presence, cursor (not GPS) and viewport. Participants can follow another viewport; moving the local map stops follow.
- Server derives identity from authenticated human sessions; validates exact web origin, project access and coordinate ranges; checks grants/session every five seconds. No geometry modifications through WebSocket.
- Ephemeral in-memory rooms: bounded to 64 connections/process, 40/project, per-peer single latest snapshot queue and ~12 updates/s. Single API worker required for global presence; multi-worker deployments must first introduce a shared broker. No presence data persisted.
- Durable changesets continue to use revision validation. Postgres emits scoped SSE spatial_change after committed changeset insert/status change. Core studio preserves current viewport and defers refresh during editing; plugins require explicit reload after saving.
- Same-origin reverse proxy must support WebSocket upgrade and long-lived SSE. No release until migrations converge with concurrent branches and deployment smoke passes.

## Verification

- Backend collaboration suite: 5 tests passed (presence validation, authenticated two-peer presence, cross-origin/unauthorized rejection, existing chat/tasks and SSE).
- TypeScript/Vite production build succeeded.
- Browser smoke reached new live-cursor, follow, departure and spatial refresh assertions successfully; full regression initially timed out on subsequent account reload under concurrent VPS build load.
- Spatial changeset SSE integration passed: a real committed GIS edit emitted spatial_change on a live authenticated event stream. Release validation still requires complete browser regression.
