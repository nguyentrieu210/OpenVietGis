import type {StoredRaster} from "./ProjectRaster";
const number=(v:unknown):v is number=>typeof v==="number"&&Number.isFinite(v);
const count=(v:unknown):v is number=>number(v)&&Number.isSafeInteger(v)&&v>=0;
const positive=(v:unknown):v is number=>count(v)&&v>0;
export function boundsValid(b:unknown):b is [number,number,number,number]{
 return Array.isArray(b)&&b.length===4&&b.every(number)&&b[0]>=-180&&b[2]<=180&&b[1]>=-90&&b[3]<=90&&b[0]<b[2]&&b[1]<b[3];
}
export function validStats(s:unknown,total:number){
 if(!s||typeof s!=="object")return false;
 const a=s as Record<string,unknown>;
 return count(a.valid)&&count(a.nodata)&&a.valid+a.nodata===total&&number(a.min)&&number(a.max)&&number(a.mean)&&number(a.std)&&a.min<=a.max&&a.std>=0;
}
export function validRaster(r:unknown):r is StoredRaster{
 if(!r||typeof r!=="object")return false;
 const a=r as StoredRaster;
 return typeof a.id==="string"&&/^[0-9a-f]{8}-(?:[0-9a-f]{4}-){3}[0-9a-f]{12}$/i.test(a.id)&&typeof a.name==="string"&&!!a.name.trim()&&typeof a.method==="string"&&positive(a.width)&&positive(a.height)&&a.width*a.height<=20_000_000&&positive(a.epsg)&&positive(a.bands)&&positive(a.output_bands??1)&&(a.output_bands??1)<=16&&boundsValid(a.bounds)&&count(a.maxzoom)&&a.maxzoom<=20&&count(a.tile_count)&&count(a.byte_size)&&Array.isArray(a.legend)&&a.legend.length===2&&a.legend.every(number)&&a.legend[0]<=a.legend[1]&&validStats(a.stats,a.width*a.height)&&(!a.channels||Array.isArray(a.channels)&&a.channels.every(s=>validStats(s,a.width*a.height)));
}
export type RasterSlot={mode:string;file:File|null;band:number};
export function validSlot(slot:RasterSlot,primary:RasterSlot,catalog:StoredRaster[],limit:number,allowSame:boolean){
 const actual=slot.mode==="same"?primary:slot;
 if(slot.mode==="same"&&!allowSame||actual.mode==="same")return false;
 const stored=catalog.find(r=>r.id===actual.mode);
 return positive(slot.band)&&slot.band<=(stored?.output_bands??(stored?1:16))&&(actual.mode==="upload"?!!actual.file&&actual.file.size>0&&actual.file.size<=limit&&/\.tiff?$/i.test(actual.file.name):!!stored);
}
export type RasterDisplay={visible:boolean;opacity:number;resampling:"nearest"|"linear";brightness:number;contrast:number;saturation:number};
export const defaultRasterDisplay:RasterDisplay={visible:true,opacity:.85,resampling:"nearest",brightness:0,contrast:0,saturation:0};
const bounded=(v:unknown,min:number,max:number,fallback:number)=>number(v)?Math.max(min,Math.min(max,v)):fallback;
export function rasterDisplay(v:unknown):RasterDisplay{
 const p=v&&typeof v==="object"?v as Partial<RasterDisplay>:{};
 return {visible:typeof p.visible==="boolean"?p.visible:true,opacity:bounded(p.opacity,0,1,.85),resampling:p.resampling==="linear"?"linear":"nearest",brightness:bounded(p.brightness,-1,1,0),contrast:bounded(p.contrast,-1,1,0),saturation:bounded(p.saturation,-1,1,0)};
}
export function rasterPaint(p:RasterDisplay){return {"raster-opacity":p.opacity,"raster-resampling":p.resampling,"raster-brightness-min":Math.max(0,p.brightness),"raster-brightness-max":Math.min(1,1+p.brightness),"raster-contrast":p.contrast,"raster-saturation":p.saturation} as const}
export function storedReceipt(r:unknown,method:string,name:string,ids:(string|null)[],bands:number[]):r is StoredRaster{
 if(!validRaster(r)||r.method!==method||r.name!==name||!r.inputs)return false;
 return [r.inputs.a??null,r.inputs.b??null,r.inputs.c??null].every((id,i)=>id===ids[i])&&[r.inputs.band_a,r.inputs.band_b,r.inputs.band_c].every((band,i)=>band===bands[i]);
}
