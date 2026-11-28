import type {LasCloud,LasPoint} from "./local-las";
/** Bounded local EPT binary subset. See https://entwine.io/en/latest/entwine-point-tile.html */
const ROOT="0-0-0-0",MAX_BYTES=64*1024*1024,MAX_POINTS=2000000,MAX_NODES=1024,R=6378137;
type LocalEntry={size:number;text():Promise<string>;arrayBuffer():Promise<ArrayBuffer>};
const obj=(v:unknown):v is Record<string,unknown>=>!!v&&typeof v==="object"&&!Array.isArray(v);
const finite=(v:unknown):v is number=>typeof v==="number"&&Number.isFinite(v);
function node(key:string){if(!/^(0|[1-9]\d{0,1})-(0|[1-9]\d{0,7})-(0|[1-9]\d{0,7})-(0|[1-9]\d{0,7})$/.test(key))throw Error("Invalid EPT key");const [d,x,y,z]=key.split("-").map(Number) as [number,number,number,number];if(d>24||[x,y,z].some(n=>n>=2**d))throw Error("Invalid EPT position");return [d,x,y,z] as const}
function within(key:string,root:string){const [d,x,y,z]=node(key),[rd,rx,ry,rz]=node(root),f=2**(d-rd);return d>=rd&&Math.floor(x/f)===rx&&Math.floor(y/f)===ry&&Math.floor(z/f)===rz}
type Dim={type:string;size:number;offset:number;scale:number;shift:number};
function read(d:DataView,at:number,f:Dim){let v:number;const p=at+f.offset;
 if(f.type==="float")v=f.size===4?d.getFloat32(p,true):d.getFloat64(p,true);
 else if(f.size===8){v=Number(f.type==="signed"?d.getBigInt64(p,true):d.getBigUint64(p,true));if(!Number.isSafeInteger(v))throw Error("EPT integer exceeds exact range")}
 else if(f.type==="signed")v=f.size===1?d.getInt8(p):f.size===2?d.getInt16(p,true):d.getInt32(p,true);
 else v=f.size===1?d.getUint8(p):f.size===2?d.getUint16(p,true):d.getUint32(p,true);
 return v*f.scale+f.shift;
}
export function eptDirectory(files:readonly File[]):Map<string,File>{
 if(!files.length||files.length>4096)throw Error("EPT directory file limit");
 const map=new Map<string,File>();let root:string|undefined;
 for(const file of files){const parts=file.webkitRelativePath.split("/");if(parts.length<2||parts.some(p=>!p||p==="."||p===".."||p.includes("\\"))||root!==undefined&&root!==parts[0])throw Error("Choose one EPT directory");root=parts.shift();const path=parts.join("/");if(map.has(path))throw Error("Duplicate EPT path");map.set(path,file)}
 return map;
}
export async function parseEpt(files:ReadonlyMap<string,LocalEntry>,crs:4326|3857):Promise<LasCloud>{
 if(crs!==4326&&crs!==3857)throw Error("Unsupported EPT CRS");
 const json=async(path:string)=>{const f=files.get(path);if(!f||f.size<1||f.size>1024*1024)throw Error("Missing or oversized EPT JSON: "+path);return JSON.parse(await f.text()) as unknown};
 const meta=await json("ept.json");
 if(!obj(meta)||typeof meta.version!=="string"||!/^1\.\d+\.\d+$/.test(meta.version)||meta.dataType!=="binary"||(meta.hierarchyType??"json")!=="json"||!Array.isArray(meta.schema)||meta.schema.length<3||meta.schema.length>64||!finite(meta.points)||!Number.isSafeInteger(meta.points)||meta.points<1||meta.points>MAX_POINTS||!Array.isArray(meta.bounds)||meta.bounds.length!==6||!meta.bounds.every(finite))throw Error("Only bounded EPT 1.x binary / JSON hierarchy is supported");
 const bounds=meta.bounds as [number,number,number,number,number,number],total=meta.points;if(bounds.slice(0,3).some((v,i)=>v>bounds[i+3]!))throw Error("Invalid EPT bounds");
 if(meta.srs!==undefined){if(!obj(meta.srs))throw Error("Invalid EPT CRS metadata");const s=meta.srs;if(Object.keys(s).length&&(s.authority!=="EPSG"||String(s.horizontal)!==String(crs)||s.vertical!==undefined))throw Error("EPT CRS mismatch or unsupported vertical CRS");}
 const dims=new Map<string,Dim>();let stride=0;
 for(const raw of meta.schema){if(!obj(raw)||typeof raw.name!=="string"||!raw.name||dims.has(raw.name)||!finite(raw.size)||!["signed","unsigned","float"].includes(String(raw.type))||!(raw.type==="float"?[4,8]:[1,2,4,8]).includes(raw.size))throw Error("Unsupported EPT dimension");
  const scale=raw.scale??1,shift=raw.offset??0;if(!finite(scale)||scale===0||!finite(shift))throw Error("Invalid EPT dimension transform");
  dims.set(raw.name,{type:String(raw.type),size:raw.size,offset:stride,scale,shift});stride+=raw.size;
 }
 const xyz=["X","Y","Z"].map(k=>{const d=dims.get(k);if(!d)throw Error("Missing EPT dimension "+k);return d});
 if(stride*total>MAX_BYTES)throw Error("EPT data exceeds 64 MiB");
 const counts=new Map<string,number>(),pages=new Set<string>();
 async function walk(root:string){if(pages.has(root)||pages.size>=MAX_NODES)throw Error("EPT hierarchy cycle or page limit");pages.add(root);
  const page=await json("ept-hierarchy/"+root+".json");if(!obj(page)||!Object.hasOwn(page,root)||!finite(page[root])||Number(page[root])<1)throw Error("Invalid EPT hierarchy page");
  for(const [k,v] of Object.entries(page)){if(!within(k,root)||!finite(v)||!Number.isSafeInteger(v)||v===0||v< -1||v>MAX_POINTS)throw Error("Invalid EPT hierarchy entry");if(v===-1){await walk(k)}else{if(counts.has(k)||counts.size>=MAX_NODES)throw Error("Duplicate or excessive EPT node");counts.set(k,v)}}
 }
 await walk(ROOT);
 for(const k of counts.keys()){const [d,x,y,z]=node(k);if(d>0&&!counts.has([d-1,Math.floor(x/2),Math.floor(y/2),Math.floor(z/2)].join("-")))throw Error("Missing EPT parent")}
 const count=[...counts.values()].reduce((s,n)=>s+n,0);if(count!==total)throw Error("EPT hierarchy does not match total points");
 const points:LasPoint[]=[],step=Math.max(1,Math.ceil(count/100000)),intensity=dims.get("Intensity");let n=0,west=Infinity,east=-Infinity,south=Infinity,north=-Infinity;
 for(const [key,num] of [...counts.entries()].sort(([a],[b])=>a.localeCompare(b))){const file=files.get("ept-data/"+key+".bin");if(!file||file.size!==num*stride)throw Error("Missing or truncated EPT tile: "+key);
  const data=new DataView(await file.arrayBuffer());if(data.byteLength!==file.size)throw Error("EPT tile size changed");
  for(let i=0;i<num;i++,n++){let [x,y,z]=xyz.map(f=>read(data,i*stride,f)) as [number,number,number];
   if(![x,y,z].every(finite)||[x,y,z].some((v,j)=>v<bounds[j]!-1e-6||v>bounds[j+3]!+1e-6))throw Error("EPT coordinate outside bounds");
   if(crs===3857){if(Math.abs(x)>Math.PI*R||Math.abs(y)>Math.PI*R)throw Error("EPT coordinate outside Mercator domain");x=x/R*180/Math.PI;y=Math.atan(Math.sinh(y/R))*180/Math.PI}
   if(Math.abs(x)>180||Math.abs(y)>85||z< -12000||z>100000)throw Error("EPT coordinate outside preview domain");
   west=Math.min(west,x);east=Math.max(east,x);south=Math.min(south,y);north=Math.max(north,y);
   if(n%step===0){const v=intensity?read(data,i*stride,intensity):0;if(!finite(v)||v<0||v>65535)throw Error("Invalid EPT intensity");points.push({longitude:x,latitude:y,height:z,intensity:v})}
  }
 }
 if(n!==count||east-west>180)throw Error("EPT incomplete or antimeridian crossing");
 return {points,count,bounds:[west,south,east,north],center:[(west+east)/2,(south+north)/2]};
}
