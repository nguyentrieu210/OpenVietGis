import type { OfflineOperationReceipt, OfflinePackage } from "@openvietgis/client-sdk";

export type Value = string | number | boolean | null;
export type Field = { field_id: string; name: string; type: "text" | "integer" | "decimal" | "boolean" | "timestamp"; nullable: boolean };
export type Point = [number, number];
export type Geometry = { type: "Point"; coordinates: Point } | { type: "LineString"; coordinates: Point[] } | { type: "Polygon"; coordinates: Point[][] };
export type Bounds = { min_x: number; min_y: number; max_x: number; max_y: number };
export type Feature = { feature_id: string; properties: Record<string, Value>; geometry: Geometry };
export type Snapshot = {
  version: 1; package_id: string; actor_id: string; organization_id: string; project_id: string;
  layer_id: string; layer_name: string; source_id: string; content_revision: string;
  schema_revision: string; crs_code: number; geometry_family: Geometry["type"]; aoi_bbox: Bounds;
  created_at: string; expires_at: string; fields: Field[]; features: Feature[];
};
export type Draft = {
  op_type: "create" | "update"; feature_id: string; properties: Record<string, Value>;
  client_operation_id?: string; // legacy attempted batches may omit this
  geometry?: { type: "Point"; coordinates: Point };
  capture?: { source: "manual" | "device"; time: string; accuracy: number | null };
};
export type Receipt = { id: string; status: string; applied_changeset_ids: string[]; conflicts: unknown[]; operation_receipts?: OfflineOperationReceipt[] };
export type PackageRecord = {
  id: string; actorId: string; organizationId: string; projectId: string; name: string;
  snapshotBytes: ArrayBuffer; snapshotSha: string; gpkgBytes: ArrayBuffer; gpkgSha: string;
  expiresAt: string; revision: number; locked: boolean; drafts: Draft[];
  batch: { id: string; attempted: boolean; receipt?: Receipt } | null;
};

const ACTOR_KEY = "ovg-offline-actor-v1";
export function activeOfflineActor(): string | null {
  try { return localStorage.getItem(ACTOR_KEY); } catch { return null; }
}
export function lockOfflineActor(): void {
  try { localStorage.removeItem(ACTOR_KEY); } catch { /* unsupported storage stays locked */ }
}
export function observeOnlineActor(actorId: string | null): void {
  if (activeOfflineActor() !== actorId) lockOfflineActor();
}
export function unlockOfflineActor(actorId: string): void {
  localStorage.setItem(ACTOR_KEY, actorId);
  if (activeOfflineActor() !== actorId) throw new Error("Không thể lưu danh tính ngoại tuyến.");
}

export async function sha256(bytes: ArrayBuffer): Promise<string> {
  const hash = await crypto.subtle.digest("SHA-256", bytes);
  return [...new Uint8Array(hash)].map(byte => byte.toString(16).padStart(2, "0")).join("");
}

function decode(bytes: ArrayBuffer): Snapshot {
  const data = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes)) as Snapshot;
  if (data.version !== 1 || !Array.isArray(data.fields) || !Array.isArray(data.features) ||
      data.features.length > 5000 || !Number.isFinite(data.crs_code) ||
      !["Point", "LineString", "Polygon"].includes(data.geometry_family) ||
      !Number.isFinite(Date.parse(data.expires_at))) throw new Error("Gói không được hỗ trợ.");
  return data;
}

export async function verifyRecord(record: PackageRecord): Promise<Snapshot> {
  if (record.locked || Date.parse(record.expiresAt) <= Date.now()) throw new Error("Gói bị khóa hoặc hết hạn.");
  if (new TextDecoder().decode(record.gpkgBytes.slice(0,16)) !== "SQLite format 3\0") throw new Error("Gói dữ liệu hỏng hoặc không đúng GeoPackage.");
  if (await sha256(record.gpkgBytes) !== record.gpkgSha || await sha256(record.snapshotBytes) !== record.snapshotSha) {
    throw new Error("Gói dữ liệu hỏng hoặc bị sửa đổi.");
  }
  const snapshot = decode(record.snapshotBytes);
  if (snapshot.package_id !== record.id || snapshot.actor_id !== record.actorId ||
      snapshot.organization_id !== record.organizationId || snapshot.project_id !== record.projectId ||
      snapshot.expires_at !== record.expiresAt) throw new Error("Gói không khớp danh tính/phạm vi.");
  return snapshot;
}

