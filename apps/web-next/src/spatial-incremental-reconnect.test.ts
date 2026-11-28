import {describe,it,expect,vi} from "vitest";
import {ApiClient,type SpatialLayer} from "@openvietgis/client-sdk";
import {incrementalLayerRows} from "./SpatialIncremental";
import type {StudioRow} from "./studio-contract";

const org="org",pid="project",source="source",layerId="layer";
const rev0="0".repeat(64),rev1="1".repeat(64),rev2="2".repeat(64),schema="a".repeat(64);
const baseLayer={
 id:layerId,organization_id:org,project_id:pid,source_id:source,name:"Layer",layer_revision:1,
 capabilities:[{name:"feature_read",available:true,actor_authorized:true}],
 source:{feature_count:2,content_revision:rev0,schema_revision:schema}
} as unknown as SpatialLayer;
const nextLayer={...baseLayer,source:{...baseLayer.source,feature_count:1,content_revision:rev2}} as SpatialLayer;
const operation=(id:string,ordinal:number,op_type:"update"|"delete",feature_id:string)=>({
 id,ordinal,op_type,feature_id,before_geometry:null,after_geometry:null,before_properties:null,after_properties:null,created_at:"2026-01-01T00:00:00Z"
});
const changeset=(id:string,base:string,result:string,ops:ReturnType<typeof operation>[])=>({
 id,source_id:source,actor_display_snapshot:"Fixture",operation_id:"op-"+id,base_content_revision:base,resulting_content_revision:result,
 base_catalog_revision:1,resulting_catalog_revision:2,summary:id,status:"committed",operation_count:ops.length,reverted_from_changeset_id:null,created_at:"2026-01-01T00:00:00Z",operations:ops
});
const c1=changeset("c1",rev0,rev1,[operation("o1",1,"update","f1")]);
const c2=changeset("c2",rev1,rev2,[operation("o2",1,"delete","f2")]);
const rows=[
 {key:source+":f1",layer:baseLayer,feature:{feature_id:"f1",attributes:{name:"old"},geometry_state:"present"},geometry:{type:"Point",coordinates:[105,21]},color:"#000"},
 {key:source+":f2",layer:baseLayer,feature:{feature_id:"f2",attributes:{name:"delete"},geometry_state:"present"},geometry:{type:"Point",coordinates:[106,22]},color:"#000"},
] as unknown as StudioRow[];

function client(history= [c2,c1]){
 const fetchImpl=vi.fn(async(input:RequestInfo|URL)=>{
  const url=String(input),path=new URL(url,"http://fixture").pathname;
  let body:unknown;
  if(path.endsWith("/sources/"+source+"/changesets"))body={items:history};
  else if(path.endsWith("/changesets/c1"))body=c1;
  else if(path.endsWith("/changesets/c2"))body=c2;
  else if(path.endsWith("/layers/"+layerId+"/features/f1"))body={
   context:{organization_id:org,project_id:pid,layer_id:layerId,source_id:source,content_revision:rev2,schema_revision:schema,response_crs:{code:4326}},
   feature:{feature_id:"f1",attributes:{name:"new"},geometry_state:"present",geometry:{type:"Point",coordinates:[107,23],coordinate_reference:{code:4326}}}
  };
  else return new Response("unexpected "+path,{status:500});
  return new Response(JSON.stringify(body),{status:200,headers:{"content-type":"application/json"}});
 });
 return {api:new ApiClient({baseUrl:"/api/v1",fetchImpl}),fetchImpl};
}

describe("incremental reconnect convergence",()=>{
 it("replays every missed revision in order and fetches only surviving changed features",async()=>{
  const {api,fetchImpl}=client();
  const result=await incrementalLayerRows(api,org,pid,baseLayer,nextLayer,rows,"#123",()=>true);
  expect(result).not.toBeNull();
  expect(result).toHaveLength(1);
  expect(result![0]!.feature.feature_id).toBe("f1");
  expect(result![0]!.feature.attributes).toEqual({name:"new"});
  expect(result![0]!.geometry).toEqual({type:"Point",coordinates:[107,23]});
  const calls=fetchImpl.mock.calls.map(([input])=>String(input));
  expect(calls.filter(x=>x.includes("/features/"))).toHaveLength(1);
  expect(calls.some(x=>x.includes("/features/f2"))).toBe(false);
  expect(calls.findIndex(x=>x.includes("/changesets/c1"))).toBeLessThan(calls.findIndex(x=>x.includes("/changesets/c2")));
 });
 it("requires a full reload when the retained revision chain has a gap",async()=>{
  const {api,fetchImpl}=client([c2]);
  await expect(incrementalLayerRows(api,org,pid,baseLayer,nextLayer,rows,"#123",()=>true)).resolves.toBeNull();
  const calls=fetchImpl.mock.calls.map(([input])=>String(input));
  expect(calls.filter(x=>x.includes("/changesets/"))).toHaveLength(0);
  expect(calls.filter(x=>x.includes("/features/"))).toHaveLength(0);
 });
 it("drops a completed replay when the workspace generation is no longer active",async()=>{
  const {api}=client();let active=true;
  const promise=incrementalLayerRows(api,org,pid,baseLayer,nextLayer,rows,"#123",()=>active);
  active=false;
  await expect(promise).resolves.toBeNull();
 });
});
