import {describe,it,expect} from "vitest";
import type {SpatialFeature,SpatialFeaturePage,SpatialLayer} from "@openvietgis/client-sdk";
import {atlasCamera,atlasFeatures,atlasText} from "./layout-atlas";
const crs={authority:"EPSG",code:4326,coordinate_order:"xy",x_axis:"longitude",y_axis:"latitude",unit:"degree"} as const;
const f={feature_id:"p1",geometry_state:"present",attributes:{zone:"A"},geometry:{type:"Polygon",coordinate_reference:crs,coordinates:[[[106,10],[106.01,10],[106.01,10.01],[106,10],[106,10]]]}} as SpatialFeature;
const l={id:"l",source_id:"s",lifecycle:"active",source:{feature_count:1,content_revision:"c",schema_revision:"r"},capabilities:[{name:"feature_read",available:true,actor_authorized:true}]} as unknown as SpatialLayer;
const page={context:{organization_id:"o",project_id:"p",layer_id:"l",source_id:"s",content_revision:"c",schema_revision:"r",response_crs:{code:4326},geometry_included:true},items:[f],next_after_feature_id:null} as SpatialFeaturePage;
describe("bounded atlas",()=>{
 it("requires exact scope, revision, complete source",()=>{
  expect(atlasFeatures(page,l,"o","p")).toEqual([f]);
  expect(()=>atlasFeatures({...page,next_after_feature_id:"p2"},l,"o","p")).toThrow();
  expect(()=>atlasFeatures(page,l,"other","p")).toThrow();
  expect(()=>atlasFeatures({...page,context:{...page.context,content_revision:"old"}},l,"o","p")).toThrow();
 });
 it("fits aspect ratio and rejects dateline/absent geometry",()=>{
  const wide=atlasCamera(f,200,100),tall=atlasCamera(f,100,200);
  expect(wide.lng).toBeCloseTo(106.005,6);expect(wide.lat).toBeCloseTo(10.005,5);
  expect(wide.zoom).toBeLessThan(tall.zoom);
  expect(()=>atlasCamera({...f,geometry_state:"absent"},100,100)).toThrow();
  expect(()=>atlasCamera({...f,geometry:{type:"LineString",coordinate_reference:crs,coordinates:[[-179,10],[179,10]]}},100,100)).toThrow();
 });
 it("substitutes only allowed tokens without evaluation",()=>{
  expect(atlasText("{page}/{pages} {fid} {field:zone}",f,0,2)).toBe("1/2 p1 A");
  expect(atlasText("{field:missing} {other}",f,0,1)).toBe(" {other}");
 });
});
