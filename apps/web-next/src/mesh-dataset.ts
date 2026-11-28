import type {LocalMesh} from "./local-mesh";
export const DATASET_MAX_BYTES=16*1024*1024;
export const DATASET_MAX_TRIANGLES=10000;
export type MeshDataset={name:string;unit:string;kind:"scalar"|"vector";times:number[];frames:(number|null)[][];min:number;max:number};
const obj=(v:unknown):v is Record<string,unknown>=>!!v&&typeof v==="object"&&!Array.isArray(v);
const num=(v:unknown):v is number=>typeof v==="number"&&Number.isFinite(v)&&Math.abs(v)<=1e12;
/** Explicit node identities, UTC times, one global color scale. Vector datasets
 * render magnitude only; null nodes mark adjacent faces unavailable. */
export function parseMeshDataset(raw:unknown,mesh:LocalMesh):MeshDataset{
 if(!obj(raw)||raw.version!==1||typeof raw.name!=="string"||!raw.name.trim()||raw.name.length>120||typeof raw.unit!=="string"||raw.unit.length>32||!(raw.kind==="scalar"||raw.kind==="vector")||!Array.isArray(raw.node_ids)||!Array.isArray(raw.frames))throw Error("Invalid mesh dataset v1");
 if(mesh.triangles.length/3>DATASET_MAX_TRIANGLES||raw.frames.length<1||raw.frames.length>120||raw.frames.length*mesh.nodes.length>1000000)throw Error("Temporal mesh budget exceeded");
 const ids=raw.node_ids;if(ids.length!==mesh.nodeIds.length||new Set(ids).size!==ids.length||ids.some(x=>typeof x!=="number"||!Number.isSafeInteger(x)||x<1))throw Error("Dataset node IDs mismatch");
 const index=new Map(ids.map((id,i)=>[id,i])),order=mesh.nodeIds.map(id=>{const i=index.get(id);if(i===undefined)throw Error("Dataset missing mesh node");return i});
 const times:number[]=[],frames:(number|null)[][]=[];let min=Infinity,max=-Infinity;
 for(const f of raw.frames){
  if(!obj(f)||typeof f.time!=="string"||!/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{3})?Z$/.test(f.time)||!Array.isArray(f.values)||f.values.length!==ids.length)throw Error("Invalid dataset frame");
  const t=Date.parse(f.time),canonical=f.time.includes(".")?f.time:f.time.replace("Z",".000Z");
  if(!Number.isFinite(t)||new Date(t).toISOString()!==canonical||times.length&&t<=times[times.length-1]!)throw Error("Frames require strictly increasing valid UTC timestamps");
  times.push(t);const values=f.values;
  frames.push(order.map(i=>{const v=values[i];if(v===null)return null;let z:number;
   if(raw.kind==="scalar"){if(!num(v))throw Error("Invalid scalar value");z=v}
   else{if(!Array.isArray(v)||v.length!==2||!v.every(num))throw Error("Invalid 2D vector");z=Math.hypot(v[0]!,v[1]!)}
   min=Math.min(min,z);max=Math.max(max,z);return z;
  }));
 }
 if(!Number.isFinite(min)||!Number.isFinite(max))throw Error("Dataset contains no valid value");
 return {name:raw.name,unit:raw.unit,kind:raw.kind,times,frames,min,max};
}
