import type {Geometry} from "geojson";
export type XY=[number,number];
export const validXY=(p:readonly number[])=>p.length===2&&p.every(Number.isFinite)&&Math.abs(p[0]!)<=180&&Math.abs(p[1]!)<=85.05112878;
export function parseXY(raw:string):XY|null{
 const s=raw.trim(),number="[+-]?(?:\\d+(?:\\.\\d+)?|\\.\\d+)",m=new RegExp("^("+number+")(?:\\s*[,;]\\s*|\\s+)("+number+")$").exec(s);
 if(!m)return null;const p:XY=[Number(m[1]),Number(m[2])];return validXY(p)?p:null;
}
export const orthoPoint=(p:XY,last:XY):XY=>Math.abs(p[0]-last[0])*Math.cos(last[1]*Math.PI/180)>=Math.abs(p[1]-last[1])?[p[0],last[1]]:[last[0],p[1]];
export type SnapScope="all"|"active"|"selected";
export function snapLayerAllowed(scope:SnapScope|undefined,id:string,active:string|undefined,selected:readonly string[]|undefined){
 return scope==="active"?!!active&&active===id:scope==="selected"?!!selected?.includes(id):true;
}
const equal=(a:readonly number[],b:readonly number[])=>a[0]===b[0]&&a[1]===b[1];
// Cheap, non-mutating structural preflight; PostGIS remains authoritative for topology.
export function draftError(g:Geometry):string|null{
 const line=(p:number[][],ring:boolean)=>{
  if(p.length<(ring?4:2)||p.some(q=>!validXY(q)))return "Tọa độ hoặc số đỉnh không hợp lệ.";
  if(ring&&!equal(p[0]!,p.at(-1)!))return "Vòng chưa khép kín.";
  if(p.some((q,i)=>i>0&&equal(q,p[i-1]!)))return "Hai đỉnh liên tiếp trùng nhau.";
  if(ring){const unique=new Set(p.slice(0,-1).map(q=>q.join(",")));if(unique.size<3)return "Vùng cần ít nhất ba đỉnh khác nhau.";
   // Local shoelace with translated origin avoids cancellation at large longitudes.
   const o=p[0]!,area=p.slice(1).reduce((sum,q,i)=>sum+(p[i]![0]!-o[0]!)*(q[1]!-o[1]!)-(q[0]!-o[0]!)*(p[i]![1]!-o[1]!),0);
   if(area===0)return "Vùng có diện tích bằng 0 hoặc vòng tự giao."; }
  return null;
 };
 if(g.type==="Point")return validXY(g.coordinates)?null:"Tọa độ ngoài phạm vi bản đồ.";
 if(g.type==="LineString")return line(g.coordinates,false);
 if(g.type==="Polygon"){if(!g.coordinates.length)return "Vùng không có vòng.";for(const r of g.coordinates){const e=line(r,true);if(e)return e}return null}
 return "Kiểu hình học chưa hỗ trợ trong công cụ này.";
}

export function parseSketchPoints(raw:string):XY[]|null{
 const number="[+-]?(?:\\d+(?:\\.\\d+)?|\\.\\d+)",separator="(?:\\s*[,;]\\s*|\\s+)",s=raw.trim();
 if(!new RegExp("^"+number+"(?:"+separator+number+")+$").test(s))return null;
 const parts=s.split(/[ ,;\s]+/).map(Number);if(parts.length%2)return null;
 const points=Array.from({length:parts.length/2},(_,i):XY=>[parts[i*2]!,parts[i*2+1]!]);return points.every(validXY)?points:null;
}
