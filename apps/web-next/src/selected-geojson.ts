import type {SpatialFeaturePage,SpatialLayer} from "@openvietgis/client-sdk";
import type {Geometry} from "geojson";
import {verifyFeaturePage} from "./spatial-snapshot-guard";
import {QUERY_SELECTION_LIMIT,SELECTED_EXPORT_MAX_SOURCE} from "./query-selection";
export const GEOJSON_PAGE_LIMIT=100;
export const GEOJSON_EXPORT_LIMIT=32*1024*1024;
const position=(p:unknown):p is number[]=>Array.isArray(p)&&p.length===2&&p.every(n=>typeof n==="number"&&Number.isFinite(n))&&Math.abs(p[0]!)<=180&&Math.abs(p[1]!)<=90;
function geometry(raw:unknown):Geometry{
 if(!raw||typeof raw!=="object")throw Error("Thiếu hình học.");
 const g=raw as {type:unknown;coordinates:unknown};
 let positions=0;
 const pos=(p:unknown)=>position(p)&&++positions<=200000;
 const line=(p:unknown)=>Array.isArray(p)&&p.length>=2&&p.every(pos);
 const ring=(p:unknown)=>Array.isArray(p)&&p.length>=4&&p.every(pos)&&p[0][0]===p.at(-1)[0]&&p[0][1]===p.at(-1)[1];
 const polygon=(p:unknown)=>Array.isArray(p)&&p.length>0&&p.every(ring);
 const many=(p:unknown,check:(v:unknown)=>boolean)=>Array.isArray(p)&&p.length>0&&p.every(check);
 const c=g.coordinates,ok=g.type==="Point"?pos(c):g.type==="MultiPoint"?many(c,pos):g.type==="LineString"?line(c):g.type==="MultiLineString"?many(c,line):g.type==="Polygon"?polygon(c):g.type==="MultiPolygon"?many(c,polygon):false;
 if(!ok)throw Error("Hình học XY không hợp lệ hoặc vượt giới hạn.");
 return {type:g.type,coordinates:c} as Geometry;
}
/** Complete, bounded, revision-pinned GeoJSON selection; no map hydration needed. */
export async function exportSelectedGeoJSON(layer:SpatialLayer,org:string,project:string,selected:ReadonlySet<string>,load:(cursor:string|null)=>Promise<SpatialFeaturePage>,current:()=>boolean,progress:(n:number,total:number)=>void){
 const prefix=layer.source_id+":",wanted=new Set([...selected].filter(k=>k.startsWith(prefix)).map(k=>k.slice(prefix.length))),expected=layer.source.feature_count;
 if(!wanted.size||wanted.size>QUERY_SELECTION_LIMIT||wanted.has("")||expected===null||!Number.isSafeInteger(expected)||expected<wanted.size||expected>SELECTED_EXPORT_MAX_SOURCE)throw Error("Vùng chọn hoặc nguồn vượt giới hạn xuất GeoJSON.");
 const seen=new Set<string>(),cursors=new Set<string>(),records:string[]=[],encoder=new TextEncoder();
 const header='{"type":"FeatureCollection","features":[';let bytes=encoder.encode(header).length+2,missing=0,cursor:string|null=null;
 do{
  if(!current())throw Error("Nguồn, quyền hoặc vùng chọn đã đổi.");
  const page=await load(cursor);if(!current())throw Error("Nguồn, quyền hoặc vùng chọn đã đổi.");
  const ids=verifyFeaturePage(page,layer,org,project,GEOJSON_PAGE_LIMIT,seen,cursors);
  for(const f of page.items){
   if(!wanted.has(f.feature_id))continue;
   const g=f.geometry_state==="present"?geometry(f.geometry):null;
   if(g===null)missing++;
   const record=JSON.stringify({type:"Feature",id:f.feature_id,geometry:g,properties:f.attributes,ovg_geometry_state:f.geometry_state});
   bytes+=encoder.encode(record).length+1;if(bytes>GEOJSON_EXPORT_LIMIT)throw Error("GeoJSON vượt 32 MiB.");
   records.push(record);wanted.delete(f.feature_id);
  }
  for(const id of ids)seen.add(id);
  cursor=page.next_after_feature_id;if(cursor)cursors.add(cursor);
  progress(seen.size,expected);
 }while(cursor&&wanted.size);
 if(!current()||wanted.size)throw Error("Không xác minh đủ FID đã chọn ở đúng revision.");
 return {json:header+records.join(",")+']}',count:records.length,missing};
}
