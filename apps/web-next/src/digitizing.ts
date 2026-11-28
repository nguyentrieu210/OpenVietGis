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
// Bounded planar ring check for small, local drafts; backend still validates all topology.
function ringCrosses(p:number[][]){
 const n=p.length-1;if(n>512||Math.max(...p.map(v=>v[0]!))-Math.min(...p.map(v=>v[0]!))>180)return false;
 const orient=(a:number[],b:number[],c:number[])=>(b[0]!-a[0]!)*(c[1]!-a[1]!)-(b[1]!-a[1]!)*(c[0]!-a[0]!);
 const on=(a:number[],b:number[],c:number[])=>Math.min(a[0]!,b[0]!)<=c[0]!&&c[0]!<=Math.max(a[0]!,b[0]!)&&Math.min(a[1]!,b[1]!)<=c[1]!&&c[1]!<=Math.max(a[1]!,b[1]!);
 for(let i=0;i<n;i++)for(let j=i+2;j<n;j++){
  if(i===0&&j===n-1)continue;
  const a=p[i]!,b=p[i+1]!,c=p[j]!,d=p[j+1]!,x=orient(a,b,c),y=orient(a,b,d),z=orient(c,d,a),w=orient(c,d,b);
  if((x*y<0&&z*w<0)||(x===0&&on(a,b,c))||(y===0&&on(a,b,d))||(z===0&&on(c,d,a))||(w===0&&on(c,d,b)))return true;
 }
 return false;
}

// Cheap, non-mutating structural preflight; PostGIS remains authoritative for topology.
export function draftError(g:Geometry):string|null{
 const line=(p:number[][],ring:boolean)=>{
  if(p.length<(ring?4:2)||p.some(q=>!validXY(q)))return "Tọa độ hoặc số đỉnh không hợp lệ.";
  if(ring&&!equal(p[0]!,p.at(-1)!))return "Vòng chưa khép kín.";
  if(p.some((q,i)=>i>0&&equal(q,p[i-1]!)))return "Hai đỉnh liên tiếp trùng nhau.";
  if(ring){const unique=new Set(p.slice(0,-1).map(q=>q.join(",")));if(unique.size<3)return "Vùng cần ít nhất ba đỉnh khác nhau.";if(unique.size!==p.length-1||ringCrosses(p))return "Vòng có đỉnh trùng hoặc tự giao.";
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

export type SegmentLock={distance:string;bearing:string};
const R=6371008.8,rad=Math.PI/180;
export function sphericalInverse(a:XY,b:XY){
 const p=a[1]*rad,q=b[1]*rad,d=(b[0]-a[0])*rad;
 const h=Math.sin((q-p)/2)**2+Math.cos(p)*Math.cos(q)*Math.sin(d/2)**2;
 const distance=2*R*Math.asin(Math.sqrt(Math.min(1,Math.max(0,h))));
 const bearing=distance>1e-7?(Math.atan2(Math.sin(d)*Math.cos(q),Math.cos(p)*Math.sin(q)-Math.sin(p)*Math.cos(q)*Math.cos(d))/rad+360)%360:null;
 return {distance,bearing};
}
export function lockedSegment(origin:XY,target:XY,lock:SegmentLock):XY{
 if(!validXY(origin)||!validXY(target))throw Error("Tọa độ ngoài phạm vi bản đồ.");
 const raw=sphericalInverse(origin,target);
 const parse=(s:string,min:number,max:number)=>{if(!/^\d+(?:\.\d+)?$/.test(s.trim()))throw Error("Nhập số hợp lệ bằng dấu chấm.");const n=Number(s);if(!Number.isFinite(n)||n<min||n>max)throw Error("Giá trị khóa ngoài giới hạn.");return n};
 const distance=lock.distance.trim()?parse(lock.distance,0.001,1000000):raw.distance;
 const bearing=lock.bearing.trim()?parse(lock.bearing,0,359.999999):raw.bearing;
 if(!distance||bearing===null)throw Error("Cần hướng hoặc khoảng cách khác 0.");
 const p=origin[1]*rad,l=origin[0]*rad,t=bearing*rad,d=distance/R;
 const q=Math.asin(Math.sin(p)*Math.cos(d)+Math.cos(p)*Math.sin(d)*Math.cos(t));
 const x=l+Math.atan2(Math.sin(t)*Math.sin(d)*Math.cos(p),Math.cos(d)-Math.sin(p)*Math.sin(q));
 const result:XY=[((x/rad+540)%360)-180,q/rad];
 if(!validXY(result))throw Error("Điểm kết quả ngoài phạm vi bản đồ.");
 return result;
}
export function hasSegmentLock(lock:SegmentLock){return !!(lock.distance.trim()||lock.bearing.trim())}
