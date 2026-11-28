import {describe,it,expect} from "vitest";
import {validatePreview,validateGcps,validateGeoref,mapTarget,type Gcp} from "./georeference-contract";
const p=validatePreview({mode:"inspect",persisted:false,width:100,height:100,bands:1,source_sha256:"a".repeat(64),png:"iVBORw0KGgo"},"a".repeat(64));
const points:Gcp[]=[[0,0,106,11],[100,0,106.01,11],[0,100,106,10.99]];
describe("georeferencer contract",()=>{
 it("checks CRS and pixel domains",()=>{
  expect(validateGcps(points,p,4326)).toEqual(points);
  expect(()=>validateGcps([points[0]!,points[0]!,points[2]!],p,4326)).toThrow();
  expect(()=>validateGcps(points,p,32648)).toThrow();
  expect(mapTarget(0,0,3857)[1]).toBeCloseTo(0,5);
 });
 it("pins source digest and rejects inconsistent residuals",()=>{
  const r={...p,mode:"solve",epsg:4326,transform:[106,.0001,0,11,0,-.0001],corners:[[106,11],[106.01,11],[106.01,10.99],[106,10.99]],residuals:[0,0,0],pixel_residuals:[0,0,0],rms:0,pixel_rms:0,redundant:false,sha256:"b".repeat(64),byte_size:4,tiff:"SUkqAA=="};
  expect(validateGeoref(r,p,points,4326).epsg).toBe(4326);
  expect(()=>validateGeoref({...r,rms:1},p,points,4326)).toThrow();
  expect(()=>validateGeoref({...r,source_sha256:"c".repeat(64)},p,points,4326)).toThrow();
 });
});
