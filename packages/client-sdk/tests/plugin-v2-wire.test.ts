import {describe,it,expect} from "vitest";
import {buildPluginV2Invocation,parsePluginV2Runtime} from "../src/plugin-v2-contract";
import vectors from "../../../fixtures/spec075/runtime-wire-vectors.json";
describe("shared SPEC075 host wire vectors",()=>{
 for(const c of vectors.valid)it("builds "+c.name,()=>{
  const r=parsePluginV2Runtime(c.runtime,c.manifest);
  expect(buildPluginV2Invocation(r,c.action,c.input,c.request_id)).toEqual(c.expected);
  expect(()=>buildPluginV2Invocation(r,"not-an-action",{},c.request_id)).toThrow();
  expect(()=>buildPluginV2Invocation(r,c.action,{...c.input,extra:"x"},c.request_id)).toThrow();
  expect(()=>buildPluginV2Invocation(r,c.action,c.input,"not-a-uuid")).toThrow();
  expect(()=>buildPluginV2Invocation({...r,granted:false},c.action,c.input,c.request_id)).toThrow();
 });
 for(const c of vectors.invalid)it("rejects "+c.name,()=>expect(()=>parsePluginV2Runtime(c.runtime,c.manifest)).toThrow());
});
