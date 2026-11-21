# UI Contract: Session Shell State

## Purpose

Prevent authentication flicker while retaining strict protected-data freshness.

## State transitions

| Current | Event | Next | UI behavior |
| --- | --- | --- | --- |
| unknown | `/me` succeeds active | authenticated | Mount authenticated shell |
| unknown | `/me` returns 401 | signed_out | Public/login path |
| unknown | network/server unavailable | unavailable | Explicit retry/error state |
| authenticated | background `/me` starts | checking | Keep shell chrome mounted |
| checking | `/me` succeeds active | authenticated | Refresh safe account summary |
| checking | `/me` returns 401 | signed_out | Clear protected caches, go login |
| checking | transient network failure | authenticated/unavailable indicator | Do not infer new authorization; protected resources remain governed by their own current reads |
| any authenticated state | logout confirmed | signed_out | Clear query caches and navigate public |
| any authenticated state | protected request confirms auth loss | signed_out | Clear protected caches and transition login |

## Security invariants

- Shell continuity is not authority to display stale tenant resources.
- Existing protected organization/project/layer queries keep their strict current-data policy.
- UI-hidden actions are not authorization controls.
- No provider token is exposed to application state or browser storage.
