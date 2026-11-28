import {describe,it,expect} from "vitest";
import {vertices,moveVertex} from "./vertices";
describe("desktop XY vertex editor",()=>{
 it("moves both copies of closed ring start without mutating source",()=>{
  const g:GeoJSON.Polygon={type:"Polygon",coordinates:[[[0,0],[2,0],[2,2],[0,0]]]};
  const v=vertices(g);expect(v).toHaveLength(3);
  const n=moveVertex(g,v[0]!,[1,1]) as GeoJSON.Polygon;
  expect(n.coordinates[0]![0]).toEqual([1,1]);expect(n.coordinates[0]!.at(-1)).toEqual([1,1]);expect(g.coordinates[0]![0]).toEqual([0,0]);
 });
 it("preserves other parts and holes in multipolygon",()=>{
  const g:GeoJSON.MultiPolygon={type:"MultiPolygon",coordinates:[[[[0,0],[2,0],[2,2],[0,0]],[[.5,.5],[1,.5],[1,1],[.5,.5]]],[[[3,3],[4,3],[4,4],[3,3]]]]};
  const n=moveVertex(g,vertices(g)[3]!,[.6,.6]) as GeoJSON.MultiPolygon;
  expect(n.coordinates[0]![1]![0]).toEqual([.6,.6]);expect(n.coordinates[1]).toEqual(g.coordinates[1]);
 });
 it("rejects oversized previews and invalid coordinates",()=>{
  expect(()=>vertices({type:"MultiPoint",coordinates:Array.from({length:257},()=>[1,2])})).toThrow("VERTEX_PREVIEW_LIMIT");
  const g:GeoJSON.Point={type:"Point",coordinates:[1,2]};expect(()=>moveVertex(g,vertices(g)[0]!,[181,1])).toThrow();
 });
});
