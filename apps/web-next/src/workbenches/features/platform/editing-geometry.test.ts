import { describe, expect, it } from "vitest";
import { buildSketch, editableVertices, fromScreen, nearestSnap, padBounds, parseSketch, toScreen, type SketchFeature, type Viewport } from "./editing-geometry";

const view: Viewport = { bounds: [100, 200, 110, 210], width: 1000, height: 500 };

describe("native geometry editor", () => {
  it("preserves original coordinate units through the viewport transform", () => {
    const point: [number, number] = [104.25, 208];
    expect(fromScreen(toScreen(point, view), view)).toEqual(point);
    expect(padBounds([100, 200, 100, 200], 10)).toEqual([93.5, 193.5, 106.5, 206.5]);
  });
  it("closes a simple ring only on explicit sketch completion and rejects degenerate drafts", () => {
    expect(buildSketch("Polygon", [[101, 201], [103, 201], [103, 203]])).toEqual({
      type: "Polygon", coordinates: [[[101, 201], [103, 201], [103, 203], [101, 201]]]
    });
    expect(buildSketch("Polygon", [[1, 1], [2, 2], [3, 3]])).toBeNull();
    expect(buildSketch("LineString", [[1, 1], [1, 1]])).toBeNull();
    expect(buildSketch("Point", [])).toBeNull();
  });
  it("rejects unsupported dimensions, unclosed rings, and unsupported geometry", () => {
    expect(parseSketch({type: "Point", coordinates: [1, 2, 3]})).toBeNull();
    expect(parseSketch({type: "Polygon", coordinates: [[[1, 1], [2, 1], [2, 2], [1, 2]]]})).toBeNull();
    expect(parseSketch({type: "MultiPoint", coordinates: [[1, 1], [2, 2]]})).toBeNull();
  });
  it("refuses graphical modification of a polygon with holes rather than silently dropping rings", () => {
    const polygon = parseSketch({type: "Polygon", coordinates: [
      [[0,0],[4,0],[4,4],[0,0]], [[2,1],[3,1],[3,2],[2,1]]
    ]})!;
    expect(editableVertices(polygon)).toBeNull();
  });
  it("snaps within explicit screen pixel tolerance to an exact vertex", () => {
    const features: SketchFeature[] = [{ id: "known-point", geometry: {type: "Point", coordinates: [105, 205]} }];
    const point = toScreen([105, 205], view);
    expect(nearestSnap([point[0] + 3, point[1] + 4], features, view, 5)).toEqual({
      point: [105, 205], featureId: "known-point", kind: "vertex", distance: 5
    });
    expect(nearestSnap([point[0] + 3, point[1] + 4], features, view, 4.99)).toBeNull();
    expect(nearestSnap(point, features, view, 10, "known-point")).toBeNull();
  });
  it("projects a segment candidate in screen space but returns native coordinates", () => {
    const features: SketchFeature[] = [{ id: "line", geometry: {
      type: "LineString", coordinates: [[100, 205], [110, 205]]
    } }];
    expect(nearestSnap([300, 254], features, view, 5)).toEqual({
      point: [103, 205], featureId: "line", kind: "segment", distance: 4
    });
  });
  it("never snaps an offscreen vertex even within near-edge tolerance", () => {
    const features: SketchFeature[] = [{ id: "hidden", geometry: {type: "Point", coordinates: [99.95, 205]} }];
    expect(nearestSnap([1, 250], features, view, 20)).toBeNull();
  });
});
