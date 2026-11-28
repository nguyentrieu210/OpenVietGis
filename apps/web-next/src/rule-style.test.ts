import {describe,it,expect} from "vitest";
import {ruleFor,ruleError,cleanRules,ruleLegend,type StyleRule} from "./rule-style";
const fields=[{field_id:"n",name:"Number",type:"decimal",nullable:true},{field_id:"txt",name:"Text",type:"text",nullable:true}];
const r:StyleRule={id:"a",field:"n",label:"High",op:"gte",value:"10.01",color:"#123456"};
describe("ordered style rules",()=>{
 it("first match wins, decimal comparisons are typed",()=>{
  const fallback={...r,id:"b",op:"not_null" as const};expect(ruleFor({n:"10.010"},[r,fallback],fields)?.id).toBe("a");expect(ruleFor({n:"10.009"},[r,fallback],fields)?.id).toBe("b");expect(ruleFor({n:null},[r,fallback],fields)).toBeUndefined();
 });
 it("validates rule field and operator, preserves null and zero",()=>{
  expect(ruleError({...r,field:"txt"},fields)).toBeTruthy();expect(ruleError({...r,field:"missing"},fields)).toBeTruthy();expect(ruleFor({n:0},[{...r,op:"eq",value:"0"}],fields)?.id).toBe("a");expect(ruleFor({n:null},[{...r,op:"is_null"}],fields)?.id).toBe("a");
 });
 it("sanitizes persisted rule list and counts unmatched",()=>{
  expect(cleanRules([r,r,{...r,id:"else"}])).toEqual([r]);
  expect(ruleLegend([{feature:{attributes:{n:12}}},{feature:{attributes:{n:0}}}],[r],fields,"#000000").map(x=>x.count)).toEqual([1,1]);
 });
});
