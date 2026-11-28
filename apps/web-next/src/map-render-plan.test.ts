import {describe,it,expect} from "vitest";
import {layerRanges,renderPlan,layerRangeKey,groupByLayer,rankMap,readCamera} from "./map-render-plan";
describe("stable map render plan",()=>{
 it("style changes do not recreate map layers",()=>{
  const a=layerRanges(["a","b"],{a:{minZoom:3,maxZoom:19}}),b=layerRanges(["a","b"],{a:{minZoom:3,maxZoom:19,color:"red"} as {minZoom:number;maxZoom:number}});
  expect(layerRangeKey(a)).toBe(layerRangeKey(b));expect(renderPlan(a,b)).toEqual({remove:[],add:[],ranges:[],reorder:false});
 });
 it("isolates range updates and layer order changes",()=>{
  const a=layerRanges(["a","b"],{}),b=layerRanges(["b","c"],{b:{minZoom:4,maxZoom:20}});
  expect(renderPlan(a,b)).toEqual({remove:["a"],add:[{id:"c",min:0,max:24}],ranges:[{id:"b",min:4,max:20}],reorder:true});
 });
 it("indexes rows and ranks in one pass",()=>{
  const rows=Array.from({length:10000},(_,i)=>({layer:{id:"l"+(i%100)},i}));
  const g=groupByLayer(rows);expect(g.size).toBe(100);expect(g.get("l9")?.length).toBe(100);expect(rankMap(["b","a"]).get("a")).toBe(1);
 });
 it("resumes valid camera and old center/zoom format only",()=>{
  expect(readCamera({center:[106,10],zoom:15})).toEqual({center:[106,10],zoom:15,bearing:0,pitch:0});
  expect(readCamera({center:[106,10],zoom:15,bearing:20,pitch:45})?.pitch).toBe(45);
  expect(readCamera({center:[106,90],zoom:15})).toBeNull();expect(readCamera({center:[106,10],zoom:15,pitch:90})).toBeNull();
 });
});
