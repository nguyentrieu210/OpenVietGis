import {describe,it,expect} from "vitest";
import {rasterTimeline} from "./raster-temporal-plan";
import type {StoredRaster} from "./ProjectRaster";
const raster=(n:number)=>({id:"00000000-0000-0000-0000-"+String(n).padStart(12,"0"),name:"R"+n,method:"ndvi",width:2,height:2,epsg:4326,bands:1,bounds:[106,10,107,11],maxzoom:10,tile_count:1,byte_size:20,legend:[0,1],stats:{valid:4,nodata:0,min:0,max:1,mean:.5,std:.1}} as StoredRaster);
const a=raster(1),b=raster(2),entries=[{id:b.id,time:"2026-11-29T00:00:00Z"},{id:a.id,time:"2026-11-28T00:00:00Z"}];
describe("raster timeline",()=>{it("orders explicit timestamps, not upload dates",()=>expect(rasterTimeline(entries,[a,b]).map(f=>f.raster.id)).toEqual([a.id,b.id]));it("rejects invalid, missing, duplicate and heterogeneous frames",()=>{expect(()=>rasterTimeline(entries,[a])).toThrow();expect(()=>rasterTimeline(entries,[a,{...b,epsg:3857}])).toThrow();expect(()=>rasterTimeline([entries[0]!,{...entries[1]!,time:entries[0]!.time}],[a,b])).toThrow();expect(()=>rasterTimeline([{...entries[0]!,time:"2026-02-30T00:00:00Z"}],[a,b])).toThrow()})});
