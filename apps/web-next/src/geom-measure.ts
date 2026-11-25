// Spec 058: "Derived" values of the Identify panel (QGIS shows area, perimeter, vertices, centroid).
// Spherical approximation on WGS 84 lon/lat (R = 6 371 008.8 m); good to ~0.3 % for parcels and roads.
import type {Geometry,Position} from "geojson";
const R=6371008.8,rad=Math.PI/180;
const seg=(a:Position,b:Position)=>{const p1=a[1]!*rad,p2=b[1]!*rad,dp=p2-p1,dl=(b[0]!-a[0]!)*rad;const h=Math.sin(dp/2)**2+Math.cos(p1)*Math.cos(p2)*Math.sin(dl/2)**2;return 2*R*Math.asin(Math.min(1,Math.sqrt(h)))};
export const lineLength=(c:Position[])=>c.slice(1).reduce((s,p,i)=>s+seg(c[i]!,p),0);
const ringArea=(c:Position[])=>{let s=0;for(let i=0;i<c.length-1;i++){const a=c[i]!,b=c[i+1]!;s+=(b[0]!-a[0]!)*rad*(2+Math.sin(a[1]!*rad)+Math.sin(b[1]!*rad))}return Math.abs(s*R*R/2)};
const polyArea=(rings:Position[][])=>rings.reduce((s,r,i)=>i===0?s+ringArea(r):s-ringArea(r),0);
export type Derived={kind:string;vertices:number;area?:number;perimeter?:number;length?:number;centroid?:[number,number]};
export function derive(g:Geometry):Derived{
 const all:Position[]=[];const walk=(x:unknown)=>{if(Array.isArray(x)&&typeof x[0]==="number")all.push(x as Position);else if(Array.isArray(x))x.forEach(walk)};
 if("coordinates" in g)walk(g.coordinates);
 const centroid:[number,number]|undefined=all.length?[all.reduce((s,p)=>s+p[0]!,0)/all.length,all.reduce((s,p)=>s+p[1]!,0)/all.length]:undefined;
 const base={vertices:all.length,...(centroid?{centroid}:{})};
 if(g.type==="Polygon")return {kind:"Vùng",...base,area:polyArea(g.coordinates),perimeter:lineLength(g.coordinates[0]??[])};
 if(g.type==="MultiPolygon")return {kind:"Đa vùng",...base,area:g.coordinates.reduce((s,p)=>s+polyArea(p),0),perimeter:g.coordinates.reduce((s,p)=>s+lineLength(p[0]??[]),0)};
 if(g.type==="LineString")return {kind:"Đường",...base,length:lineLength(g.coordinates)};
 if(g.type==="MultiLineString")return {kind:"Đa đường",...base,length:g.coordinates.reduce((s,l)=>s+lineLength(l),0)};
 return {kind:g.type==="Point"?"Điểm":g.type,...base};
}
export const fmtLength=(m:number)=>m>=1000?(m/1000).toLocaleString("vi-VN",{maximumFractionDigits:3})+" km":m.toLocaleString("vi-VN",{maximumFractionDigits:2})+" m";
export const fmtArea=(m2:number)=>m2.toLocaleString("vi-VN",{maximumFractionDigits:1})+" m²"+(m2>=10000?" ("+(m2/10000).toLocaleString("vi-VN",{maximumFractionDigits:4})+" ha)":"");
