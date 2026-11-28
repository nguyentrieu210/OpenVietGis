import {describe,it,expect} from "vitest";
import type {FeatureCollection} from "geojson";
import {comparisonPrefs,comparisonCollection,moveComparisonLayer} from "./secondary-map-plan";
const layers=[{id:"a",name:"A",min:0,max:24},{id:"b",name:"B",min:0,max:24}];
const f=(id:string)=>({type:"Feature" as const,geometry:{type:"Point" as const,coordinates:[106,11]},properties:{layerId:id}});
describe("secondary map preferences and budget",()=>{
 it("prunes stale IDs, keeps independent order and validates camera",()=>{
  const p=comparisonPrefs({order:["b","missing","b"],hidden:["missing","a"],camera:{center:[181,0],zoom:5}},layers);
  expect(p.order).toEqual(["b","a"]);expect(p.hidden).toEqual(["a"]);expect(p.camera).toBeNull();
  expect(moveComparisonLayer(p.order,"a",-1)).toEqual(["a","b"]);expect(layers.map(l=>l.id)).toEqual(["a","b"]);
 });
 it("filters unauthorized layers and rejects over-budget geometry",()=>{
  const c:FeatureCollection={type:"FeatureCollection",features:[f("a"),f("unknown")]};
  expect(comparisonCollection(c,layers).features).toEqual([f("a")]);
  expect(()=>comparisonCollection({...c,features:Array.from({length:5001},()=>f("a"))},layers)).toThrow();
  expect(()=>comparisonCollection({...c,features:[{...f("a"),geometry:{type:"LineString",coordinates:Array.from({length:200001},()=>[106,11])}}]},layers)).toThrow();
 });
});
