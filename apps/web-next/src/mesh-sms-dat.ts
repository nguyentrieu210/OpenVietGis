import type {LocalMesh} from "./local-mesh";
import {parseMeshDataset,DATASET_MAX_BYTES,type MeshDataset} from "./mesh-dataset";
/** SMS ASCII DAT node-centered scalar/vector subset. No cell-centered data,
 * active-face masks, binary DAT or implicit time origin. */
export function parseSmsDat(text:string,mesh:LocalMesh,origin:string,unit:string):MeshDataset{
 if(!text||text.length>DATASET_MAX_BYTES)throw Error("DAT file size exceeds limit");
 if(!/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{3})?Z$/.test(origin))throw Error("Supply the actual UTC time origin");
 const epoch=Date.parse(origin);if(!Number.isFinite(epoch)||new Date(epoch).toISOString()!==(origin.includes(".")?origin:origin.replace("Z",".000Z")))throw Error("Invalid UTC origin");
 const nodes=mesh.nodes.length;if(nodes<1||mesh.nodeIds.slice().sort((a,b)=>a-b).some((id,i)=>id!==i+1))throw Error("SMS DAT requires mesh node IDs 1 through ND");
 const lines=text.split(/\r?\n/).map(s=>s.trim()).filter(s=>s&&!s.startsWith("#"));
 let i=0;const take=()=>{const s=lines[i++];if(!s||s.length>512)throw Error("Truncated or oversized DAT record");return s};
 if(take()!=="DATASET"||!/^OBJTYPE\s+"mesh2d"$/i.test(take()))throw Error("Expected SMS DATASET mesh2d");
 const begin=take(),kind=begin==="BEGSCL"?"scalar":begin==="BEGVEC"?"vector":null;if(!kind)throw Error("Only BEGSCL/BEGVEC supported");
 let nd:number|undefined,nc:number|undefined,name="",factor:number|undefined;
 const num=(v:string)=>{if(!/^[-+]?(?:\d+(?:\.\d*)?|\.\d+)(?:e[-+]?\d+)?$/i.test(v))throw Error("Invalid DAT value");const n=Number(v);if(!Number.isFinite(n))throw Error("Non-finite DAT value");return n};
 const frames:{time:string;values:(number|number[]|null)[]}[]=[];
 let ended=false;
 while(i<lines.length){
  const line=take();if(line==="ENDDS"){ended=true;break}
  let m:RegExpExecArray|null;
  if(!frames.length&&(m=/^ND\s+(\d+)$/.exec(line))){if(nd!==undefined)throw Error("Duplicate ND");nd=Number(m[1]);continue}
  if(!frames.length&&(m=/^NC\s+(\d+)$/.exec(line))){if(nc!==undefined)throw Error("Duplicate NC");nc=Number(m[1]);continue}
  if(!frames.length&&(m=/^NAME\s+"([^"]{1,120})"$/.exec(line))){if(name)throw Error("Duplicate NAME");name=m[1]!;continue}
  if(!frames.length&&(m=/^TIMEUNITS\s+(\d+)$/.exec(line))){if(factor!==undefined)throw Error("Duplicate TIMEUNITS");factor=({0:1000,1:60000,2:3600000,3:86400000} as Record<number,number>)[Number(m[1])];if(!factor)throw Error("Unsupported TIMEUNITS");continue}
  if((m=/^TS\s+0\s+(\S+)$/.exec(line))){
   if(nd!==nodes||nc!==mesh.faceCount||!name||factor===undefined||frames.length>=120||(frames.length+1)*nodes>1000000)throw Error("DAT header, mesh counts or frame budget mismatch");
   const offset=num(m[1]!)*factor,time=epoch+offset;if(!Number.isSafeInteger(time)||Math.abs(time)>8640000000000000)throw Error("DAT timestamp outside supported range");
   const values:(number|number[]|null)[]=[];
   for(let j=0;j<nodes;j++){const a=take().split(/\s+/);if(a.length!==(kind==="scalar"?1:2))throw Error("DAT value arity mismatch");values.push(kind==="scalar"?num(a[0]!):a.map(num))}
   frames.push({time:new Date(time).toISOString(),values});continue
  }
  throw Error("Unsupported DAT record or active-face mask: "+line.slice(0,50));
 }
 if(!ended||i!==lines.length||!frames.length)throw Error("Incomplete or multiple DAT datasets");
 return parseMeshDataset({version:1,name,unit,kind,node_ids:Array.from({length:nodes},(_,i)=>i+1),frames},mesh);
}
