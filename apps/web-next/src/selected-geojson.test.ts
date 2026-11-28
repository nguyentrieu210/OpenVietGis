import {describe,it,expect} from "vitest";
import type {SpatialFeaturePage,SpatialFeature,SpatialLayer} from "@openvietgis/client-sdk";
import {exportSelectedGeoJSON} from "./selected-geojson";
const layer={id:"l",source_id:"s",source:{content_revision:"c",schema_revision:"v",feature_count:2}} as unknown as SpatialLayer;
const context={organization_id:"o",project_id:"p",layer_id:"l",source_id:"s",content_revision:"c",schema_revision:"v",geometry_included:true,response_crs:{code:4326}};
const a={feature_id:"a",geometry_state:"present",geometry:{type:"Point",coordinates:[106,10],coordinate_reference:{code:4326}},attributes:{decimal:"9007199254740993.01"}} as unknown as SpatialFeature;
const b={feature_id:"b",geometry_state:"absent",geometry:null,attributes:{name:"missing"}} as SpatialFeature;
const page=(cursor:string|null)=>({context,items:cursor?[b]:[a],next_after_feature_id:cursor?null:"a"}) as unknown as SpatialFeaturePage;
describe("selected GeoJSON export",()=>{
 it("includes selected unloaded features and explicit missing geometry state",async()=>{
  const r=await exportSelectedGeoJSON(layer,"o","p",new Set(["s:a","s:b","other:secret"]),async c=>page(c),()=>true,()=>{});
  expect(r.count).toBe(2);expect(r.missing).toBe(1);
  expect(JSON.parse(r.json).features).toEqual([{type:"Feature",id:"a",geometry:{type:"Point",coordinates:[106,10]},properties:{decimal:"9007199254740993.01"},ovg_geometry_state:"present"},{type:"Feature",id:"b",geometry:null,properties:{name:"missing"},ovg_geometry_state:"absent"}]);
 });
 it("rejects missing FID, revision changes and invalid coordinates",async()=>{
  await expect(exportSelectedGeoJSON(layer,"o","p",new Set(["s:x"]),async c=>page(c),()=>true,()=>{})).rejects.toThrow();
  await expect(exportSelectedGeoJSON(layer,"o","p",new Set(["s:a"]),async()=>({...page(null),context:{...page(null).context,content_revision:"other"}}),()=>true,()=>{})).rejects.toThrow();
  await expect(exportSelectedGeoJSON(layer,"o","p",new Set(["s:a"]),async()=>({...page(null),items:[{...a,geometry:{...a.geometry!,coordinates:[181,0]}} as unknown as SpatialFeature]}),()=>true,()=>{})).rejects.toThrow();
 });
});
