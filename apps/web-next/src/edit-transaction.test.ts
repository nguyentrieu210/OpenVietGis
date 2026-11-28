import {describe,it,expect} from "vitest";
import type {Changeset,SpatialLayer} from "@openvietgis/client-sdk";
import {sameEditPin,verifyEditReceipt,definitiveFirstRejection,type EditBody} from "./edit-transaction";
const l={id:"l",source_id:"s",lifecycle:"active",source:{content_revision:"c",schema_revision:"s"},capabilities:[{name:"edit",available:true,actor_authorized:true}]} as unknown as SpatialLayer;
const body:EditBody={base_content_revision:"c",base_schema_revision:"s",operation_id:"op",summary:"create",operations:[{op_type:"create",feature_id:"f"}]};
const r={id:"r",source_id:"s",operation_id:"op",base_content_revision:"c",status:"committed",operation_count:1,resulting_content_revision:"d"} as Changeset;
describe("edit transaction preflight",()=>{
 it("requires current source and revision with edit authorization",()=>{
  expect(sameEditPin(l,l)).toBe(true);
  expect(sameEditPin({...l,capabilities:[]},l)).toBe(false);
  expect(sameEditPin({...l,source_id:"foreign"},l)).toBe(false);
  expect(sameEditPin({...l,source:{...l.source,schema_revision:"new"}},l)).toBe(false);
 });
 it("never discards ambiguous request or accepts mismatched receipt",()=>{
  expect(verifyEditReceipt(r,"s",body)).toBe(r);
  for(const p of [{status:"failed"},{operation_count:2},{base_content_revision:"other"},{source_id:"foreign"}])expect(()=>verifyEditReceipt({...r,...p},"s",body)).toThrow();
  expect(definitiveFirstRejection(409,false)).toBe(true);
  expect(definitiveFirstRejection(409,true)).toBe(false);
  expect(definitiveFirstRejection(429,false)).toBe(false);
  expect(definitiveFirstRejection(503,false)).toBe(false);
 });
});
