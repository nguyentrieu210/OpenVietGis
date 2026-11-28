import {describe,it,expect,vi} from "vitest";
import type {ProcessingJob,SpatialLayer} from "@openvietgis/client-sdk";
import {newModelRun,readModelRun,executeVectorModel,type VectorStep,type ModelRun} from "./model-run-journal";
import type {ProcessingRequest} from "./workbenches/features/platform/processing-request";
const layer={id:"layer",source_id:"source",lifecycle:"active",capabilities:[{name:"feature_read",available:true,actor_authorized:true}],source:{lifecycle:"active",validation_state:"ready",stored_crs:{code:4326},feature_count:1,content_revision:"a".repeat(64),schema_revision:"b".repeat(64),schema:{geometry_family:"Point",fields:[]}}} as unknown as SpatialLayer;
const steps:VectorStep[]=[{id:"first",algorithm:"buffer",name:"B",a:{type:"layer",id:"layer"},b:null,parameters:{distance_meters:10,quad_segs:8}}];
const op="12345678-1234-1234-1234-123456789abc";
function receipt(r:ProcessingRequest):ProcessingJob{return {id:"job",operation_id:r.operation_id,algorithm_id:r.algorithm_id,output_name:r.output_name,status:"succeeded",output_crs:4326,engine_version:"test",attempt_count:1,output_source_id:"out",output_layer_id:"output",feature_count:1,created_at:"2026-11-28T00:00:00Z",completed_at:null,input_refs:{input_source_id:"source",input_content_revision:"a".repeat(64),input_schema_revision:"b".repeat(64)}}}
describe("vector model journal",()=>{
 it("writes exact request before submit, recovers lost response and verifies stored job",async()=>{
  let saved:ModelRun=newModelRun(steps,[layer],op);
  const submit=vi.fn(async(r:ProcessingRequest)=>{expect(saved.entries[0]?.request).toEqual(r);return receipt(r)});
  const io={current:async()=>{},layer:async()=>layer,submit,job:async()=>receipt(saved.entries[0]!.request),persist:(r:ModelRun)=>{saved=r},progress:()=>{},stopped:()=>false,operation:()=>op};
  await expect(executeVectorModel(saved,steps,{...io,submit:async()=>{throw Error("lost")}})).rejects.toThrow("lost");
  expect(readModelRun(JSON.stringify(saved))).toEqual(saved);
  expect((await executeVectorModel(saved,steps,io)).finished).toBe(1);
  expect((await executeVectorModel(saved,steps,io)).finished).toBe(1);
  expect(submit).toHaveBeenCalledTimes(1);
 });
 it("rejects changed source before sending and changed graph",async()=>{
  const run=newModelRun(steps,[layer],op),submit=vi.fn(async(r:ProcessingRequest)=>receipt(r));
  const io={current:async()=>{},layer:async()=>({...layer,source:{...layer.source,content_revision:"c".repeat(64)}}),submit,job:async()=>{throw Error("unexpected")},persist:()=>{},progress:()=>{},stopped:()=>false,operation:()=>op};
  await expect(executeVectorModel(run,steps,io)).rejects.toThrow("Nguồn đã thay đổi");
  await expect(executeVectorModel(run,[{...steps[0]!,name:"Changed"}],io)).rejects.toThrow("Graph");
  expect(submit).not.toHaveBeenCalled();
 });
});
