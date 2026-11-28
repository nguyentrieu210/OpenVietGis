/** NetCDF4/HDF5 node dataset, using NIST h5wasm in a dedicated worker.
 * Explicit coordinate identities and dimension scales; bounded scalar/vector slices.
 * HDF5 datasets with unsupported filters, external links or unverified axes are rejected.
 */
import type {LocalMesh} from "./local-mesh";
import {parseMeshDataset,DATASET_MAX_BYTES,type MeshDataset} from "./mesh-dataset";
type Attr=unknown;
type Numeric=Int8Array|Uint8Array|Int16Array|Uint16Array|Int32Array|Uint32Array|Float32Array|Float64Array;
export type HdfVariable={shape:number[]|null;dtype:unknown;attrs:Record<string,{value:Attr}>;filters:readonly {id:number}[];get_attached_scales(index:number):string[];slice(ranges:[]|[number,number][]):unknown;value:unknown};
const text=(v:Attr)=>typeof v==="string"?v:Array.isArray(v)&&v.length===1&&typeof v[0]==="string"?v[0]:undefined;
const attr=(v:HdfVariable,k:string)=>Object.hasOwn(v.attrs,k)?v.attrs[k]?.value:undefined;
const numeric=(v:unknown,n:number):number[]=>{
 if(!(v instanceof Int8Array||v instanceof Uint8Array||v instanceof Int16Array||v instanceof Uint16Array||v instanceof Int32Array||v instanceof Uint32Array||v instanceof Float32Array||v instanceof Float64Array)||v.length!==n)throw Error("HDF5 numeric array mismatch");
 return Array.from(v as Numeric);
};
function scalar(v:unknown,defaultValue:number){if(v===undefined)return defaultValue;if(typeof v==="number"&&Number.isFinite(v))return v;if(ArrayBuffer.isView(v)&&"length" in v&&v.length===1){const x=numeric(v,1)[0]!;if(Number.isFinite(x))return x}throw Error("Invalid HDF5 numeric attribute")}
function missingValues(v:unknown):number[]{if(v===undefined)return [];if(typeof v==="number")return [v];if(ArrayBuffer.isView(v)&&"length" in v&&typeof v.length==="number"&&v.length<=256)return numeric(v,v.length);throw Error("Invalid HDF5 missing values")}
function assertNumeric(v:HdfVariable){
 if(typeof v.dtype!=="string"||!/^([<>|=])?[bBhHiIfd]$/.test(v.dtype)||v.filters.some(f=>![1,2,3].includes(f.id)))throw Error("Unsupported HDF5 dtype or compression");
}
function scale(v:HdfVariable,axis:number,name:string){const names=v.get_attached_scales(axis);if(names.length!==1||names[0]!=="/"+name)throw Error("NetCDF4 axis lacks explicit dimension scale: "+name)}
function timeValues(v:HdfVariable,nt:number){
 const unit=text(attr(v,"units")),calendar=text(attr(v,"calendar"));
 if(!unit||calendar&& !["standard","gregorian","proleptic_gregorian"].includes(calendar))throw Error("NetCDF4 time units/calendar required");
 const m=/^(seconds|minutes|hours|days) since (\d{4}-\d{2}-\d{2})[ T](\d{2}:\d{2}:\d{2})(?: ?(?:Z|UTC|\+00:00))$/.exec(unit);
 if(!m)throw Error("NetCDF4 requires UTC time origin");
 const originText=m[2]+"T"+m[3]+".000Z",origin=Date.parse(originText);
 if(!Number.isFinite(origin)||new Date(origin).toISOString()!==originText)throw Error("Invalid NetCDF4 time origin");
 const factor=({seconds:1000,minutes:60000,hours:3600000,days:86400000} as Record<string,number>)[m[1]!]!;
 return numeric(v.value,nt).map(x=>{const ms=origin+x*factor;if(!Number.isSafeInteger(ms)||Math.abs(ms)>8640000000000000)throw Error("Invalid NetCDF4 time");return new Date(ms).toISOString()});
}
export function readNetcdf4Variables(get:(name:string)=>HdfVariable,mesh:LocalMesh,variable:string,componentY?:string):MeshDataset{
 const node=get("node_id"),time=get("time"),x=get(variable),y=componentY?get(componentY):undefined;
 for(const v of [node,time,x,...(y?[y]:[])])assertNumeric(v);
 if(node.shape?.length!==1||time.shape?.length!==1||node.shape[0]!==mesh.nodes.length)throw Error("NetCDF4 requires explicit node_id and time arrays");
 const nn=node.shape[0]!,nt=time.shape[0]!;
 if(nt<1||nt>120||nn*nt>1000000||mesh.triangles.length/3>10000)throw Error("NetCDF4 mesh budget exceeded");
 for(const v of [node,time])for(const k of ["scale_factor","add_offset","_FillValue","missing_value"])if(attr(v,k)!==undefined)throw Error("Packed/missing coordinate arrays unsupported");
 const ids=numeric(node.value,nn),times=timeValues(time,nt),unit=text(attr(x,"units"));
 if(unit===undefined||unit.length>32)throw Error("NetCDF4 data units required");
 const reader=(v:HdfVariable)=>{
  if(v.shape?.length!==2||v.shape[0]!==nt||v.shape[1]!==nn)throw Error("NetCDF4 data must be [time,node_id]");
  scale(v,0,"time");scale(v,1,"node_id");
  if(text(attr(v,"units"))!==unit)throw Error("NetCDF4 vector components must share units");
  const factor=scalar(attr(v,"scale_factor"),1),offset=scalar(attr(v,"add_offset"),0);
  if(!factor)throw Error("Invalid NetCDF4 scale");
  const fill=attr(v,"_FillValue"),missing=[...missingValues(fill),...missingValues(attr(v,"missing_value"))];
  if(fill===undefined){const defaults:Record<string,number>={b:-127,B:255,h:-32767,H:65535,i:-2147483647,I:4294967295,f:Math.fround(9.969209968386869e36),d:9.969209968386869e36};const k=String(v.dtype).replace(/^[<>|=]/,"");if(Object.hasOwn(defaults,k))missing.push(defaults[k]!)}
  return (i:number)=>numeric(v.slice([[i,i+1],[0,nn]]),nn).map(n=>Number.isNaN(n)||missing.includes(n)?null:n*factor+offset);
 };
 const readX=reader(x),readY=y?reader(y):undefined;
 const frames=times.map((time,i)=>{const a=readX(i),b=readY?.(i);return {time,values:b?a.map((v,j)=>v===null||b[j]===null?null:[v,b[j]]):a}});
 return parseMeshDataset({version:1,name:variable+(componentY?" / "+componentY:""),unit,kind:y?"vector":"scalar",node_ids:ids,frames},mesh);
}
export async function decodeNetcdf4(bytes:ArrayBuffer,mesh:LocalMesh,variable:string,componentY?:string):Promise<MeshDataset>{
 if(bytes.byteLength<8||bytes.byteLength>DATASET_MAX_BYTES||new DataView(bytes).getBigUint64(0)!==0x894844460d0a1a0an)throw Error("Invalid or oversized NetCDF4/HDF5 file");
 const validName=(s:string)=>/^[A-Za-z_][A-Za-z0-9_.-]{0,119}$/.test(s);
 if(!validName(variable)||componentY&&!validName(componentY)||variable==="time"||variable==="node_id"||componentY===variable||componentY==="time"||componentY==="node_id")throw Error("Invalid NetCDF4 variable name");
 const h=await import("h5wasm");await h.ready;
 const fs=h.FS;if(!fs)throw Error("HDF5 virtual filesystem unavailable");
 const path="/ovg-netcdf4.h5";fs.writeFile(path,new Uint8Array(bytes));
 let file:InstanceType<typeof h.File>|undefined;
 try{
  file=new h.File(path,"r");
  // Only root datasets: no groups, traversed external or soft links.
  const root=file;const get=(name:string):HdfVariable=>{
   if(!root.keys().includes(name)||root.get_link(name)||root.get_external_link(name)?.filename)throw Error("Missing or linked HDF5 variable: "+name);
   const v=root.get(name);if(!(v instanceof h.Dataset))throw Error("Not an HDF5 dataset: "+name);
   return v as unknown as HdfVariable;
  };
  return readNetcdf4Variables(get,mesh,variable,componentY);
 }finally{try{file?.close()}finally{fs.unlink(path)}}
}
