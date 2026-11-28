import {describe,it,expect} from "vitest";
import {attributeValue} from "./attribute-value";
const field=(type:string,nullable=false)=>({name:"value",type,nullable});
describe("typed attribute editing",()=>{
 it("exact decimal bounds and canonicalization",()=>{
  expect(attributeValue(field("decimal"),"+00012.3400")).toBe("12.34");
  expect(attributeValue(field("decimal"),"-0.0")).toBe("0");
  expect(attributeValue(field("decimal"),"12345678901234567890.123456789012345678")).toBe("12345678901234567890.123456789012345678");
  expect(()=>attributeValue(field("decimal"),"1.1234567890123456789")).toThrow();
  expect(()=>attributeValue(field("decimal"),"1e10")).toThrow();
 });
 it("integer boolean and null",()=>{
  expect(attributeValue(field("integer"),"2147483647")).toBe(2147483647);
  expect(()=>attributeValue(field("integer"),"2147483648")).toThrow();
  expect(attributeValue(field("boolean"),"false")).toBe(false);
  expect(()=>attributeValue(field("boolean"),"0")).toThrow();
  expect(attributeValue(field("text",true),"",true)).toBeNull();
  expect(()=>attributeValue(field("text"),"",true)).toThrow();
  expect(attributeValue(field("text"),"")).toBe("");
 });
 it("RFC3339 calendar validation",()=>{
  expect(attributeValue(field("timestamp"),"2024-02-29T12:00:00+07:00")).toBe("2024-02-29T12:00:00+07:00");
  for(const v of ["2025-02-29T12:00:00Z","2026-04-31T12:00:00Z","2026-01-01T24:00:00Z","2026-01-01T12:00:00"])expect(()=>attributeValue(field("timestamp"),v)).toThrow();
 });
});
