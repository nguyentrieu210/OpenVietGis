import {describe,it,expect,vi} from "vitest";
import type {SpatialLayer,LayerQueryResponse} from "@openvietgis/client-sdk";
import {loadAttributePopulation} from "./attribute-population";
const layer={id:"l",source_id:"s",source:{content_revision:"c",schema_revision:"v",feature_count:101,schema:{fields:[]}}} as unknown as SpatialLayer;
const page=(offset:number,ids:string[])=>({layer_id:"l",source_id:"s",content_revision:"c",schema_revision:"v",offset,limit:100,query_fingerprint:"pin",attribute_only:true,total_count:101,has_more:offset===0,items:ids.map(feature_id=>({feature_id,attributes:{},geometry:null,geometry_state:"absent"}))} as LayerQueryResponse);
const first=page(0,Array.from({length:100},(_,i)=>"id"+i));
describe("revision-pinned attribute population",()=>{
 it("pins second page, counts all rows including absent geometry",async()=>{
  const request=vi.fn(async(body:Record<string,unknown>)=>body.offset===0?first:page(100,["last"]));
  const rows=await loadAttributePopulation(layer,200,request,()=>true,()=>{});
  expect(rows.length).toBe(101);expect(request.mock.calls[1]?.[0]).toMatchObject({offset:100,expected_query_fingerprint:"pin",attribute_only:true});
 });
 it("rejects repeated FID across pages",async()=>{
  await expect(loadAttributePopulation(layer,200,async b=>b.offset===0?first:page(100,["id0"]),()=>true,()=>{})).rejects.toThrow("Trùng FID");
 });
 it("fails closed on scope revocation and cap",async()=>{
  let active=true;const request=vi.fn(async()=>{active=false;return first});
  await expect(loadAttributePopulation(layer,200,request,()=>active,()=>{})).rejects.toThrow();
  await expect(loadAttributePopulation(layer,100,request,()=>true,()=>{})).rejects.toThrow();
  expect(request).toHaveBeenCalledTimes(1);
 });
});