export async function makeRecord(
  pkg: OfflinePackage, snapshotBytes: ArrayBuffer, gpkgBytes: ArrayBuffer,
  actorId: string, org: string, project: string
): Promise<PackageRecord> {
  if (snapshotBytes.byteLength > 25 * 1024 * 1024 || gpkgBytes.byteLength > 75 * 1024 * 1024) {
    throw new Error("Gói vượt giới hạn lưu trữ thiết bị.");
  }
  const snapshotSha = pkg.layer_pins[0]?.snapshot_sha256;
  if (typeof snapshotSha !== "string" || gpkgBytes.byteLength !== pkg.byte_size || pkg.revoked_at) {
    throw new Error("Gói không hợp lệ.");
  }
  const record: PackageRecord = {
    id: pkg.id, actorId, organizationId: org, projectId: project, name: pkg.name,
    snapshotBytes, snapshotSha, gpkgBytes, gpkgSha: pkg.sha256, expiresAt: pkg.expires_at,
    revision: 0, locked: false, drafts: [], batch: null
  };
  const s = await verifyRecord(record);
  const pin = pkg.layer_pins[0];
  if (!pin || s.features.length !== pkg.feature_count || s.crs_code !== pkg.crs_code ||
      s.layer_id !== pin.layer_id || s.source_id !== pin.source_id ||
      s.content_revision !== pin.content_revision || s.schema_revision !== pin.schema_revision) {
    throw new Error("Gói không khớp bản kê khai máy chủ.");
  }
  return record;
}

function openStore(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open("ovg-offline-v1", 1);
    request.onupgradeneeded = () => request.result.createObjectStore("packages", { keyPath: "id" });
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
    request.onblocked = () => reject(new Error("Kho ngoại tuyến đang bị khóa."));
  });
}
async function readAll(): Promise<PackageRecord[]> {
  const db = await openStore();
  try {
    return await new Promise((resolve, reject) => {
      const req = db.transaction("packages", "readonly").objectStore("packages").getAll();
      req.onsuccess = () => resolve(req.result as PackageRecord[]);
      req.onerror = () => reject(req.error);
    });
  } finally { db.close(); }
}

export async function listLocalPackages(actor: string, org: string, project: string): Promise<PackageRecord[]> {
  return (await readAll()).filter(r => r.actorId === actor && r.organizationId === org && r.projectId === project);
}

// A known authorization denial must survive a later device unlock for another
// project. Increment revisions in the same transaction so stale tabs cannot
// overwrite the lock while retaining their own draft copy.
export async function lockLocalScope(actor: string, org: string, project: string): Promise<void> {
  const db = await openStore();
  try {
    await new Promise<void>((resolve, reject) => {
      const tx = db.transaction("packages", "readwrite", { durability: "strict" });
      const cursor = tx.objectStore("packages").openCursor();
      cursor.onsuccess = () => {
        const row = cursor.result;
        if (!row) return;
        const r = row.value as PackageRecord;
        if (!r.locked && r.actorId === actor && r.organizationId === org && r.projectId === project) {
          row.update({...r, locked:true, revision:r.revision+1});
        }
        row.continue();
      };
      tx.oncomplete = () => resolve();
      tx.onerror = () => reject(tx.error ?? new Error("Không thể lưu khóa gói."));
      tx.onabort = () => reject(tx.error ?? new Error("Không thể lưu khóa gói."));
    });
  } finally { db.close(); }
}

