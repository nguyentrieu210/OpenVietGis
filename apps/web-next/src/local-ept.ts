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

/** Remote EPT preview: bounded breadth-first LOD, only selected nodes downloaded.
 * No server proxy, no credentials, no redirect, no arbitrary paths from metadata.
 * This is a user-selected depth preview rather than continuous camera LOD.
 */
export type RemoteEptPreview=LasCloud&{datasetCount:number;nodeCount:number;complete:boolean};
export function remoteEptUrl(raw:string){
 const url=new URL(raw.trim());
 if(url.protocol!=="https:"||url.username||url.password||url.search||url.hash||url.port&&url.port!=="443"||!url.pathname.endsWith("/ept.json")||!url.hostname.includes(".")||/^(?:\d+(?:\.\d+){3}|\[.*\])$/.test(url.hostname)||/\.(?:local|internal|localhost|test|invalid)$/.test(url.hostname)||url.hostname==="localhost"||typeof location!=="undefined"&&url.origin===location.origin)throw Error("Use a public HTTPS EPT manifest URL without credentials or parameters");
 return url;
}
export async function loadRemoteEpt(raw:string,crs:4326|3857,depth:number,signal:AbortSignal,fetcher:typeof fetch=fetch):Promise<RemoteEptPreview>{
 const url=remoteEptUrl(raw);
 if(!Number.isInteger(depth)||depth<0||depth>12)throw Error("Remote EPT depth must be 0–12");
 const base=new URL(".",url),files=new Map<string,Blob>(),MAX_REMOTE_BYTES=32*1024*1024,MAX_REMOTE_NODES=128,MAX_REMOTE_POINTS=500000;
 let transferred=0,requests=0;
 async function get(path:string,max:number){
  signal.throwIfAborted();
  const target=new URL(path,base);
  if(target.origin!==url.origin||!target.pathname.startsWith(base.pathname)||++requests>MAX_REMOTE_NODES*2+1)throw Error("Remote EPT request limit");
  const cap=Math.min(max,MAX_REMOTE_BYTES-transferred);if(cap<1)throw Error("Remote EPT byte budget");
  const r=await fetcher(target,{signal,mode:"cors",credentials:"omit",redirect:"error",referrerPolicy:"no-referrer",cache:"no-store"});
  if(!r.ok||r.status!==200||!r.body)throw Error("EPT requires public CORS-enabled files");
  const len=r.headers.get("content-length");if(len!==null&&(!/^\d+$/.test(len)||Number(len)>cap)){await r.body.cancel();throw Error("Remote EPT response exceeds limit")}
  const reader=r.body.getReader(),chunks:Uint8Array<ArrayBuffer>[]= [];let size=0;
  try{while(true){const next=await reader.read();if(next.done)break;size+=next.value.byteLength;if(size>cap)throw Error("Remote EPT response exceeds byte limit");chunks.push(new Uint8Array(next.value))}}catch(e){await reader.cancel().catch(()=>{});throw e}finally{reader.releaseLock()}
  if(size===0)throw Error("Empty EPT response");transferred+=size;
  return new Blob(chunks);
 }
 const metadata=await get("ept.json",1024*1024),meta:unknown=JSON.parse(await metadata.text());
 if(!obj(meta)||meta.dataType!=="binary"||(meta.hierarchyType??"json")!=="json"||!Array.isArray(meta.schema)||meta.schema.length<3||meta.schema.length>64||!finite(meta.points)||!Number.isSafeInteger(meta.points)||meta.points<1)throw Error("Only remote EPT binary/JSON is supported");
 const datasetCount=meta.points;
 const stride=meta.schema.reduce((sum:number,d:unknown)=>{if(!obj(d)||!finite(d.size)||![1,2,4,8].includes(d.size))throw Error("Invalid remote EPT schema");return sum+d.size},0);
 const queue=new Map<string,number>(),seenPages=new Set<string>(),seenNodes=new Set<string>();
 let complete=true;
 async function page(root:string){
  if(seenPages.has(root)||seenPages.size>=MAX_REMOTE_NODES)throw Error("EPT hierarchy page limit or cycle");
  seenPages.add(root);const f=await get("ept-hierarchy/"+root+".json",1024*1024),v:unknown=JSON.parse(await f.text());
  if(!obj(v)||!finite(v[root])||v[root]<1||Object.keys(v).length>65536)throw Error("Invalid EPT hierarchy");
  for(const [k,n] of Object.entries(v)){
   if(!within(k,root)||!finite(n)||!Number.isSafeInteger(n)||n===0||n< -1||n>datasetCount)throw Error("Invalid EPT hierarchy entry");
   if(node(k)[0]>depth){complete=false;continue}
   if(k===root||!seenNodes.has(k)){if(n===-1){if(k===root)throw Error("Invalid EPT page pointer");await page(k)}else{if(queue.has(k))throw Error("Duplicate EPT node");queue.set(k,n)}seenNodes.add(k)}
   else throw Error("Duplicate EPT node");
  }
 }
 await page(ROOT);
 const selected:Record<string,number>=Object.create(null);let count=0,nodes=0;
 const sorted=[...queue].sort(([a],[b])=>node(a)[0]-node(b)[0]||a.localeCompare(b));
 for(const [k,n] of sorted){
  signal.throwIfAborted();
  const [d,x,y,z]=node(k),parent=[d-1,Math.floor(x/2),Math.floor(y/2),Math.floor(z/2)].join("-");
  if(d>0&&!Object.hasOwn(selected,parent)||nodes>=MAX_REMOTE_NODES||count+n>MAX_REMOTE_POINTS||n*stride>MAX_REMOTE_BYTES-transferred){complete=false;continue}
  const path="ept-data/"+k+".bin",blob=await get(path,n*stride);if(blob.size!==n*stride)throw Error("Truncated EPT tile");
  selected[k]=n;files.set(path,blob);count+=n;nodes++;
 }
 if(!Object.hasOwn(selected,ROOT)||count>datasetCount||complete&&count!==datasetCount)throw Error("Invalid or oversized EPT root/point count");
 files.set("ept.json",new Blob([JSON.stringify({...meta,points:count})]));
 files.set("ept-hierarchy/"+ROOT+".json",new Blob([JSON.stringify(selected)]));
 const preview=await parseEpt(files,crs);
 return {...preview,datasetCount,nodeCount:nodes,complete:complete&&count===datasetCount};
}
