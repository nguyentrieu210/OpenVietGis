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
export function validateGeoref(raw:unknown,p:GeorefPreview,points:Gcp[],epsg:number):GeorefResult{
 const v=validatePreview(raw,p.source_sha256) as GeorefResult;
 const residual=(a:unknown):a is number[]=>Array.isArray(a)&&a.length===points.length&&a.every(x=>finite(x)&&x>=0);
 if(v.mode!=="solve"||v.width!==p.width||v.height!==p.height||v.bands!==p.bands||v.epsg!==epsg||!Array.isArray(v.transform)||v.transform.length!==6||!v.transform.every(finite)||!Array.isArray(v.corners)||v.corners.length!==4||!v.corners.every(c=>Array.isArray(c)&&c.length===2&&c.every(finite)&&Math.abs(c[0])<=180&&Math.abs(c[1])<=85.05112878)||!residual(v.residuals)||!residual(v.pixel_residuals)||!finite(v.rms)||v.rms<0||!finite(v.pixel_rms)||v.pixel_rms<0||v.redundant!==(points.length>3)||!digest(v.sha256)||!integer(v.byte_size)||v.byte_size<=0||v.byte_size>16*1024*1024||typeof v.tiff!=="string"||v.tiff.length!==4*Math.ceil(v.byte_size/3))throw Error("Kết quả định vị không hợp lệ.");
 const rms=(a:number[])=>Math.sqrt(a.reduce((sum,n)=>sum+n*n,0)/a.length);
 if(Math.abs(rms(v.residuals)-v.rms)>Math.max(1e-10,v.rms*1e-8)||Math.abs(rms(v.pixel_residuals)-v.pixel_rms)>Math.max(1e-10,v.pixel_rms*1e-8))throw Error("Sai số RMS không nhất quán.");
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
