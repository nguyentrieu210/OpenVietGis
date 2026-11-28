/** Bounded local preview of uncompressed LAS 1.2/1.3, point formats 0–3.
 * No implicit CRS inference or upload. The operator must confirm WGS84 horizontal
 * CRS and ellipsoidal metre heights; unsupported metadata is rejected. */
export type LasPoint={longitude:number;latitude:number;height:number;intensity:number};
export type LasCloud={points:LasPoint[];count:number;center:[number,number];bounds:[number,number,number,number]};
export const LAS_MAX_BYTES=64*1024*1024;
export const LAS_MAX_POINTS=100000;
const R=6378137;
export function parseLas(bytes:ArrayBuffer,crs:4326|3857):LasCloud{
 if(bytes.byteLength<227||bytes.byteLength>LAS_MAX_BYTES)throw Error("LAS size exceeds preview limits");
 if(crs!==4326&&crs!==3857)throw Error("Unsupported horizontal CRS");
 const d=new DataView(bytes),s=(o:number,n:number)=>String.fromCharCode(...new Uint8Array(bytes,o,n)).replace(/\0.*$/s,"").trim();
 if(s(0,4)!=="LASF"||d.getUint8(24)!==1||![2,3].includes(d.getUint8(25)))throw Error("Only LAS 1.2/1.3 is supported");
 const minor=d.getUint8(25),header=d.getUint16(94,true),offset=d.getUint32(96,true),vlrs=d.getUint32(100,true),format=d.getUint8(104),length=d.getUint16(105,true),count=d.getUint32(107,true);
 if(header<(minor===3?235:227)||header>offset||offset>bytes.byteLength||vlrs>1024||format>3||length<[20,28,26,34][format]!||count<1||count>2000000||offset+length*count>bytes.byteLength)throw Error("Invalid or unsupported LAS records (LAZ is not supported)");
 if(d.getUint16(6,true)&16)throw Error("WKT CRS metadata is unsupported");
 let cursor=header;
 for(let i=0;i<vlrs;i++){
  if(cursor+54>offset)throw Error("Truncated LAS metadata");
  const user=s(cursor+2,16),id=d.getUint16(cursor+18,true),size=d.getUint16(cursor+20,true);
  if(cursor+54+size>offset)throw Error("Truncated LAS metadata payload");
  if(user==="LASF_Projection"){
   if(id!==34735||size<8||size%2)throw Error("Unsupported LAS CRS metadata; convert to an explicitly supported CRS first");
   const start=cursor+54,keys=d.getUint16(start+6,true);
   if(d.getUint16(start,true)!==1||8+keys*8>size)throw Error("Invalid GeoKey directory");
   let horizontal=false;
   for(let k=0;k<keys;k++){
    const p=start+8+k*8,key=d.getUint16(p,true),tag=d.getUint16(p+2,true),num=d.getUint16(p+4,true),value=d.getUint16(p+6,true);
    if(key===2048||key===3072){if(tag!==0||num!==1||value!==(key===2048?4326:crs)||key===3072&&crs!==3857)throw Error("LAS CRS does not match the selected CRS");if(key===(crs===4326?2048:3072))horizontal=true}
    if(key===4096||key===4099||key===4098){if(tag!==0||num!==1||!(key===4099&&value===9001||key===4096&&value===4979))throw Error("Unsupported vertical CRS metadata")}
   }
   if(!horizontal)throw Error("LAS CRS cannot be verified");
  }
  cursor+=54+size;
 }
 const scales=[131,139,147].map(i=>d.getFloat64(i,true)),origins=[155,163,171].map(i=>d.getFloat64(i,true));
 if(scales.some(v=>!Number.isFinite(v)||v<=0)||origins.some(v=>!Number.isFinite(v)))throw Error("Invalid LAS scale or offset");
 const points:LasPoint[]=[];let west=Infinity,east=-Infinity,south=Infinity,north=-Infinity;
 const step=Math.max(1,Math.ceil(count/LAS_MAX_POINTS));
 for(let i=0;i<count;i++){
  const at=offset+i*length,xyz=[0,4,8].map((o,j)=>d.getInt32(at+o,true)*scales[j]!+origins[j]!);
  let [longitude,latitude,height]=xyz as [number,number,number];
  if(crs===3857){longitude=longitude/R*180/Math.PI;latitude=Math.atan(Math.sinh(latitude/R))*180/Math.PI}
  if(![longitude,latitude,height].every(Number.isFinite)||Math.abs(longitude)>180||Math.abs(latitude)>85.051129||height< -12000||height>100000)throw Error("LAS coordinate outside supported range");
  west=Math.min(west,longitude);east=Math.max(east,longitude);south=Math.min(south,latitude);north=Math.max(north,latitude);
  if(i%step===0)points.push({longitude,latitude,height,intensity:d.getUint16(at+12,true)});
 }
 if(east-west>180)throw Error("LAS crossing the antimeridian is unsupported");
 return {points,count,center:[(west+east)/2,(south+north)/2],bounds:[west,south,east,north]};
}
