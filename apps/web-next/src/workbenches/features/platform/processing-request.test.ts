import {describe,it,expect} from "vitest";
import type {SpatialLayer,ProcessingJob} from "@openvietgis/client-sdk";
import {processingRequest,processingLayer,readPendingProcessing,verifyProcessingReceipt,supportedAlgorithm,reprojectTargets,type Algorithm} from "./processing-request";
const layer=(id:string,family="Point")=>({id,source_id:"source-"+id,lifecycle:"active",capabilities:[{name:"feature_read",available:true,actor_authorized:true}],source:{lifecycle:"active",validation_state:"ready",stored_crs:{code:4326},feature_count:12,content_revision:"a".repeat(64),schema_revision:"b".repeat(64),schema:{geometry_family:family,fields:[{field_id:"zone",name:"Zone"}]}}}) as unknown as SpatialLayer;
const op="12345678-1234-1234-1234-123456789abc";
const build=(algo:Algorithm,a=layer("a"),b?:SpatialLayer,d="100",s="8",g="")=>processingRequest(algo,a,b,d,s,g,"Output",op);
describe("processing job request",()=>{
 it("uses authorized ready layers and supported registry IDs",()=>{
  expect(processingLayer(layer("a"))).toBe(true);
  expect(processingLayer({...layer("a"),capabilities:[]})).toBe(false);
  expect(supportedAlgorithm("union")).toBe(true);
  expect(supportedAlgorithm("unimplemented")).toBe(false);
  expect(()=>processingRequest("buffer",undefined,undefined,"100","8","","Output",op)).toThrow();
 });
 it("validates buffer, polygon clip and schema dissolve",()=>{
  expect(build("buffer").parameters).toEqual({distance_meters:100,quad_segs:8});
  expect(()=>build("buffer",layer("a"),undefined,"","8")).toThrow();
  expect(()=>build("buffer",layer("a"),undefined,"1","2.5")).toThrow();
  expect(()=>build("clip",layer("a"),layer("b"))).toThrow();
  expect(build("clip",layer("a"),layer("b","Polygon")).overlay_layer_id).toBe("b");
  expect(()=>build("clip",layer("a"),{...layer("b","Polygon"),source_id:"source-a"})).toThrow();
  expect(build("dissolve",layer("a"),undefined,"","8","zone").parameters).toEqual({group_by_field:"zone"});
  expect(()=>build("dissolve",layer("a"),undefined,"","8","missing")).toThrow();
 });
 it("recovers exact pending operation, rejects corrupt receipts",()=>{
  const request=build("buffer");
  expect(readPendingProcessing(JSON.stringify(request))).toEqual(request);
  expect(readPendingProcessing(JSON.stringify({...request,parameters:{distance_meters:0,quad_segs:8}}))).toBeNull();
  expect(readPendingProcessing(JSON.stringify({...request,algorithm_id:"union"}))).toBeNull();
  const job={id:"job1",operation_id:op,algorithm_id:"buffer",output_name:"Output",status:"queued",output_crs:4326,engine_version:"PostGIS/test",attempt_count:1,output_source_id:null,output_layer_id:null,feature_count:null,created_at:"2026-11-28T00:00:00Z",completed_at:null,input_refs:{input_source_id:"source-a",input_content_revision:"a".repeat(64),input_schema_revision:"b".repeat(64)}} satisfies ProcessingJob;
  expect(verifyProcessingReceipt(job,request)).toBe(job);
  expect(()=>verifyProcessingReceipt({...job,input_refs:{}},request)).toThrow();
  expect(readPendingProcessing(JSON.stringify({...request,expected_input:{...request.expected_input,content_revision:"stale"}}))).toBeNull();
  expect(()=>verifyProcessingReceipt({...job,operation_id:"other"},request)).toThrow();
 });
});

it("requires both source pins for Difference and Intersection",()=>{
 for(const algo of ["difference","intersection"] as const){
  expect(()=>build(algo)).toThrow();
  const r=build(algo,layer("a"),layer("b","Polygon"));
  expect(r.expected_input?.source_id).toBe("source-a");
  expect(r.expected_overlay?.source_id).toBe("source-b");
  expect(r.parameters).toEqual({});
  expect(readPendingProcessing(JSON.stringify(r))).toEqual(r);
  expect(readPendingProcessing(JSON.stringify({...r,expected_overlay:undefined}))).toBeNull();
 }
});

it("validates and restores union, reproject and spatial join options",()=>{
 const a=layer("a","Polygon"), b=layer("b","Polygon");
 const u=build("union",a,b);expect(readPendingProcessing(JSON.stringify(u))).toEqual(u);
 expect(()=>build("union",layer("a"),b)).toThrow();
 const r=processingRequest("reproject",a,undefined,"","","","Projected",op,{targetSrid:3857});
 expect(readPendingProcessing(JSON.stringify(r))).toEqual(r);
 expect(()=>processingRequest("reproject",a,undefined,"","","","Projected",op,{targetSrid:3405})).toThrow();
 const j=processingRequest("spatial_join",a,layer("b"),"","","","Joined",op,{predicate:"contains",joinType:"inner",matchMode:"first"});
 expect(j.parameters).toEqual({predicate:"contains",join_type:"inner",match_mode:"first"});
 expect(readPendingProcessing(JSON.stringify(j))).toEqual(j);
 expect(readPendingProcessing(JSON.stringify({...j,parameters:{...j.parameters,predicate:"sql"}}))).toBeNull();
});

it("rejects altered persisted parameters and mismatched reprojection receipt",()=>{
 const a=layer("a","Polygon"), b=layer("b","Polygon");
 const u=build("union",a,b);
 expect(readPendingProcessing(JSON.stringify({...u,parameters:{unsupported:true}}))).toBeNull();
 expect(readPendingProcessing(JSON.stringify({...u,expected_overlay:u.expected_input}))).toBeNull();
 const r=processingRequest("reproject",a,undefined,"","","","Projected",op,{targetSrid:3857});
 const job={id:"job",operation_id:op,algorithm_id:"reproject",output_name:"Projected",status:"queued",output_crs:3857,engine_version:"PostGIS/test",attempt_count:1,output_source_id:null,output_layer_id:null,feature_count:null,created_at:"2026-11-28T00:00:00Z",completed_at:null,input_refs:{input_source_id:r.expected_input!.source_id,input_content_revision:r.expected_input!.content_revision,input_schema_revision:r.expected_input!.schema_revision}} as ProcessingJob;
 expect(verifyProcessingReceipt(job,r)).toBe(job);
 expect(()=>verifyProcessingReceipt({...job,output_crs:4326},r)).toThrow();
});

it("supports native VN-2000 projection targets but not unsupported datum transitions",()=>{
 const a=layer("a");const native={...a,source:{...a.source,stored_crs:{...a.source.stored_crs!,code:4756}}};
 expect(reprojectTargets(4756)).toContain(3405);expect(reprojectTargets(4756)).toContain(9218);
 const req=processingRequest("reproject",native,undefined,"","","","Projected",op,{targetSrid:3405});
 expect(readPendingProcessing(JSON.stringify(req))).toEqual(req);
 const projected={...native,source:{...native.source,stored_crs:{...native.source.stored_crs!,code:3405}}};
 expect(reprojectTargets(3405)).toContain(4326); // server still requires verified regional provenance
 expect(()=>processingRequest("buffer",projected,undefined,"100","8","","Buffer",op)).toThrow();
});
