import type {StoredRaster} from "./ProjectRaster";
import {validRaster} from "./raster-contract";
import {timestampMs} from "./temporal-plan";
export type RasterTimeEntry={id:string;time:string};
export const RASTER_TEMPORAL_LIMIT=32;
/** Operator-specified observation times; never assume upload time is acquisition time. */
export function rasterTimeline(entries:readonly RasterTimeEntry[],catalog:readonly StoredRaster[]){
 if(!entries.length||entries.length>RASTER_TEMPORAL_LIMIT||new Set(entries.map(e=>e.id)).size!==entries.length)throw Error("Chọn 1–32 raster khác nhau.");
 const frames=entries.map(e=>{const raster=catalog.find(r=>r.id===e.id),time=timestampMs(e.time);if(!validRaster(raster)||time===null)throw Error("Raster không còn khả dụng hoặc timestamp không hợp lệ.");return {raster,time}});
 if(new Set(frames.map(f=>f.time)).size!==frames.length)throw Error("Mỗi khung cần một thời điểm riêng.");
 const first=frames[0]!.raster;
 if(frames.some(({raster:r})=>r.epsg!==first.epsg||r.method!==first.method||(r.output_bands??1)!==(first.output_bands??1)))throw Error("Chuỗi raster phải cùng CRS, phương pháp và số band đầu ra.");
 return frames.sort((a,b)=>a.time-b.time);
}
