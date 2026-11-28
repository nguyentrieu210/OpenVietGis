import type {FeatureCollection,Geometry,Position} from "geojson";
export type VectorGcp=[number,number,number,number];
export const VECTOR_GEOREF_LIMITS={features:5000,vertices:200000,bytes:8*1024*1024};
const finite=(n:unknown):n is number=>typeof n==="number"&&Number.isFinite(n);
type XY=[number,number];
/** Center and normalize both source axes before solving the two-variable least-squares system. */
export function fitVectorAffine(points:readonly VectorGcp[]){
 if(points.length<3||points.length>32||points.some(p=>p.length!==4||!p.every(finite)||Math.abs(p[2])>180||Math.abs(p[3])>85.05112878)||new Set(points.map(p=>JSON.stringify(p.slice(0,2)))).size!==points.length)throw Error("Cần 3–32 GCP XY duy nhất và đích WGS84 hợp lệ.");
 const n=points.length,mean=(i:number)=>points.reduce((s,p)=>s+p[i]!,0)/n;
 const mx=mean(0),my=mean(1),tx=mean(2),ty=mean(3),sx=Math.max(...points.map(p=>Math.abs(p[0]-mx))),sy=Math.max(...points.map(p=>Math.abs(p[1]-my)));
 if(![mx,my,tx,ty,sx,sy].every(finite)||sx===0||sy===0)throw Error("GCP suy biến.");
 let xx=0,xy=0,yy=0,ux=0,uy=0,vx=0,vy=0;
 for(const [x,y,u,v] of points){const a=(x-mx)/sx,b=(y-my)/sy;xx+=a*a;xy+=a*b;yy+=b*b;ux+=a*(u-tx);uy+=b*(u-tx);vx+=a*(v-ty);vy+=b*(v-ty)}
 const det=xx*yy-xy*xy;
 if(!finite(det)||det<=1e-10*xx*yy)throw Error("GCP thẳng hàng hoặc gần suy biến.");
 const a=(yy*ux-xy*uy)/det,b=(xx*uy-xy*ux)/det,c=(yy*vx-xy*vy)/det,d=(xx*vy-xy*vx)/det;
 const scale=Math.max(Math.abs(a),Math.abs(b),Math.abs(c),Math.abs(d));
 if(!finite(scale)||scale===0||Math.abs(a*d-b*c)<=1e-10*scale*scale)throw Error("Phép biến đổi đích suy biến.");
 const transform=(x:number,y:number):XY=>[tx+a*(x-mx)/sx+b*(y-my)/sy,ty+c*(x-mx)/sx+d*(y-my)/sy];
 const residuals=points.map(([x,y,u,v])=>{const [p,q]=transform(x,y);return Math.hypot(p-u,q-v)});
 return {transform,residuals,rms:Math.sqrt(residuals.reduce((s,r)=>s+r*r,0)/n)};
}
function record(v:unknown):v is Record<string,unknown>{return !!v&&typeof v==="object"&&!Array.isArray(v)}
export function georeferenceVector(raw:unknown,points:readonly VectorGcp[]){
 if(!record(raw)||raw.type!=="FeatureCollection"||!Array.isArray(raw.features)||raw.features.length<1||raw.features.length>VECTOR_GEOREF_LIMITS.features)throw Error("Chọn FeatureCollection có 1–5.000 đối tượng.");
 const fit=fitVectorAffine(points);let vertices=0;
 const pos=(v:unknown):Position=>{
  if(!Array.isArray(v)||v.length!==2||!v.every(finite)||++vertices>VECTOR_GEOREF_LIMITS.vertices)throw Error("Chỉ hỗ trợ XY hữu hạn, tối đa 200.000 đỉnh.");
  const p=fit.transform(v[0]!,v[1]!);
  if(!p.every(finite)||Math.abs(p[0])>180||Math.abs(p[1])>85.05112878)throw Error("Tọa độ kết quả ngoài miền WGS84.");
  return p;
 };
 const list=<T>(v:unknown,min:number,fn:(x:unknown)=>T):T[]=>{if(!Array.isArray(v)||v.length<min||v.length>VECTOR_GEOREF_LIMITS.vertices)throw Error("Cấu trúc hình học không hợp lệ.");return v.map(fn)};
 const line=(v:unknown)=>{const p=list(v,2,pos);for(let i=1;i<p.length;i++)if(Math.abs(p[i]![0]!-p[i-1]![0]!)>180)throw Error("Đường cắt kinh tuyến 180° chưa hỗ trợ.");return p};
 const ring=(v:unknown)=>{const p=line(v);if(p.length<4||p[0]![0]!==p[p.length-1]![0]||p[0]![1]!==p[p.length-1]![1])throw Error("Vòng polygon phải khép kín.");return p};
 const polygon=(v:unknown)=>list(v,1,ring);
 const geometry=(g:unknown,depth=0):Geometry|null=>{
  if(g===null)return null;
  if(!record(g)||depth>8)throw Error("Hình học không hợp lệ.");
  switch(g.type){
   case "Point":return {type:g.type,coordinates:pos(g.coordinates)};
   case "MultiPoint":return {type:g.type,coordinates:list(g.coordinates,1,pos)};
   case "LineString":return {type:g.type,coordinates:line(g.coordinates)};
   case "MultiLineString":return {type:g.type,coordinates:list(g.coordinates,1,line)};
   case "Polygon":return {type:g.type,coordinates:polygon(g.coordinates)};
   case "MultiPolygon":return {type:g.type,coordinates:list(g.coordinates,1,polygon)};
   case "GeometryCollection":return {type:g.type,geometries:list(g.geometries,1,v=>{const child=geometry(v,depth+1);if(!child)throw Error("GeometryCollection chứa NULL.");return child})};
   default:throw Error("Kiểu hình học không hỗ trợ.");
  }
 };
 const collection:FeatureCollection={type:"FeatureCollection",features:raw.features.map(f=>{
  if(!record(f)||f.type!=="Feature"||!(f.properties===null||record(f.properties))||!(f.id===undefined||typeof f.id==="string"||finite(f.id)))throw Error("Feature hoặc thuộc tính không hợp lệ.");
  return {type:"Feature",geometry:geometry(f.geometry),properties:f.properties,...(f.id===undefined?{}:{id:f.id})};
 })};
 if(!vertices)throw Error("Không có tọa độ để định vị.");
 return {collection,vertices,rms:fit.rms,residuals:fit.residuals};
}
