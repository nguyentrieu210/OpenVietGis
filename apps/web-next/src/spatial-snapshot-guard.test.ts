import {describe,it,expect} from "vitest";
import type {SpatialLayer,SpatialFeaturePage,SpatialFeature} from "@openvietgis/client-sdk";
import {verifyFeaturePage,completeSnapshot} from "./spatial-snapshot-guard";
const l={id:"layer",source_id:"src",source:{content_revision:"c",schema_revision:"s",feature_count:2}} as unknown as SpatialLayer;
const f=(id:string)=>({feature_id:id,attributes:{},geometry_state:"absent",geometry:null} as SpatialFeature);
const p={context:{organization_id:"org",project_id:"prj",layer_id:"layer",source_id:"src",content_revision:"c",schema_revision:"s",geometry_included:true,response_crs:{code:4326},transformation:null},items:[f("a")],next_after_feature_id:"a"} as SpatialFeaturePage;
describe("cache snapshot guards",()=>{
 it("rejects stale scope, duplicate FID and repeated or empty cursor",()=>{
  const check=(page:SpatialFeaturePage,seen=new Set<string>(),cursors=new Set<string>())=>verifyFeaturePage(page,l,"org","prj",100,seen,cursors);
  expect(check(p)).toEqual(new Set(["a"]));expect(()=>check({...p,context:{...p.context,schema_revision:"other"}})).toThrow();
  expect(()=>check(p,new Set(["a"]))).toThrow();expect(()=>check(p,new Set(),new Set(["a"]))).toThrow();
  expect(()=>check({...p,items:[]})).toThrow();expect(()=>check({...p,items:[f("a"),f("a")]})).toThrow();
 });
 it("never caches incomplete or duplicated rows",()=>{
  expect(completeSnapshot([{feature:f("a"),geometry:null},{feature:f("b"),geometry:null}],2)).toBe(true);
  expect(completeSnapshot([{feature:f("a"),geometry:null},{feature:f("a"),geometry:null}],2)).toBe(false);
  expect(completeSnapshot([{feature:f("a"),geometry:null}],2)).toBe(false);
 });
});
