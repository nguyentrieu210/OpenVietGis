/** Bounded CDF-1/CDF-2 reader for CF-style scalar node time series.
 * Supports fixed and interleaved record arrays; no HDF5 or inferred node identity.
 * NetCDF classic is big endian; offsets and lengths are checked before reads.
 */
import {parseMeshDataset,DATASET_MAX_BYTES,type MeshDataset} from "./mesh-dataset";
import type {LocalMesh} from "./local-mesh";
type Attr=string|number[];
type Variable={name:string;dims:number[];attrs:Map<string,Attr>;type:number;size:number;offset:number};
const width=[0,1,1,2,4,4,8];
export function parseClassicNetcdf(bytes:ArrayBuffer,mesh:LocalMesh,variableName:string):MeshDataset{
 if(bytes.byteLength<32||bytes.byteLength>DATASET_MAX_BYTES)throw Error("NetCDF exceeds 16 MiB limit");
 const d=new DataView(bytes),decoder=new TextDecoder("utf-8",{fatal:true});let p=0;
 const need=(n:number)=>{if(!Number.isSafeInteger(n)||n<0||p+n>d.byteLength)throw Error("Truncated NetCDF")};
 const u32=()=>{need(4);const v=d.getUint32(p);p+=4;return v};
 const padded=(n:number)=>Math.ceil(n/4)*4;
 const str=()=>{const n=u32();if(!n||n>1024)throw Error("Invalid NetCDF name");need(padded(n));const s=decoder.decode(new Uint8Array(bytes,p,n));p+=padded(n);if(!s.trim()||s.includes("\0"))throw Error("Invalid NetCDF name");return s};
 const tag=u32(),version=tag&255;
 if(tag>>>8!==0x434446||![1,2].includes(version))throw Error("Only classic NetCDF CDF-1/CDF-2 is supported");
 const records=u32();if(records===0xffffffff||records>120)throw Error("Unsupported NetCDF record count");
 function list<T>(kind:number,read:()=>T,max=256):T[]{
  const t=u32(),n=u32();if(t===0&&n===0)return [];
  if(t!==kind||n>max)throw Error("Invalid NetCDF header list");
  return Array.from({length:n},read);
 }
 function numeric(type:number,at:number):number{
  switch(type){case 1:return d.getInt8(at);case 3:return d.getInt16(at);case 4:return d.getInt32(at);case 5:return d.getFloat32(at);case 6:return d.getFloat64(at);default:throw Error("Unsupported NetCDF numeric type")}
 }
 const dims=list(10,()=>({name:str(),length:u32()}),64);
 if(new Set(dims.map(x=>x.name)).size!==dims.length||dims.filter(x=>x.length===0).length>1||dims.some(x=>x.length>1000000))throw Error("Invalid NetCDF dimensions");
 const attrs=()=>{
  const a=new Map<string,Attr>();for(const [name,value] of list(12,():[string,Attr]=>{
   const name=str(),type=u32(),n=u32(),w=width[type]??0,sz=n*w;
   if(!w||n>1024*16)throw Error("Unsupported NetCDF attribute");need(padded(sz));
   const value=type===2?decoder.decode(new Uint8Array(bytes,p,sz)).replace(/\0+$/,""):Array.from({length:n},(_,i)=>numeric(type,p+i*w));
   p+=padded(sz);return [name,value];
  })){if(a.has(name))throw Error("Duplicate NetCDF attribute");a.set(name,value)}return a;
 };
 attrs();
 const vars=list(11,():Variable=>{
  const name=str(),n=u32();if(n>8)throw Error("NetCDF rank limit");const ids=Array.from({length:n},u32);
  if(ids.some(i=>i>=dims.length)||new Set(ids).size!==ids.length)throw Error("Invalid NetCDF dimension reference");
  const a=attrs(),type=u32(),size=u32();if(!width[type]||size>DATASET_MAX_BYTES)throw Error("Unsupported NetCDF variable");
  let offset:number;if(version===1)offset=u32();else{const hi=u32(),lo=u32();if(hi!==0)throw Error("NetCDF offset exceeds input limit");offset=lo}
  return {name,dims:ids,attrs:a,type,size,offset};
 });
 if(new Set(vars.map(v=>v.name)).size!==vars.length)throw Error("Duplicate NetCDF variable");
 const header=p;
 const unlimited=dims.findIndex(x=>x.length===0);
 if(unlimited<0&&records!==0)throw Error("Unexpected NetCDF records");
 const recordVars=vars.filter(v=>v.dims[0]===unlimited&&unlimited>=0);
 if(vars.some(v=>unlimited>=0&&v.dims.slice(1).includes(unlimited)))throw Error("Unlimited dimension must be first");
 if(unlimited>=0&&(!recordVars.length||records<1))throw Error("Empty NetCDF record dimension");
 const count=(v:Variable,record:boolean)=>v.dims.slice(record?1:0).reduce((s,i)=>s*dims[i]!.length,1);
 const layout=(v:Variable,record:boolean)=>{
  const n=count(v,record),w=width[v.type]!,sz=n*w;
  if(!Number.isSafeInteger(n)||n<1||n>1000000||v.offset<header||v.offset%4||v.size!==padded(sz))throw Error("Invalid NetCDF variable extent: "+v.name);
  return {n,w,sz};
 };
 const fixed=vars.filter(v=>!recordVars.includes(v)).map(v=>{layout(v,false);return [v.offset,v.offset+v.size] as const}).sort((a,b)=>a[0]-b[0]);
 if(fixed.some((r,i)=>r[1]>d.byteLength||i>0&&fixed[i-1]![1]>r[0]))throw Error("Overlapping or truncated NetCDF fixed variables");
 const ordered=[...recordVars].sort((a,b)=>a.offset-b.offset);
 const base=ordered[0]?.offset??d.byteLength;
 let stride=0;
 for(const v of ordered){
  const {sz}=layout(v,true);
  if(v.offset!==base+stride)throw Error("Invalid NetCDF record interleave");
  stride+=ordered.length===1?sz:v.size;
 }
 if(ordered.length&&(fixed.some(r=>r[1]>base)||base+stride*records>d.byteLength))throw Error("Overlapping or truncated NetCDF records");
 if(unlimited>=0)dims[unlimited]!.length=records;
 const byName=(name:string)=>{const v=vars.find(x=>x.name===name);if(!v)throw Error("Missing NetCDF variable: "+name);return v};
 function values(v:Variable){
  const record=recordVars.includes(v),{n,w}=layout(v,record),total=n*(record?records:1);
  if(v.type===2||total>1000000)throw Error("Unsupported NetCDF numeric array: "+v.name);
  return Array.from({length:total},(_,i)=>numeric(v.type,v.offset+(record?Math.floor(i/n)*stride+i%n*w:i*w)));
 }
 const node=byName("node_id"),time=byName("time"),data=byName(variableName.trim());
 if(node.dims.length!==1||time.dims.length!==1||data.dims.length!==2||data.dims[0]!==time.dims[0]||data.dims[1]!==node.dims[0]||node.dims[0]===time.dims[0])throw Error("Dataset must have dimensions [time,node] and explicit node_id");
 const nn=dims[node.dims[0]!]!.length,nt=dims[time.dims[0]!]!.length;
 if(nn!==mesh.nodes.length||nt<1||nt>120||nn*nt>1000000)throw Error("NetCDF mesh/time dimensions mismatch");
 const unit=data.attrs.get("units"),timeUnit=time.attrs.get("units"),calendar=time.attrs.get("calendar");
 if(typeof unit!=="string"||unit.length>32||typeof timeUnit!=="string"||calendar!==undefined&&calendar!=="standard"&&calendar!=="gregorian"&&calendar!=="proleptic_gregorian")throw Error("Explicit units and Gregorian calendar required");
 const m=/^(seconds|minutes|hours|days) since (\d{4}-\d{2}-\d{2})[ T](\d{2}:\d{2}:\d{2})(?: ?(?:Z|UTC|\+00:00))$/.exec(timeUnit);
 if(!m)throw Error("CF time units require explicit UTC origin");
 const originText=m[2]+"T"+m[3]+".000Z",origin=Date.parse(originText);
 if(!Number.isFinite(origin)||new Date(origin).toISOString()!==originText)throw Error("Invalid UTC origin");
 const factor=({seconds:1000,minutes:60000,hours:3600000,days:86400000} as Record<string,number>)[m[1]!]!;
 const scalar=(v:Variable,k:string,def:number)=>{const a=v.attrs.get(k);if(a===undefined)return def;if(!Array.isArray(a)||a.length!==1||!Number.isFinite(a[0]))throw Error("Invalid "+k);return a[0]!};
 // No silent packing of coordinate identifiers or time offsets.
 for(const v of [node,time])if(v.attrs.has("scale_factor")||v.attrs.has("add_offset")||v.attrs.has("_FillValue")||v.attrs.has("missing_value"))throw Error("Packed or missing coordinates unsupported");
 const scale=scalar(data,"scale_factor",1),offset=scalar(data,"add_offset",0);
 if(scale===0)throw Error("Invalid NetCDF scale");
 const missing=[data.attrs.get("_FillValue"),data.attrs.get("missing_value")].flatMap(a=>a===undefined?[]:Array.isArray(a)?a:(()=>{throw Error("Invalid missing value")})());
 // NetCDF default fill values are recognized even without an explicit attribute.
 const defaults:Record<number,number>={1:-127,3:-32767,4:-2147483647,5:Math.fround(9.969209968386869e36),6:9.969209968386869e36};
 if(!data.attrs.has("_FillValue"))missing.push(defaults[data.type]!);
 const ids=values(node),ts=values(time),raw=values(data);
 const frames=ts.map((t,i)=>{
  const ms=origin+t*factor;if(!Number.isSafeInteger(ms)||Math.abs(ms)>8640000000000000)throw Error("Invalid NetCDF time");
  return {time:new Date(ms).toISOString(),values:raw.slice(i*nn,(i+1)*nn).map(v=>missing.some(x=>Object.is(x,v)||x===v)||Number.isNaN(v)?null:v*scale+offset)};
 });
 return parseMeshDataset({version:1,name:data.name,unit,kind:"scalar",node_ids:ids,frames},mesh);
}
