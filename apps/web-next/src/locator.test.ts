import {describe,it,expect} from "vitest";
import {parseLocator,locatorMatches,formatDms} from "./locator";
describe("locator",()=>{
 it("explicit sources and accented multiword matching",()=>{
  expect(parseLocator("lớp: Đường")).toEqual({kind:"layer",term:"Đường"});
  expect(parseLocator("feature: B1")).toEqual({kind:"feature",term:"B1"});
  expect(locatorMatches("Đường giao thông Bình Hưng","duong hung")).toBe(true);
 });
 it("requires explicit coordinates and validates axis and map bounds",()=>{
  expect(parseLocator("xy: 106.5, 10.6")).toEqual({kind:"coordinate",point:[106.5,10.6]});
  expect(parseLocator("106.5, 10.6").kind).toBe("all");
  for(const q of ["xy: 10.6,106.5","xy: 181,10","xy: 1,90","xy: 1e2,5","xy: ,"])expect(parseLocator(q).kind).toBe("invalid");
 });
 it("carries seconds and minutes on rounding",()=>{
  expect(formatDms(12.9999999,"E","W")).toBe("13°00′00.00″E");
  expect(formatDms(-0.5,"N","S")).toBe("0°30′00.00″S");
 });
});
