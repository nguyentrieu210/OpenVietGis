import {describe,it,expect} from "vitest";
import {calculateExact,summarizeAttributes} from "./precise-calculation";
describe("exact canonical calculator",()=>{
 it("preserves 38-digit decimals and rounds only the final value",()=>{
  expect(calculateExact("[a] + 0.000000000000000001",{a:"12345678901234567890.123456789012345678"},new Set(["a"]),"decimal")).toBe("12345678901234567890.123456789012345679");
  expect(calculateExact("0.1 + 0.2",{},new Set(),"decimal")).toBe("0.3");
  expect(calculateExact("-1 / 6",{},new Set(),"decimal")).toBe("-0.166666666666666667");
  expect(calculateExact("100",{},new Set(),"decimal")).toBe("100");
 });
 it("rejects fractional integers, text fields and zero division",()=>{
  expect(()=>calculateExact("1/2",{},new Set(),"integer")).toThrow();
  expect(()=>calculateExact("[a]",{a:"10"},new Set(),"decimal")).toThrow();
  expect(()=>calculateExact("1/0",{},new Set(),"decimal")).toThrow();
 });
 it("separates null from empty group and sums exactly",()=>{
  const rows=[{feature_id:"a",attributes:{g:null,n:"0.1"}},{feature_id:"b",attributes:{g:"",n:null}},{feature_id:"c",attributes:{g:null,n:"0.2"}}];
  const result=summarizeAttributes(rows,"n","g",true);expect(result[0]).toMatchObject({label:"(NULL)",count:2,sum:"0.3",mean:"0.15"});expect(result[1]).toMatchObject({label:"(Chuỗi rỗng)",missing:1,sum:null});
 });
});
