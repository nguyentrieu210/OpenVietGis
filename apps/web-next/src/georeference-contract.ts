export type Gcp=[number,number,number,number];
export type GeorefPreview={mode:"inspect"|"solve";persisted:false;width:number;height:number;bands:number;source_sha256:string;png:string};
export type GeorefResult=GeorefPreview&{mode:"solve";epsg:number;transform:number[];corners:[number,number][];residuals:number[];pixel_residuals:number[];rms:number;pixel_rms:number;redundant:boolean;sha256:string;byte_size:number;tiff:string};
const finite=(x:unknown):x is number=>typeof x==="number"&&Number.isFinite(x);
const integer=(x:unknown):x is number=>finite(x)&&Number.isSafeInteger(x);
const digest=(s:unknown)=>typeof s==="string"&&/^[a-f0-9]{64}$/.test(s);
export function validatePreview(raw:unknown,sha:string):GeorefPreview{
 const p=raw as GeorefPreview;
 if(!p||p.persisted!==false||!["inspect","solve"].includes(p.mode)||p.source_sha256!==sha||!digest(sha)||!integer(p.width)||!integer(p.height)||p.width<2||p.height<2||p.width*p.height>2_000_000||!integer(p.bands)||p.bands<1||p.bands>16||typeof p.png!=="string"||p.png.length>2_000_000||!p.png.startsWith("iVBORw0KGgo"))throw Error("Ảnh xem trước không khớp tệp nguồn.");
 return p;
}
export function validateGcps(points:Gcp[],p:GeorefPreview,epsg:number){
 if(![4326,3857].includes(epsg)||points.length<3||points.length>32||points.some(g=>g.length!==4||!g.every(finite)||g[0]<0||g[0]>p.width||g[1]<0||g[1]>p.height||Math.abs(g[2])>(epsg===4326?180:20037508.342789244)||Math.abs(g[3])>(epsg===4326?85.05112878:20037508.342789244))||new Set(points.map(g=>JSON.stringify(g.slice(0,2)))).size!==points.length)throw Error("Cần 3–32 GCP duy nhất, trong ảnh và miền CRS.");
 return points;
}
/** Verify the declared affine, footprint and point residuals against the submitted GCPs. */
function checkAffine(v:GeorefResult,points:Gcp[]){
 const [x0,a,b,y0,c,d]=v.transform as [number,number,number,number,number,number];
 const scale=Math.max(Math.abs(a),Math.abs(b),Math.abs(c),Math.abs(d));
 const det=a*d-b*c;
 if(scale===0||!Number.isFinite(det)||Math.abs(det)/(scale*scale)<1e-8)throw Error("Ma trận affine suy biến.");
 const native=(x:number,y:number):[number,number]=>[x0+a*x+b*y,y0+c*x+d*y];
 const bound=v.epsg===4326?[180,85.05112878]:[20037508.342789244,20037508.342789244];
 const geographic=(x:number,y:number):[number,number]=>v.epsg===4326?[x,y]:[x/6378137*180/Math.PI,(2*Math.atan(Math.exp(y/6378137))-Math.PI/2)*180/Math.PI];
 const approx=(a:number,b:number,tol:number)=>Number.isFinite(a)&&Math.abs(a-b)<=Math.max(tol,Math.abs(b)*1e-8);
 const corners=[[0,0],[v.width,0],[v.width,v.height],[0,v.height]] as [number,number][];
 for(let i=0;i<corners.length;i++){
  const n=native(...corners[i]!);
  if(n.some((x,j)=>!Number.isFinite(x)||Math.abs(x)>bound[j]!+1e-8))throw Error("Ranh ảnh ngoài miền CRS.");
  const g=geographic(...n),declared=v.corners[i]!;
  if(!g.every((x,j)=>approx(x,declared[j]!,1e-7)))throw Error("Ranh ảnh không khớp affine.");
 }
 if(Math.max(...v.corners.map(p=>p[0]))-Math.min(...v.corners.map(p=>p[0]))>180)throw Error("Ranh ảnh vượt miền bản đồ.");
 for(let i=0;i<points.length;i++){
  const [x,y,tx,ty]=points[i]!,n=native(x,y),dx=n[0]-tx,dy=n[1]-ty;
  const err=Math.hypot(dx,dy),pixel=Math.hypot((d*dx-b*dy)/det,(-c*dx+a*dy)/det);
  // Account for floating point cancellation at large projected coordinate origins.
  const nativeTolerance=Math.max(1e-10,Math.max(Math.abs(tx),Math.abs(ty),Math.abs(x0),Math.abs(y0))*Number.EPSILON*64);
  const pixelTolerance=Math.max(1e-7,nativeTolerance*2*scale/Math.abs(det));
  if(!approx(err,v.residuals[i]!,nativeTolerance)||!approx(pixel,v.pixel_residuals[i]!,pixelTolerance))throw Error("Sai số GCP không khớp affine.");
 }
}
export function validateGeoref(raw:unknown,p:GeorefPreview,points:Gcp[],epsg:number):GeorefResult{
 validateGcps(points,p,epsg);
 const v=validatePreview(raw,p.source_sha256) as GeorefResult;
 const residual=(a:unknown):a is number[]=>Array.isArray(a)&&a.length===points.length&&a.every(x=>finite(x)&&x>=0);
 if(v.mode!=="solve"||v.width!==p.width||v.height!==p.height||v.bands!==p.bands||v.epsg!==epsg||!Array.isArray(v.transform)||v.transform.length!==6||!v.transform.every(finite)||!Array.isArray(v.corners)||v.corners.length!==4||!v.corners.every(c=>Array.isArray(c)&&c.length===2&&c.every(finite)&&Math.abs(c[0])<=180&&Math.abs(c[1])<=85.05112878)||!residual(v.residuals)||!residual(v.pixel_residuals)||!finite(v.rms)||v.rms<0||!finite(v.pixel_rms)||v.pixel_rms<0||v.redundant!==(points.length>3)||!digest(v.sha256)||!integer(v.byte_size)||v.byte_size<=0||v.byte_size>16*1024*1024||typeof v.tiff!=="string"||v.tiff.length!==4*Math.ceil(v.byte_size/3))throw Error("Kết quả định vị không hợp lệ.");
 const rms=(a:number[])=>Math.sqrt(a.reduce((sum,n)=>sum+n*n,0)/a.length);
 if(Math.abs(rms(v.residuals)-v.rms)>Math.max(1e-10,v.rms*1e-8)||Math.abs(rms(v.pixel_residuals)-v.pixel_rms)>Math.max(1e-10,v.pixel_rms*1e-8))throw Error("Sai số RMS không nhất quán.");
 checkAffine(v,points);
 return v;
}
export async function sha256(bytes:ArrayBuffer){
 if(!crypto.subtle)throw Error("Cần HTTPS để xác minh tệp.");
 return Array.from(new Uint8Array(await crypto.subtle.digest("SHA-256",bytes)),x=>x.toString(16).padStart(2,"0")).join("");
}
export async function georefBlob(r:GeorefResult){
 const bytes=Uint8Array.from(atob(r.tiff),x=>x.charCodeAt(0));
 if(bytes.length!==r.byte_size||await sha256(bytes.buffer)!==r.sha256||![[73,73,42,0],[77,77,0,42],[73,73,43,0],[77,77,0,43]].some(m=>m.every((x,i)=>bytes[i]===x)))throw Error("Tệp GeoTIFF không khớp checksum.");
 return new Blob([bytes],{type:"image/tiff"});
}
export function mapTarget(lng:number,lat:number,epsg:number):[number,number]{
 if(!finite(lng)||!finite(lat)||Math.abs(lng)>180||Math.abs(lat)>85.05112878||![4326,3857].includes(epsg))throw Error("Tọa độ bản đồ ngoài miền hỗ trợ.");
 return epsg===4326?[lng,lat]:[6378137*lng*Math.PI/180,6378137*Math.log(Math.tan(Math.PI/4+lat*Math.PI/360))];
}
