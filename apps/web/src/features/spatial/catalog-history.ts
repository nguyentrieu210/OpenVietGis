/** History pages are current-authority reads, not a retained local audit log. */
export interface HistoryScope {
  accountId: string;
  organizationId: string;
  projectId: string;
}

export const HISTORY_PAGE_SIZE = 20;

export function historyPrefix(scope: HistoryScope) {
  return ["spatial-catalog", scope.accountId, scope.organizationId, scope.projectId, "changes"] as const;
}

export function historyKey(scope: HistoryScope, cursor: string | null) {
  return [...historyPrefix(scope), HISTORY_PAGE_SIZE, cursor] as const;
}

interface HistoryProject {
  id: string;
  organization_id: string;
  status: string;
}

/** A settled, current matching Project is a prerequisite, not a substitute for history authorization. */
export function canReadHistory(scope: HistoryScope, project: HistoryProject | undefined): boolean {
  return Boolean(project && project.id === scope.projectId && project.organization_id === scope.organizationId &&
    (project.status === "active" || project.status === "archived"));
}

interface ScopedChange {
  id: string;
  organization_id: string;
  project_id: string;
}

interface HistoryPageScope {
  organization_id: string;
  project_id: string;
  items: readonly ScopedChange[];
  next_after_id: string | null;
}

/** Fail closed on any mismatched row or cursor, even if the query itself succeeded. */
export function historyResponseMatchesScope(
  scope: HistoryScope, page: HistoryPageScope | null | undefined, cursor: string | null
): boolean {
  if (!page || page.organization_id !== scope.organizationId || page.project_id !== scope.projectId ||
      !Array.isArray(page.items) || page.items.length > HISTORY_PAGE_SIZE) return false;
  let prior = cursor;
  for (const item of page.items) {
    if (!item || item.organization_id !== scope.organizationId || item.project_id !== scope.projectId ||
        typeof item.id !== "string" || !item.id || (prior !== null && item.id <= prior)) return false;
    prior = item.id;
  }
  return page.next_after_id === null ||
    (page.items.length === HISTORY_PAGE_SIZE && page.next_after_id === prior);
}

export interface HistoryTraversal {
  cursor: string | null;
  history: (string | null)[];
}

export function createHistoryTraversal(): HistoryTraversal {
  return { cursor: null, history: [] };
}

export function nextHistoryPage(state: HistoryTraversal, next: string | null): HistoryTraversal {
  if (!next || (state.cursor !== null && next <= state.cursor)) return state;
  return { cursor: next, history: [...state.history, state.cursor] };
}

export function previousHistoryPage(state: HistoryTraversal): HistoryTraversal {
  if (!state.history.length) return state;
  return { cursor: state.history[state.history.length - 1] ?? null, history: state.history.slice(0, -1) };
}

/** JSON scalar formatting only: preserve string precision, null, empty string, zero and false. */
export function formatHistoryValue(value: unknown): string {
  if (value === null || typeof value === "string" || typeof value === "boolean" ||
      (typeof value === "number" && Number.isFinite(value))) return JSON.stringify(value);
  return "Unavailable";
}

function positiveRevision(value: unknown): value is number {
  return typeof value === "number" && Number.isSafeInteger(value) && value > 0;
}

/** Never stringify an entire revision or unknown payload; show only typed revision domains. */
export function revisionEntries(resourceType: string, revision: unknown): [string, number | string][] {
  if (typeof revision !== "object" || revision === null) return [];
  if (resourceType === "layer" && "layer_revision" in revision && positiveRevision(revision.layer_revision)) {
    return [["Phiên bản lớp", revision.layer_revision]];
  }
  if (resourceType === "data_source" && "catalog_revision" in revision &&
      positiveRevision(revision.catalog_revision) && "content_revision" in revision &&
      typeof revision.content_revision === "string" && "schema_revision" in revision &&
      typeof revision.schema_revision === "string") {
    return [
      ["Phiên bản danh mục", revision.catalog_revision],
      ["Phiên bản nội dung", revision.content_revision],
      ["Phiên bản lược đồ", revision.schema_revision]
    ];
  }
  return [];
}
