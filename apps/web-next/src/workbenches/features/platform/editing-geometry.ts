/** Native-coordinate sketch geometry. No implicit CRS transform or metric claim. */
export type XY = [number, number];
export type NativeBounds = [number, number, number, number];
export type SketchFamily = "Point" | "LineString" | "Polygon";
export type Sketch =
  | { type: "Point"; coordinates: XY }
  | { type: "LineString"; coordinates: XY[] }
  | { type: "Polygon"; coordinates: XY[][] };
export interface SketchFeature {
  id: string;
  geometry: Sketch;
}
export interface Viewport {
  bounds: NativeBounds;
  width: number;
  height: number;
}
export interface SnapCandidate {
  point: XY;
  featureId: string;
  kind: "vertex" | "segment";
  distance: number;
}

function xy(value: unknown): XY | null {
  return Array.isArray(value) && value.length === 2 &&
    value.every((item: unknown) => typeof item === "number" && Number.isFinite(item))
    ? [Number(value[0]), Number(value[1])] : null;
}
function positions(value: unknown): XY[] | null {
  if (!Array.isArray(value)) return null;
  const result = value.map(xy);
  return result.every((p): p is XY => p !== null) ? result : null;
}
export function samePosition(a: XY, b: XY): boolean {
  return a[0] === b[0] && a[1] === b[1];
}
export function parseSketch(value: unknown): Sketch | null {
  if (typeof value !== "object" || value === null || !("type" in value) || !("coordinates" in value)) return null;
  if (value.type === "Point") {
    const p = xy(value.coordinates);
    return p ? { type: "Point", coordinates: p } : null;
  }
  if (value.type === "LineString") {
    const points = positions(value.coordinates);
    return points && points.length >= 2 && points.length <= 4096
      ? { type: "LineString", coordinates: points } : null;
  }
  if (value.type === "Polygon" && Array.isArray(value.coordinates)) {
    const rings = value.coordinates.map(positions);
    if (rings.length && rings.every((ring): ring is XY[] =>
      ring !== null && ring.length >= 4 && samePosition(ring[0]!, ring[ring.length - 1]!)) &&
      rings.reduce((sum, ring) => sum + ring.length, 0) <= 4096) {
      return { type: "Polygon", coordinates: rings };
    }
  }
  return null;
}

export function pathsOf(geometry: Sketch): XY[][] {
  return geometry.type === "Point" ? [[geometry.coordinates]] :
    geometry.type === "LineString" ? [geometry.coordinates] : geometry.coordinates;
}

/** Empty, degenerate and repeated consecutive vertices don't produce a draft. */
export function buildSketch(family: SketchFamily, points: readonly XY[]): Sketch | null {
  if (points.length > (family === "Polygon" ? 4095 : 4096) ||
      points.some((p, index) => xy(p) === null || (index > 0 && samePosition(p, points[index - 1]!)))) return null;
  if (family === "Point") return points.length === 1 ? { type: family, coordinates: points[0]! } : null;
  if (family === "LineString") return points.length >= 2 ? { type: family, coordinates: [...points] } : null;
  if (points.length < 3 || samePosition(points[0]!, points[points.length - 1]!) ||
      new Set(points.map(p => JSON.stringify(p))).size < 3) return null;
  const origin = points[0]!;
  const ring = [...points, origin];
  let twiceArea = 0;
  for (let i = 0; i < ring.length - 1; i++) {
    const a = ring[i]!, b = ring[i + 1]!;
    twiceArea += (a[0] - origin[0]) * (b[1] - origin[1]) - (b[0] - origin[0]) * (a[1] - origin[1]);
  }
  return twiceArea !== 0 ? { type: family, coordinates: [ring] } : null;
}

/** A polygon with holes can't be safely modified by the first graphical slice. */
export function editableVertices(geometry: Sketch): XY[] | null {
  if (geometry.type === "Polygon") return geometry.coordinates.length === 1
    ? geometry.coordinates[0]!.slice(0, -1) : null;
  return geometry.type === "Point" ? [geometry.coordinates] : [...geometry.coordinates];
}

export function padBounds(extent: NativeBounds, minimumSpan: number): NativeBounds {
  const [west, south, east, north] = extent;
  const x = (west + east) / 2, y = (south + north) / 2;
  const half = Math.max(east - west, north - south, minimumSpan) * 0.65;
  return [x - half, y - half, x + half, y + half];
}

export function toScreen(point: XY, view: Viewport): XY {
  const [west, south, east, north] = view.bounds;
  return [(point[0] - west) / (east - west) * view.width, (north - point[1]) / (north - south) * view.height];
}

export function fromScreen(point: XY, view: Viewport): XY {
  const [west, south, east, north] = view.bounds;
  return [west + point[0] / view.width * (east - west), north - point[1] / view.height * (north - south)];
}

/** Snap to the nearest visible candidate in screen pixels. Exact ties prefer vertices. */
export function nearestSnap(target: XY, features: readonly SketchFeature[], view: Viewport, tolerance: number,
  excludedId?: string): SnapCandidate | null {
  if (!Number.isFinite(tolerance) || tolerance < 0 || view.width <= 0 || view.height <= 0) return null;
  let best: SnapCandidate | null = null;
  function consider(p: XY, screen: XY, featureId: string, kind: SnapCandidate["kind"]): void {
    // Only candidates displayed within the current sketch bounds are eligible.
    if (screen[0] < 0 || screen[1] < 0 || screen[0] > view.width || screen[1] > view.height) return;
    const distance = Math.hypot(screen[0] - target[0], screen[1] - target[1]);
    if (distance <= tolerance && (best === null || distance < best.distance ||
      (distance === best.distance && kind === "vertex" && best.kind === "segment"))) {
      best = { point: p, featureId, kind, distance };
    }
  }
  for (const feature of features) {
    if (feature.id === excludedId) continue;
    for (const path of pathsOf(feature.geometry)) {
      for (let index = 0; index < path.length; index++) {
        const p = path[index]!, pScreen = toScreen(p, view);
        consider(p, pScreen, feature.id, "vertex");
        if (index === 0) continue;
        const a = path[index - 1]!, as = toScreen(a, view);
        const dx = pScreen[0] - as[0], dy = pScreen[1] - as[1];
        const length2 = dx * dx + dy * dy;
        if (length2 === 0) continue;
        const t = ((target[0] - as[0]) * dx + (target[1] - as[1]) * dy) / length2;
        if (t <= 0 || t >= 1) continue;
        const projected: XY = [as[0] + t * dx, as[1] + t * dy];
        consider([a[0] + t * (p[0] - a[0]), a[1] + t * (p[1] - a[1])], projected, feature.id, "segment");
      }
    }
  }
  return best;
}
