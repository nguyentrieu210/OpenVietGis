import {describe,expect,it} from "vitest";
import type {Geometry} from "geojson";
import {geometryPartCount,geometryRingCount,geometryVertices,segmentEditable,vertexEditable,withGeometryVertices,addPolygonHole,removePolygonHole,sharedVertexChanges,addGeometryPart,removeGeometryPart,reshapeGeometryPath} from "./advanced-geometry";

describe("multipart vertex editing",()=>{
 it("exposes editable multipart lines while keeping split and merge capability scoped",()=>{
  const line:Extract<Geometry,{type:"MultiLineString"}>={type:"MultiLineString",coordinates:[[[0,0],[1,1]],[[4,4],[5,5],[6,6]]]};
  expect(vertexEditable(line)).toBe(true);expect(segmentEditable(line)).toBe(false);
  expect(geometryPartCount(line)).toBe(2);expect(geometryVertices(line,1)).toEqual([[4,4],[5,5],[6,6]]);
  const changed=withGeometryVertices(line,1,0,[[4,4],[5,7],[6,6]]) as typeof line;
  expect(changed.coordinates).toEqual([[[0,0],[1,1]],[[4,4],[5,7],[6,6]]]);
  expect(line.coordinates[1]).toEqual([[4,4],[5,5],[6,6]]);
 });

 it("edits one selected multipart polygon ring and keeps it closed",()=>{
  const polygon:Extract<Geometry,{type:"MultiPolygon"}>={type:"MultiPolygon",coordinates:[
   [[[0,0],[2,0],[2,2],[0,0]],[[.5,.5],[1,.5],[.5,1],[.5,.5]]],
   [[[5,5],[7,5],[7,7],[5,5]]]
  ]};
  expect(vertexEditable(polygon)).toBe(true);expect(segmentEditable(polygon)).toBe(false);
  expect(geometryPartCount(polygon)).toBe(2);expect(geometryRingCount(polygon,0)).toBe(2);
  expect(geometryVertices(polygon,0,1)).toEqual([[.5,.5],[1,.5],[.5,1]]);
  const changed=withGeometryVertices(polygon,0,1,[[.5,.5],[1.2,.5],[.5,1]]) as typeof polygon;
  expect(changed.coordinates[0]?.[1]).toEqual([[.5,.5],[1.2,.5],[.5,1],[.5,.5]]);
  expect(changed.coordinates[1]).toEqual(polygon.coordinates[1]);
  expect(polygon.coordinates[0]?.[1]?.[1]).toEqual([1,.5]);
 });

 it("keeps single polygon hole selection and excludes non-line/polygon geometry",()=>{
  const polygon:Geometry={type:"Polygon",coordinates:[[[0,0],[2,0],[2,2],[0,0]],[[.5,.5],[1,.5],[.5,1],[.5,.5]]]};
  expect(geometryPartCount(polygon)).toBe(1);expect(geometryRingCount(polygon)).toBe(2);
  expect(geometryVertices(polygon,0,1)).toEqual([[.5,.5],[1,.5],[.5,1]]);
  expect(withGeometryVertices(polygon,0,1,[[.5,.5],[1.2,.5],[.5,1]])).toMatchObject({type:"Polygon",coordinates:[polygon.coordinates[0],[[.5,.5],[1.2,.5],[.5,1],[.5,.5]]]});
  expect(vertexEditable({type:"Point",coordinates:[0,0]})).toBe(false);
 });
});

describe("polygon hole operations",()=>{
 const g:Geometry={type:"Polygon",coordinates:[[[0,0],[4,0],[4,4],[0,4],[0,0]]]};
 it("adds and removes an interior ring without changing source",()=>{
  const h=addPolygonHole(g,0,[[1,1],[2,1],[2,2],[1,2]]);
  expect(geometryRingCount(h)).toBe(2);expect(geometryRingCount(g)).toBe(1);
  expect(removePolygonHole(h,0,1)).toEqual(g);
  expect(()=>removePolygonHole(h,0,0)).toThrow();
 });
 it("rejects outside, boundary crossing, touching and nested holes",()=>{
  expect(()=>addPolygonHole(g,0,[[5,5],[6,5],[6,6]])).toThrow();
  expect(()=>addPolygonHole(g,0,[[1,1],[5,1],[1,2]])).toThrow();
  expect(()=>addPolygonHole(g,0,[[0,1],[1,1],[1,2]])).toThrow();
  const h=addPolygonHole(g,0,[[1,1],[3,1],[3,3],[1,3]]);
  expect(()=>addPolygonHole(h,0,[[1.5,1.5],[2,1.5],[2,2]])).toThrow();
 });
});
describe("exact shared vertex changes",()=>{
 it("updates matching vertices in both polygons and keeps closed rings",()=>{
  const a:Geometry={type:"Polygon",coordinates:[[[0,0],[1,0],[1,1],[0,1],[0,0]]]};
  const b:Geometry={type:"Polygon",coordinates:[[[1,0],[2,0],[2,1],[1,1],[1,0]]]};
  const rows=[{id:"a",geometry:a},{id:"b",geometry:b}];
  const changes=sharedVertexChanges(rows,"a",[1,0],[1.1,0]);
  expect(changes.map(x=>x.id)).toEqual(["a","b"]);
  expect(changes[1]!.geometry).toEqual({type:"Polygon",coordinates:[[[1.1,0],[2,0],[2,1],[1,1],[1.1,0]]]});
  expect(a.coordinates[0]![1]).toEqual([1,0]);
  expect(()=>sharedVertexChanges(rows,"a",[1,0],[1,1])).toThrow();
  expect(()=>sharedVertexChanges(rows,"a",[3,0],[3,1])).toThrow();
 });
});

describe("multipart and path reshape",()=>{
 it("adds and removes multipart members, retaining source family",()=>{
  const g:Geometry={type:"MultiLineString",coordinates:[[[0,0],[1,1]]]};
  const next=addGeometryPart(g,[[2,2],[3,3]]);
  expect(geometryPartCount(next)).toBe(2);expect(removeGeometryPart(next,1)).toEqual(g);
  expect(()=>removeGeometryPart(g,0)).toThrow();
  expect(()=>addGeometryPart({type:"LineString",coordinates:[[0,0],[1,1]]},[[2,2],[3,3]])).toThrow();
  const polygon:Geometry={type:"MultiPolygon",coordinates:[[[[0,0],[1,0],[1,1],[0,0]]]]};
  const added=addGeometryPart(polygon,[[3,3],[4,3],[4,4]]);
  expect(geometryPartCount(added)).toBe(2);expect(removeGeometryPart(added,1)).toEqual(polygon);
 });
 it("replaces only selected forward path, preserves endpoints and rejects invalid shape",()=>{
  const g:Geometry={type:"LineString",coordinates:[[0,0],[1,1],[2,0],[3,0]]};
  expect(reshapeGeometryPath(g,0,0,0,2,[[1,2]])).toEqual({type:"LineString",coordinates:[[0,0],[1,2],[2,0],[3,0]]});
  expect(()=>reshapeGeometryPath(g,0,0,2,1,[])).toThrow();
  const p:Geometry={type:"Polygon",coordinates:[[[0,0],[4,0],[4,4],[0,4],[0,0]]]};
  expect(reshapeGeometryPath(p,0,0,0,1,[[2,1]])).toMatchObject({coordinates:[[[0,0],[2,1],[4,0],[4,4],[0,4],[0,0]]]});
  expect(()=>reshapeGeometryPath(p,0,0,0,3,[])).toThrow();
 });
});
