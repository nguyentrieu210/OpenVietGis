/** Stable layer ordering and minimal structural render changes; no server data. */
export type LayerRange={id:string;min:number;max:number};
export function layerRanges(ids:readonly string[],views:Record<string,{minZoom:number;maxZoom:number}|undefined>):LayerRange[]{
 return ids.map(id=>{const v=views[id];return {id,min:v?.minZoom??0,max:v?.maxZoom??24}});
}
export function layerRangeKey(ranges:readonly LayerRange[]){return JSON.stringify(ranges)}
export function renderPlan(previous:readonly LayerRange[],next:readonly LayerRange[]){
 const old=new Map(previous.map(x=>[x.id,x])),current=new Set(next.map(x=>x.id));
 return {remove:previous.filter(x=>!current.has(x.id)).map(x=>x.id),add:next.filter(x=>!old.has(x.id)),ranges:next.filter(x=>{const p=old.get(x.id);return p&&(p.min!==x.min||p.max!==x.max)}),reorder:previous.map(x=>x.id).join("\0")!==next.map(x=>x.id).join("\0")};
}
export function rankMap(ids:readonly string[]){return new Map(ids.map((id,i)=>[id,i]))}
export function groupByLayer<T extends {layer:{id:string}}>(rows:readonly T[]){
 const groups=new Map<string,T[]>();
 for(const r of rows){const group=groups.get(r.layer.id);if(group)group.push(r);else groups.set(r.layer.id,[r])}
 return groups;
}
export type CameraResume={center:[number,number];zoom:number;bearing:number;pitch:number};
export function readCamera(raw:unknown):CameraResume|null{
 if(!raw||typeof raw!=="object")return null;
 const c=raw as Partial<CameraResume>;
 const inRange=(v:unknown,min:number,max:number):v is number=>typeof v==="number"&&Number.isFinite(v)&&v>=min&&v<=max;
 if(!Array.isArray(c.center)||c.center.length!==2||!inRange(c.center[0],-180,180)||!inRange(c.center[1],-85.05112878,85.05112878)||!inRange(c.zoom,0,22))return null;
 const bearing=c.bearing??0,pitch=c.pitch??0;if(!inRange(bearing,-180,180)||!inRange(pitch,0,60))return null;
 return {center:[c.center[0],c.center[1]],zoom:c.zoom,bearing,pitch};
}