// Compare local revision inside the same IDB transaction to avoid overwriting
// another tab's drafts. Resolve only after the transaction commits durably.
async function writeRecord(record: PackageRecord, expected: number | null, remove = false): Promise<PackageRecord> {
  const db = await openStore();
  const next = { ...record, revision: (expected ?? -1) + 1 };
  try {
    return await new Promise((resolve, reject) => {
      const tx = db.transaction("packages", "readwrite", { durability: "strict" });
      const store = tx.objectStore("packages");
      const req = store.get(record.id);
      let problem: Error | undefined;
      req.onsuccess = () => {
        const current = req.result as PackageRecord | undefined;
        if ((current?.revision ?? null) !== expected ||
            (current && (current.actorId !== record.actorId || current.projectId !== record.projectId ||
              current.organizationId !== record.organizationId))) {
          problem = new Error("Gói đã thay đổi ở cửa sổ khác. Hãy tải lại.");
          tx.abort();
          return;
        }
        if (remove) store.delete(record.id);
        else store.put(next);
      };
      tx.oncomplete = () => resolve(next);
      tx.onerror = () => reject(problem ?? tx.error ?? new Error("Lỗi lưu ngoại tuyến."));
      tx.onabort = () => reject(problem ?? tx.error ?? new Error("Lưu bị hủy."));
    });
  } finally { db.close(); }
}

export function installRecord(record: PackageRecord): Promise<PackageRecord> { return writeRecord(record, null); }
export function saveRecord(record: PackageRecord): Promise<PackageRecord> { return writeRecord(record, record.revision); }
export function removeRecord(record: PackageRecord): Promise<PackageRecord> { return writeRecord(record, record.revision, true); }

export function normalizeField(field: Field, input: string, isNull: boolean): Value {
  if (isNull) {
    if (!field.nullable) throw new Error(field.name + " không thể để trống.");
    return null;
  }
  if (field.type === "text") {
    if (input.length > 4096) throw new Error(field.name + " quá dài.");
    return input;
  }
  if (field.type === "integer") {
    if (!/^[+-]?\d+$/.test(input) || !Number.isSafeInteger(Number(input)) || Number(input) < -2147483648 || Number(input) > 2147483647) throw new Error(field.name + " phải là số nguyên 32-bit.");
    return Number(input);
  }
  if (field.type === "decimal") {
    if (!/^-?\d+(\.\d+)?$/.test(input)) throw new Error(field.name + " phải là số thập phân.");
    const [whole = "", fraction = ""] = input.replace(/^-/, "").split(".");
    const i = whole.replace(/^0+(?=\d)/, "");
    const f = fraction.replace(/0+$/, "");
    if (i.length + f.length > 38 || f.length > 18) throw new Error(field.name + " vượt độ chính xác.");
    return (input.startsWith("-") && (i !== "0" || f) ? "-" : "") + i + (f ? "." + f : "");
  }
  if (field.type === "boolean") {
    if (input !== "true" && input !== "false") throw new Error(field.name + " phải là true hoặc false.");
    return input === "true";
  }
  if (!/^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d(?:\.\d+)?Z$/.test(input) || !Number.isFinite(Date.parse(input))) {
    throw new Error(field.name + " phải là ISO-8601 UTC (Z).");
  }
  const [prefix = "", fraction = ""] = input.slice(0, -1).split(".");
  if (fraction.length > 6) throw new Error(field.name + " vượt độ chính xác timestamp.");
  return prefix + (Number(fraction) ? "." + fraction.padEnd(6,"0") : "") + "Z";
}

export function queueDraft(record: PackageRecord, draft: Draft): PackageRecord {
  if (record.batch?.attempted) throw new Error("Nhóm đã thử gửi; không thể thay đổi nội dung.");
  const previous = record.drafts.find(d => d.feature_id === draft.feature_id);
  const identified = {...draft, client_operation_id: previous?.client_operation_id ?? crypto.randomUUID()};
  const drafts = [...record.drafts.filter(d => d.feature_id !== draft.feature_id), identified];
  if (drafts.length > 2000) throw new Error("Tối đa 2000 thay đổi/nhóm.");
  return { ...record, drafts, batch: record.batch ?? { id: crypto.randomUUID(), attempted: false } };
}
export function wireDrafts(drafts: Draft[]) {
  return drafts.map(({ op_type, feature_id, properties, geometry, client_operation_id }) => ({
    op_type, feature_id, properties, ...(geometry ? { geometry } : {}),
    ...(client_operation_id !== undefined ? { client_operation_id } : {})
  }));
}
