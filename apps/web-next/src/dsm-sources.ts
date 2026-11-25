// Spec 058: Data Source Manager converters. Every remote/text source becomes a GeoJSON file that goes
// through the normal server import (inspection, CRS, validation), so nothing bypasses the import checks.
export type Fc = { type: "FeatureCollection"; features: { type: "Feature"; geometry: unknown; properties: Record<string, unknown> }[] };

export function splitCsv(text: string, delim: string): string[][] {
  const rows: string[][] = []; let row: string[] = [], cell = "", q = false;
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (q) { if (c === '"') { if (text[i + 1] === '"') { cell += '"'; i++; } else q = false; } else cell += c; continue; }
    if (c === '"') q = true; else if (c === delim) { row.push(cell); cell = ""; }
    else if (c === "\n" || c === "\r") { if (c === "\r" && text[i + 1] === "\n") i++; row.push(cell); rows.push(row); row = []; cell = ""; }
    else cell += c;
  }
  if (cell || row.length) { row.push(cell); rows.push(row); }
  return rows.filter(r => r.some(x => x.trim() !== ""));
}
export const guessDelim = (text: string) => { const line = text.split(/\r?\n/, 1)[0] ?? ""; return [",", ";", "\t", "|"].sort((a, b) => line.split(b).length - line.split(a).length)[0]; };
// Vietnamese sheets often write decimals with a comma ("106,7123") when the delimiter is ";".
const num = (v: string) => { const t = v.trim().replace(/\s/g, ""); const n = Number(/^-?\d+,\d+$/.test(t) ? t.replace(",", ".") : t); return t !== "" && Number.isFinite(n) ? n : null; };
const X_NAMES = ["x", "lon", "lng", "long", "longitude", "kinh_do", "kinhdo", "easting", "e"];
const Y_NAMES = ["y", "lat", "latitude", "vi_do", "vido", "northing", "n"];
export const guessXY = (header: string[]) => { const low = header.map(h => h.trim().toLowerCase()); const f = (names: string[]) => header[low.findIndex(h => names.includes(h))] ?? ""; return { x: f(X_NAMES), y: f(Y_NAMES) }; };

export function csvToGeoJSON(text: string, opt: { delim: string; x: string; y: string }): { fc: Fc; skipped: number } {
  const [header, ...rows] = splitCsv(text.replace(/^﻿/, ""), opt.delim);
  if (!header) throw Error("Tệp trống");
  const xi = header.indexOf(opt.x), yi = header.indexOf(opt.y);
  if (xi < 0 || yi < 0) throw Error("Chọn cột X và cột Y");
  let skipped = 0; const features: Fc["features"] = [];
  for (const r of rows) {
    const x = num(r[xi] ?? ""), y = num(r[yi] ?? "");
    if (x === null || y === null) { skipped++; continue; }
    const properties: Record<string, unknown> = {}; header.forEach((h, i) => { if (i !== xi && i !== yi) { const n = num(r[i] ?? ""); properties[h || "cot_" + (i + 1)] = n ?? (r[i] ?? ""); } });
    features.push({ type: "Feature", geometry: { type: "Point", coordinates: [x, y] }, properties });
  }
  return { fc: { type: "FeatureCollection", features }, skipped };
}

export const asFile = (fc: Fc | unknown, name: string) => new File([JSON.stringify(fc)], name.replace(/[^\p{L}\p{N}_ -]+/gu, "_").slice(0, 60) + ".geojson", { type: "application/geo+json" });

const MAX = 50000;
async function getJson(url: string) {
  const r = await fetch(url, { credentials: "omit" });
  if (!r.ok) throw Error("Máy chủ trả lỗi " + r.status);
  return r.json();
}
const httpUrl = (raw: string) => { const u = new URL(raw.trim()); if (u.protocol !== "https:" && u.protocol !== "http:") throw Error("Chỉ hỗ trợ http(s)"); return u; };

// WFS 2.0 / OGC API Features
export async function wfsLayers(raw: string): Promise<{ name: string; title: string }[]> {
  const u = httpUrl(raw);
  if (/\/collections\/?$/.test(u.pathname) || u.searchParams.get("f") === "json") {
    const j = await getJson(u.toString()); return (j.collections ?? []).map((c: { id: string; title?: string }) => ({ name: c.id, title: c.title ?? c.id }));
  }
  u.search = ""; u.searchParams.set("service", "WFS"); u.searchParams.set("request", "GetCapabilities");
  const r = await fetch(u.toString(), { credentials: "omit" }); if (!r.ok) throw Error("Máy chủ trả lỗi " + r.status);
  const xml = new DOMParser().parseFromString(await r.text(), "application/xml");
  return [...xml.getElementsByTagNameNS("*", "FeatureType")].map(ft => ({ name: ft.getElementsByTagNameNS("*", "Name")[0]?.textContent ?? "", title: ft.getElementsByTagNameNS("*", "Title")[0]?.textContent ?? "" })).filter(x => x.name);
}
export async function wfsFetch(raw: string, layer: string, limit: number): Promise<Fc> {
  const u = httpUrl(raw), n = Math.min(Math.max(1, limit), MAX);
  if (/\/collections\/?$/.test(u.pathname)) { const items = new URL(u.toString().replace(/\/?$/, "/") + encodeURIComponent(layer) + "/items"); items.searchParams.set("f", "json"); items.searchParams.set("limit", String(n)); return getJson(items.toString()); }
  u.search = ""; for (const [k, v] of Object.entries({ service: "WFS", version: "2.0.0", request: "GetFeature", typeNames: layer, outputFormat: "application/json", srsName: "EPSG:4326", count: String(n) })) u.searchParams.set(k, v);
  return getJson(u.toString());
}

// ArcGIS REST: service root lists layers; a layer URL (…/FeatureServer/0) is queried as GeoJSON.
export async function arcgisLayers(raw: string): Promise<{ name: string; title: string }[]> {
  const u = httpUrl(raw); if (/\/\d+\/?$/.test(u.pathname)) return [{ name: u.toString().replace(/\/$/, ""), title: "Lớp " + u.pathname.split("/").filter(Boolean).pop() }];
  u.search = "f=json"; const j = await getJson(u.toString()); const base = raw.trim().split("?")[0].replace(/\/$/, "");
  return [...(j.layers ?? []), ...(j.tables ?? [])].map((l: { id: number; name: string }) => ({ name: base + "/" + l.id, title: l.name }));
}
export async function arcgisFetch(layerUrl: string, limit: number): Promise<Fc> {
  const u = httpUrl(layerUrl.replace(/\/$/, "") + "/query");
  for (const [k, v] of Object.entries({ where: "1=1", outFields: "*", outSR: "4326", f: "geojson", resultRecordCount: String(Math.min(Math.max(1, limit), MAX)) })) u.searchParams.set(k, v);
  const j = await getJson(u.toString()); if (j.error) throw Error(j.error.message ?? "ArcGIS báo lỗi"); return j;
}
