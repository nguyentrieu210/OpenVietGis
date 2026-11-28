/** Bounded local LAS 1.2–1.4 preview; no compressed records or arbitrary CRS transforms. */
export type LasPoint={longitude:number;latitude:number;height:number;intensity:number;rgb?:readonly [number,number,number]};
export type LasCloud={points:LasPoint[];count:number;center:[number,number];bounds:[number,number,number,number]};
export const LAS_MAX_BYTES=64*1024*1024;
export const LAS_MAX_POINTS=100000;
const R=6378137;
const lengths:Record<number,number>={0:20,1:28,2:26,3:34,6:30,7:36,8:38};
const rgbOffset:Record<number,number>={2:20,3:28,7:30,8:30};
/** Recognize only a root horizontal EPSG authority; never infer CRS from nested datum IDs.
 * WKT text is not executed as a projection pipeline. Explicit operator confirmation is required. */
export function validateLasWkt(wkt:string,crs:4326|3857){
 const root=/^\s*(GEOGCS|PROJCS|GEOGCRS|GEODCRS|PROJCRS)\s*\[/i.exec(wkt);
 if(!root||wkt.length>32768)throw Error("Unsupported LAS WKT CRS");
 const projected=/^PROJ/i.test(root[1]!);if(projected!==(crs===3857))throw Error("LAS WKT CRS mismatch");
 const top:string[]=[];let depth=1,quoted=false,start=root[0].length,end=-1;
 for(let i=start;i<wkt.length;i++){
  const c=wkt[i];if(c==='"'){if(quoted&&wkt[i+1]==='"'){i++;continue}quoted=!quoted;continue}
  if(quoted)continue;
  if(c==="["){if(++depth>32)throw Error("WKT nesting limit")}
  else if(c==="]"){if(--depth===0){top.push(wkt.slice(start,i).trim());end=i;break}}
  else if(c===","&&depth===1){top.push(wkt.slice(start,i).trim());start=i+1}
 }
 if(end<0||quoted||wkt.slice(end+1).trim())throw Error("Invalid LAS WKT");
 const authorities=top.filter(x=>/^(ID|AUTHORITY)\s*\[/i.test(x));
 if(authorities.length!==1||!new RegExp('^(?:ID|AUTHORITY)\\s*\\[\\s*"EPSG"\\s*,\\s*"?'+crs+'"?\\s*\\]$','i').test(authorities[0]!))throw Error("LAS WKT lacks matching root EPSG authority");
 const axes=top.filter(x=>/^AXIS\s*\[/i.test(x));
 if(axes.length!==0&&axes.length!==2)throw Error("Only horizontal 2D WKT is supported");
 if(root[1]!.toUpperCase().endsWith("CRS")){
  const cs=top.filter(x=>/^CS\s*\[/i.test(x));
  if(cs.length!==1||!/^CS\s*\[\s*(ellipsoidal|Cartesian)\s*,\s*2\s*\]$/i.test(cs[0]!))throw Error("Only horizontal 2D WKT is supported");
 }
 if(top.some(x=>/^(VERT_CS|VERTCRS|COMPOUNDCRS|BOUNDCRS)\s*\[/i.test(x)))throw Error("Unsupported LAS vertical or compound CRS");
}
export function parseLas(bytes:ArrayBuffer,crs:4326|3857):LasCloud{return parseLasRecords(bytes,crs)}
export function parseLasRecords(bytes:ArrayBuffer,crs:4326|3857,decoded?:ArrayBuffer):LasCloud{
 if(bytes.byteLength<227||bytes.byteLength>LAS_MAX_BYTES)throw Error("LAS size exceeds preview limits");
 if(crs!==4326&&crs!==3857)throw Error("Unsupported horizontal CRS");
 const d=new DataView(bytes),s=(o:number,n:number)=>String.fromCharCode(...new Uint8Array(bytes,o,n)).replace(/\0.*$/s,"").trim();
 if(s(0,4)!=="LASF"||d.getUint8(24)!==1||![2,3,4].includes(d.getUint8(25)))throw Error("Only LAS 1.2–1.4 is supported");
 const minor=d.getUint8(25),header=d.getUint16(94,true),offset=d.getUint32(96,true),vlrs=d.getUint32(100,true),rawFormat=d.getUint8(104),format=decoded?rawFormat&63:rawFormat,length=d.getUint16(105,true);
 if(decoded&&(rawFormat&128)===0||header<(minor===4?375:minor===3?235:227)||header>offset||offset>bytes.byteLength||vlrs>1024||!Object.hasOwn(lengths,format)||format>=6&&minor!==4||length<lengths[format]!)throw Error("Invalid or unsupported LAS records (LAZ is not supported)");
 const legacy=d.getUint32(107,true),extended=minor===4?d.getBigUint64(247,true):0n;
 if(extended>2000000n||minor===4&&format>=6&&(legacy!==0||extended===0n)||extended&&legacy&&extended!==BigInt(legacy))throw Error("Invalid LAS point count");
 const count=extended?Number(extended):legacy,pointEnd=decoded?offset:offset+length*count;
 if(count<1||count>2000000||pointEnd>bytes.byteLength||decoded&&(decoded.byteLength!==count*length||decoded.byteLength>LAS_MAX_BYTES))throw Error("Invalid LAS point count or truncated records");
 const wktFlag=(d.getUint16(6,true)&16)!==0;
 if(format>=6&&!wktFlag||minor!==4&&wktFlag)throw Error("Invalid LAS WKT flag");
 let geoKeys=false,wktSeen=false;
 function metadata(user:string,id:number,start:number,size:number){
  if(user.toLowerCase()==="laszip encoded"){if(!decoded)throw Error("LAZ compression is unsupported");return}
  if(user!=="LASF_Projection")return;
  if(id===2112){
   if(!wktFlag||wktSeen||geoKeys||size<1||size>32768)throw Error("Conflicting or oversized WKT metadata");
   const raw=new Uint8Array(bytes,start,size),zero=raw.indexOf(0),end=zero<0?raw.length:zero;
   if(zero>=0&&raw.slice(zero).some(v=>v!==0))throw Error("Invalid WKT termination");
   validateLasWkt(new TextDecoder("utf-8",{fatal:true}).decode(raw.slice(0,end)),crs);wktSeen=true;return;
  }
  if(id!==34735||wktFlag||wktSeen||geoKeys||size<8||size%2)throw Error("Unsupported or conflicting LAS CRS metadata");
  const keys=d.getUint16(start+6,true);if(d.getUint16(start,true)!==1||8+keys*8>size)throw Error("Invalid GeoKey directory");
  let horizontal=false;const seen=new Set<number>();
  for(let k=0;k<keys;k++){
   const p=start+8+k*8,key=d.getUint16(p,true),tag=d.getUint16(p+2,true),num=d.getUint16(p+4,true),value=d.getUint16(p+6,true);
   if(seen.has(key))throw Error("Duplicate LAS GeoKey");seen.add(key);
   if(key===2048||key===3072){if(tag!==0||num!==1||value!==(key===2048?4326:crs)||key===3072&&crs!==3857)throw Error("LAS CRS does not match the selected CRS");if(key===(crs===4326?2048:3072))horizontal=true}
   if(key===4096||key===4099||key===4098){if(tag!==0||num!==1||!(key===4099&&value===9001||key===4096&&value===4979))throw Error("Unsupported vertical CRS metadata")}
  }
  if(!horizontal)throw Error("LAS CRS cannot be verified");geoKeys=true;
 }
 let cursor=header;
 for(let i=0;i<vlrs;i++){if(cursor+54>offset)throw Error("Truncated LAS metadata");const size=d.getUint16(cursor+20,true);if(cursor+54+size>offset)throw Error("Truncated LAS metadata payload");metadata(s(cursor+2,16),d.getUint16(cursor+18,true),cursor+54,size);cursor+=54+size}
 if(minor===4){
  const pos=d.getBigUint64(235,true),n=d.getUint32(243,true);if(n>1024||(!n&&pos!==0n)||(n>0&&(pos<BigInt(pointEnd)||pos>BigInt(bytes.byteLength))))throw Error("Invalid LAS EVLR offset");
  cursor=Number(pos);for(let i=0;i<n;i++){if(cursor+60>bytes.byteLength)throw Error("Truncated LAS EVLR");const big=d.getBigUint64(cursor+20,true);if(big>BigInt(bytes.byteLength-cursor-60))throw Error("Truncated LAS EVLR payload");const size=Number(big);metadata(s(cursor+2,16),d.getUint16(cursor+18,true),cursor+60,size);cursor+=60+size}
 }
 if(wktFlag&&!wktSeen)throw Error("Missing LAS WKT CRS metadata");
 const scales=[131,139,147].map(i=>d.getFloat64(i,true)),origins=[155,163,171].map(i=>d.getFloat64(i,true));
 if(scales.some(v=>!Number.isFinite(v)||v<=0)||origins.some(v=>!Number.isFinite(v)))throw Error("Invalid LAS scale or offset");
 const points:LasPoint[]=[],recordView=decoded?new DataView(decoded):d;let west=Infinity,east=-Infinity,south=Infinity,north=-Infinity;
 const step=Math.max(1,Math.ceil(count/LAS_MAX_POINTS));
 for(let i=0;i<count;i++){
  const at=(decoded?0:offset)+i*length,xyz=[0,4,8].map((o,j)=>recordView.getInt32(at+o,true)*scales[j]!+origins[j]!);
  let [longitude,latitude,height]=xyz as [number,number,number];
  if(crs===3857){if(Math.abs(longitude)>Math.PI*R||Math.abs(latitude)>Math.PI*R)throw Error("Projected LAS coordinate outside domain");longitude=longitude/R*180/Math.PI;latitude=Math.atan(Math.sinh(latitude/R))*180/Math.PI}
  if(![longitude,latitude,height].every(Number.isFinite)||Math.abs(longitude)>180||Math.abs(latitude)>85||height< -12000||height>100000)throw Error("LAS coordinate outside supported range");
  west=Math.min(west,longitude);east=Math.max(east,longitude);south=Math.min(south,latitude);north=Math.max(north,latitude);
  if(i%step===0){const rgb=rgbOffset[format];points.push({longitude,latitude,height,intensity:recordView.getUint16(at+12,true),...(rgb===undefined?{}:{rgb:[recordView.getUint16(at+rgb,true),recordView.getUint16(at+rgb+2,true),recordView.getUint16(at+rgb+4,true)] as const})})}
 }
 if(east-west>180)throw Error("LAS crossing the antimeridian is unsupported");
 return {points,count,center:[(west+east)/2,(south+north)/2],bounds:[west,south,east,north]};
}
