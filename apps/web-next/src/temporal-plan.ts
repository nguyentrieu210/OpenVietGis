import type {SpatialLayer} from "@openvietgis/client-sdk";
import type {StudioRow} from "./studio-contract";
import type {FeatureCollection} from "geojson";
export const TEMPORAL_LIMIT=5000;
export const DAY_MS=86400000;
export function timestampMs(raw:unknown):number|null{
 if(typeof raw!=="string")return null;
 const m=/^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})(?:\.\d{1,9})?(Z|[+-]\d{2}:\d{2})$/.exec(raw);
 if(!m)return null;
 const y=Number(m[1]),mo=Number(m[2]),d=Number(m[3]),h=Number(m[4]),mi=Number(m[5]),s=Number(m[6]);
 const leap=y%4===0&&(y%100!==0||y%400===0),days=[31,leap?29:28,31,30,31,30,31,31,30,31,30,31];
 if(y<1||mo<1||mo>12||d<1||d>days[mo-1]!||h>23||mi>59||s>59)return null;
 const zone=m[7]!;
 if(zone!=="Z"&&(Number(zone.slice(1,3))>23||Number(zone.slice(4,6))>59))return null;
 const t=Date.parse(raw);return Number.isFinite(t)?t:null;
}
export const temporalFields=(layer:SpatialLayer)=>layer.source.schema.fields.filter(f=>f.type==="timestamp");
export function temporalReadable(l:SpatialLayer){return l.lifecycle==="active"&&l.capabilities.some(c=>c.name==="feature_read"&&c.available&&c.actor_authorized&&c.technical_supported)}
function validatedTemporalRows(layer:SpatialLayer,rows:readonly StudioRow[],field:string){
 if(!temporalReadable(layer)||!temporalFields(layer).some(f=>f.field_id===field))throw Error("Thiếu quyền đọc hoặc trường thời gian không hợp lệ.");
 const expected=layer.source.feature_count;
 if(expected===null||!Number.isSafeInteger(expected)||expected<0||expected>TEMPORAL_LIMIT)throw Error("Hỗ trợ lớp đã xác minh tối đa 5.000 đối tượng.");
 const all=rows.filter(r=>r.layer.id===layer.id);
 if(all.length!==expected||new Set(all.map(r=>r.feature.feature_id)).size!==expected||new Set(all.map(r=>r.key)).size!==expected||all.some(r=>!r.key||!r.feature.feature_id||r.layer.source_id!==layer.source_id||r.layer.source.content_revision!==layer.source.content_revision||r.layer.source.schema_revision!==layer.source.schema_revision))throw Error("Chưa tải đầy đủ lớp ở đúng phiên bản.");
 return all;
}
export function temporalPopulation(layer:SpatialLayer,rows:readonly StudioRow[],field:string){
 const all=validatedTemporalRows(layer,rows,field);
 const byDay=new Map<number,Set<string>>();let invalid=0;
 for(const row of all){const ms=timestampMs(row.feature.attributes[field]);if(ms===null){invalid++;continue}const day=Math.floor(ms/DAY_MS)*DAY_MS;const ids=byDay.get(day)??new Set<string>();ids.add(layer.id+":"+row.key);byDay.set(day,ids)}
 return {days:[...byDay.keys()].sort((a,b)=>a-b),byDay,invalid,total:all.length};
}
export function temporalCollection(collection:FeatureCollection,ids:ReadonlySet<string>):FeatureCollection{
 return {type:"FeatureCollection",features:collection.features.filter(f=>typeof f.id==="string"&&ids.has(f.id)).map(f=>({...f,properties:{...f.properties,selected:false,selectable:false}}))};
}

export const TEMPORAL_FRAME_LIMIT=5000;
export type TemporalWindow={start:number;end:number;stepMs:number};
export function temporalWindowPopulation(layer:SpatialLayer,rows:readonly StudioRow[],field:string,endField:string,window:TemporalWindow){
 const all=validatedTemporalRows(layer,rows,field);
 if(endField&&!temporalFields(layer).some(f=>f.field_id===endField))throw Error("Trường kết thúc không hợp lệ.");
 const {start,end,stepMs}=window;
 if(![start,end,stepMs].every(Number.isSafeInteger)||start>=end||stepMs<60000||stepMs>366*DAY_MS||Math.abs(start)>8640000000000000||Math.abs(end)>8640000000000000)throw Error("Khoảng UTC hoặc bước thời gian không hợp lệ (1 phút–366 ngày).");
 const count=Math.ceil((end-start)/stepMs);
 if(!Number.isSafeInteger(count)||count>TEMPORAL_FRAME_LIMIT)throw Error("Tối đa 5.000 khung thời gian.");
 const entries:{id:string;start:number;end:number;instant:boolean}[]=[];let invalid=0;
 for(const row of all){
  const a=timestampMs(row.feature.attributes[field]),b=endField?timestampMs(row.feature.attributes[endField]):a;
  if(a===null||b===null||b<a){invalid++;continue}
  entries.push({id:layer.id+":"+row.key,start:a,end:b,instant:a===b});
 }
 const frames=Array.from({length:count},(_,i)=>start+i*stepMs);
 return {frames,invalid,total:all.length,idsAt:(frame:number)=>{
  if(!Number.isSafeInteger(frame)||frame<0||frame>=count)return new Set<string>();
  const a=frames[frame]!,b=Math.min(end,a+stepMs);
  return new Set(entries.filter(e=>e.instant?e.start>=a&&e.start<b:e.start<b&&e.end>a).map(e=>e.id));
 }};
}
