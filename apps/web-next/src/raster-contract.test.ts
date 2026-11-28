import {describe,it,expect} from "vitest";
import {validRaster,validSlot,rasterDisplay,rasterPaint,storedReceipt} from "./raster-contract";
import type {StoredRaster} from "./ProjectRaster";
const r={id:"12345678-1234-1234-1234-123456789abc",name:"NDVI",method:"ndvi",width:3,height:3,bands:1,output_bands:1,epsg:3857,bounds:[0,0,1,1],maxzoom:12,tile_count:1,byte_size:123,legend:[-1,1],stats:{valid:8,nodata:1,min:.5,max:.5,mean:.5,std:0},inputs:{a:null,b:null,c:null,band_a:1,band_b:2,band_c:3}} as StoredRaster;
describe("raster contract",()=>{
 it("rejects malformed metadata and checks lineage",()=>{
  expect(validRaster(r)).toBe(true);expect(validRaster({...r,stats:{...r.stats,nodata:9}})).toBe(false);expect(validRaster({...r,bounds:[1,0,0,1]})).toBe(false);
  expect(storedReceipt(r,"ndvi","NDVI",[null,null,null],[1,2,3])).toBe(true);expect(storedReceipt(r,"ndvi","NDVI",["foreign",null,null],[1,2,3])).toBe(false);
 });
 it("same-source bands honor stored output band count",()=>{
  const a={mode:r.id,file:null,band:1};expect(validSlot({mode:"same",file:null,band:2},a,[r],100,true)).toBe(false);
  expect(validSlot({mode:"same",file:null,band:1},a,[r],100,true)).toBe(true);
  expect(validSlot({mode:"same",file:null,band:1},a,[r],100,false)).toBe(false);
 });
 it("clamps display preferences without changing data",()=>{
  const p=rasterDisplay({opacity:2,brightness:-.5,contrast:5,resampling:"bad"});expect(p.opacity).toBe(1);expect(p.resampling).toBe("nearest");expect(rasterPaint(p)["raster-brightness-max"]).toBe(.5);expect(p.contrast).toBe(1);
 });
});
