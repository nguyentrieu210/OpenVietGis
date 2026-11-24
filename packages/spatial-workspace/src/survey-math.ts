export type XY=[number,number];
export type Leg={distance:number;azimuth:number};
const finite=(n:number)=>Number.isFinite(n)&&Math.abs(n)<=1e8;
export const validPoint=(p:XY)=>p.every(finite);
const deg=(n:number)=>n*180/Math.PI;
export function traverse(start:XY,legs:Leg[]){
 if(!validPoint(start)||!legs.length||legs.length>500)throw Error("Tọa độ hoặc số cạnh không hợp lệ.");
 let p=start,total=0;return legs.map((leg,i)=>{if(!Number.isFinite(leg.distance)||leg.distance<=0||leg.distance>1e6||!Number.isFinite(leg.azimuth)||leg.azimuth<0||leg.azimuth>=360)throw Error("Cạnh "+(i+1)+": khoảng cách dương, phương vị 0–<360°.");const r=leg.azimuth*Math.PI/180;const dx=leg.distance*Math.sin(r),dy=leg.distance*Math.cos(r);p=[p[0]+dx,p[1]+dy];if(!validPoint(p))throw Error("Kết quả ngoài giới hạn.");total+=leg.distance;return {i:i+1,point:p,dx,dy,total,distance:leg.distance,azimuth:leg.azimuth}})
}
export function inverse(a:XY,b:XY){if(!validPoint(a)||!validPoint(b))throw Error("Tọa độ không hợp lệ");const dx=b[0]-a[0],dy=b[1]-a[1],distance=Math.hypot(dx,dy);return {dx,dy,distance,azimuth:distance?(deg(Math.atan2(dx,dy))+360)%360:null}}
export function polygonMetrics(points:XY[]){if(points.length>3&&points[0][0]===points.at(-1)![0]&&points[0][1]===points.at(-1)![1])points=points.slice(0,-1);if(points.length<3||points.length>500||points.some(p=>!validPoint(p))||new Set(points.map(p=>p.join(","))).size!==points.length)throw Error("Nhập từ 3 đến 500 điểm phân biệt.");
 const cross=(a:XY,b:XY,c:XY)=>(b[0]-a[0])*(c[1]-a[1])-(b[1]-a[1])*(c[0]-a[0]);
 const intersects=(a:XY,b:XY,c:XY,d:XY)=>Math.max(a[0],b[0])>=Math.min(c[0],d[0])&&Math.max(c[0],d[0])>=Math.min(a[0],b[0])&&Math.max(a[1],b[1])>=Math.min(c[1],d[1])&&Math.max(c[1],d[1])>=Math.min(a[1],b[1])&&cross(a,b,c)*cross(a,b,d)<=0&&cross(c,d,a)*cross(c,d,b)<=0;
 for(let i=0;i<points.length;i++)for(let j=i+2;j<points.length;j++){if(i===0&&j===points.length-1)continue;if(intersects(points[i],points[(i+1)%points.length],points[j],points[(j+1)%points.length]))throw Error("Polygon tự giao hoặc chạm cạnh không liền kề.")}const origin=points[0];let twice=0,perimeter=0;for(let i=0;i<points.length;i++){const a=points[i],b=points[(i+1)%points.length];twice+=(a[0]-origin[0])*(b[1]-origin[1])-(b[0]-origin[0])*(a[1]-origin[1]);perimeter+=Math.hypot(b[0]-a[0],b[1]-a[1])}if(Math.abs(twice)<1e-9)throw Error("Polygon có diện tích bằng 0.");return {area:Math.abs(twice)/2,perimeter}}

export function bowditch(start:XY,legs:Leg[],target:XY){
 const raw=traverse(start,legs),last=raw.at(-1)!;
 if(!validPoint(target))throw Error("Tọa độ điểm khép không hợp lệ.");
 const err=inverse(target,last.point);let point:XY=[...start];
 const adjusted=raw.map((leg,i)=>{const ratio=leg.distance/last.total,cE=-err.dx*ratio,cN=-err.dy*ratio;point=[point[0]+leg.dx+cE,point[1]+leg.dy+cN];return {i:leg.i,raw:leg.point,point:[...point] as XY,cE,cN,dE:leg.dx+cE,dN:leg.dy+cN,distance:leg.distance,azimuth:leg.azimuth}});
 return {adjusted,err,total:last.total,relativePrecision:err.distance>1e-9?last.total/err.distance:null,residual:inverse(target,adjusted.at(-1)!.point).distance};
}
