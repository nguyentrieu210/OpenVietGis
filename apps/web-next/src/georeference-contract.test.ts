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
  expect(()=>validateGeoref({...r,transform:[106,0,0,11,0,0]},p,points,4326)).toThrow("affine");
  expect(()=>validateGeoref({...r,corners:[[105,11],...r.corners.slice(1)]},p,points,4326)).toThrow("Ranh ảnh");
  expect(()=>validateGeoref({...r,residuals:[1,1,1],rms:1},p,points,4326)).toThrow("Sai số GCP");
  expect(()=>validateGeoref({...r,pixel_residuals:[2,2,2],pixel_rms:2},p,points,4326)).toThrow("Sai số GCP");
  const mercatorPoints:Gcp[]=[[0,0,1000,2000],[100,0,1200,2050],[0,100,1030,1700]];
  const projected={...r,epsg:3857,transform:[1000,2,.3,2000,.5,-3],
   corners:[[1000,2000],[1200,2050],[1230,1750],[1030,1700]].map(([x,y])=>[x!/6378137*180/Math.PI,(2*Math.atan(Math.exp(y!/6378137))-Math.PI/2)*180/Math.PI])};
  expect(validateGeoref(projected,p,mercatorPoints,3857).epsg).toBe(3857);

 });
});
