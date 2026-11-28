import {describe,it,expect} from "vitest";
import type {SpatialLayer,ProcessingJob} from "@openvietgis/client-sdk";
import {processingRequest,processingLayer,readPendingProcessing,verifyProcessingReceipt,supportedAlgorithm} from "./processing-request";
const layer=(id:string,family="Point")=>({id,source_id:"source-"+id,lifecycle:"active",capabilities:[{name:"feature_read",available:true,actor_authorized:true}],source:{lifecycle:"active",validation_state:"ready",stored_crs:{code:4326},feature_count:12,content_revision:"a".repeat(64),schema_revision:"b".repeat(64),schema:{geometry_family:family,fields:[{field_id:"zone",name:"Zone"}]}}}) as unknown as SpatialLayer;
const op="12345678-1234-1234-1234-123456789abc";
const build=(algo:"buffer"|"clip"|"dissolve",a=layer("a"),b?:SpatialLayer,d="100",s="8",g="")=>processingRequest(algo,a,b,d,s,g,"Output",op);
describe("processing job request",()=>{
 it("uses authorized ready layers and supported registry IDs",()=>{
  expect(processingLayer(layer("a"))).toBe(true);
  expect(processingLayer({...layer("a"),capabilities:[]})).toBe(false);
  expect(supportedAlgorithm("union")).toBe(false);
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
  const job={id:"job1",operation_id:op,algorithm_id:"buffer",output_name:"Output",status:"queued",input_refs:{input_source_id:"source-a",input_content_revision:"a".repeat(64),input_schema_revision:"b".repeat(64)}} as ProcessingJob;
  expect(verifyProcessingReceipt(job,request)).toBe(job);
  expect(()=>verifyProcessingReceipt({...job,input_refs:{}},request)).toThrow();
  expect(readPendingProcessing(JSON.stringify({...request,expected_input:{...request.expected_input,content_revision:"stale"}}))).toBeNull();
  expect(()=>verifyProcessingReceipt({...job,operation_id:"other"},request)).toThrow();
 });
});
