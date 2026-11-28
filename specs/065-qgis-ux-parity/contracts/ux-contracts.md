# UX contracts & acceptance evidence

## Command contract (conceptual TypeScript)
```ts
interface WorkspaceCommand {
  id: string;
  category: string;
  labels: { vi: string; en: string };
  aliases: readonly string[];
  modes: readonly ('gis'|'cad'|'3d')[];
  capability: string;
  canExecute(context: AuthorizedScope): { enabled: boolean; reason?: string };
  execute(context: AuthorizedScope, args: unknown): Promise<CommandReceipt>;
}
```
Use existing CoreCommand/WorkbenchRegistry contracts; this is target behavior, not a replacement ABI. All entry points resolve same command and permission/revision check. Unknown command disabled; no unsafe raw SQL, eval or generic CAD JS.

## Preference contract
Keys versioned with account, org, project, engine. Shared map configuration is saved by explicit versioned API. Migration reads legacy preference once, copies allowed fields after account scope validation, clamps values, retains safe fallback. No tokens or private source payload in preferences. F5 does not auto-execute mutations or resume prior AI consent.

## Server contracts
Use current API paths after inventory; document exact endpoints, request/response, authorization, revision, pagination semantics, error mapping and idempotency in per-wave design. If missing, add reviewed Rust endpoints with backward compatibility; no invented existing routes. Full-source filter and sort execute against authorized source, not current visible page.

## UI state contract
Every panel has loading/empty/ready/disabled/error and recovery where applicable. Menus, modal, toolbar expose keyboard focus and aria names; no keyboard interception in text fields/IME. Project/mode switch cancels stale requests, retains allowed state and rejects stale receipts. Separate personal view changes from shared map dirty state.

## Evidence record
Each matrix row records QGIS chapter + version, OVG commit, fixture/checksum, exact steps, expected outcomes, actual result, unit/API/E2E test IDs, paired screenshots, metrics, reviewer and blocker. No status V based solely on source presence. Record expected vs actual rendered geometry and exported artifacts when correctness matters.
