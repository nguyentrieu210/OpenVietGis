import {describe,it,expect} from "vitest";
import {parseXY,parseSketchPoints,draftError,orthoPoint,snapLayerAllowed} from "./digitizing";
describe("digitizing safeguards",()=>{
 it("strict coordinates do not interpret missing fields as zero",()=>{
  expect(parseXY("106.5, 10.5")).toEqual([106.5,10.5]);expect(parseXY("0 0")).toEqual([0,0]);expect(parseSketchPoints("1,2 3,4")).toEqual([[1,2],[3,4]]);expect(parseSketchPoints("1,,2 3,4")).toBeNull();
  for(const s of ["1,","1,,2","1e2,4","0,90","181,0"])expect(parseXY(s)).toBeNull();
 });
 it("rejects degenerate drafts without changing coordinates",()=>{
  expect(draftError({type:"LineString",coordinates:[[1,2],[1,2]]})).toBeTruthy();
  expect(draftError({type:"Polygon",coordinates:[[[1,1],[2,2],[3,3],[1,1]]]})).toBeTruthy();
  expect(draftError({type:"Polygon",coordinates:[[[1,1],[2,1],[2,2],[1,1]]]})).toBeNull();
 });
 it("explicit snap scopes and ortho",()=>{
  expect(snapLayerAllowed("active","a","b",[])).toBe(false);
  expect(snapLayerAllowed("selected","a","b",["a"])).toBe(true);
  expect(snapLayerAllowed("selected","a","a",[])).toBe(false);
  expect(orthoPoint([2,1.2],[1,1])).toEqual([2,1]);
 });
});
