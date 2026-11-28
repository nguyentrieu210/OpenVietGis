/** COPC 1.0 selective HTTP-range preview. Runs inside the isolated LAZ worker.
 * Every request is public, credential-free, same URL, pinned by a strong ETag.
 * Hierarchy pages and LAZ chunks have disjoint validated ranges and finite budgets.
 */
import {decodeLazChunk} from "./laz-decoder";
import {nativeRegion,intersectsCloudNode,type CloudRegion} from "./cloud-region";
import {boundedParallel} from "./bounded-parallel";
import {parseLasRecords,LAS_MAX_BYTES,type LasCloud} from "./local-las";
export type CopcPreview=LasCloud&{datasetCount:number;nodeCount:number;complete:boolean};
const MAX_BYTES=32*1024*1024,MAX_POINTS=500000,MAX_NODES=128,MAX_REQUESTS=320;
const ascii=(d:DataView,at:number,n:number)=>String.fromCharCode(...new Uint8Array(d.buffer,d.byteOffset+at,n)).replace(/\0.*$/s,"").trim();
export function copcUrl(raw:string){
 const u=new URL(raw.trim());
 if(u.protocol!=="https:"||u.username||u.password||u.hash||u.search||u.port&&u.port!=="443"||!u.pathname.toLowerCase().endsWith(".copc.laz")||!u.hostname.includes(".")||/^(?:\d+(?:\.\d+){3}|\[.*\])$/.test(u.hostname)||/\.(?:local|internal|localhost|test|invalid)$/.test(u.hostname)||typeof location!=="undefined"&&u.origin===location.origin)throw Error("Use a public HTTPS .copc.laz URL without credentials or parameters");
 return u;
}
export function boundedRangeReader(url:URL,signal:AbortSignal,fetcher:typeof fetch=fetch){
 let size=0,etag="",transferred=0,requests=0;
 async function read(offset:number,length:number):Promise<ArrayBuffer>{
  signal.throwIfAborted();
  if(!Number.isSafeInteger(offset)||offset<0||!Number.isSafeInteger(length)||length<1||!Number.isSafeInteger(offset+length)||length>MAX_BYTES-transferred||size&&offset+length>size||++requests>MAX_REQUESTS)throw Error("COPC range budget");
  // Reserve the full range before any await, including concurrent reads.
  transferred+=length;
  const r=await fetcher(url,{method:"GET",headers:{Range:"bytes="+offset+"-"+(offset+length-1)},mode:"cors",credentials:"omit",referrerPolicy:"no-referrer",redirect:"error",cache:"no-store",signal});
  const fail=async(message:string)=>{await r.body?.cancel().catch(()=>{});throw Error(message)};
  const range=/^bytes (\d+)-(\d+)\/(\d+)$/.exec(r.headers.get("content-range")??""),tag=r.headers.get("etag")??"";
  if(r.status!==206||!range||!/^"[^"]+"$/.test(tag)||!r.body)return fail("COPC requires CORS-exposed Content-Range and strong ETag");
  const [a,b,total]=range.slice(1).map(Number);
  if(a!==offset||b!==offset+length-1||!Number.isSafeInteger(total)||total!<offset+length||size&&size!==total||etag&&etag!==tag)return fail("COPC changed or incorrect byte range");
  size=total!;etag=tag;
  const reader=r.body.getReader(),out=new Uint8Array(length);let at=0;
  try{while(true){const next=await reader.read();if(next.done)break;if(at+next.value.byteLength>length)throw Error("Oversized COPC response");out.set(next.value,at);at+=next.value.byteLength}}catch(e){await reader.cancel().catch(()=>{});throw e}finally{reader.releaseLock()}
  if(at!==length)throw Error("Truncated COPC range");
  return out.buffer;
 }
 return {read,get size(){return size},get remaining(){return MAX_BYTES-transferred}};
}
type Key=readonly [number,number,number,number];
type Entry={key:Key;offset:number;bytes:number;count:number};
const id=(k:Key)=>k.join("-");
const root:Key=[0,0,0,0];
const parent=(k:Key):Key=>[k[0]-1,Math.floor(k[1]/2),Math.floor(k[2]/2),Math.floor(k[3]/2)];
const within=(key:Key,p:Key)=>{const f=2**(key[0]-p[0]);return key[0]>=p[0]&&[1,2,3].every(i=>Math.floor(key[i]!/f)===p[i])};
const validKey=(key:Key)=>key[0]>=0&&key[0]<=24&&[key[1],key[2],key[3]].every(x=>x>=0&&x<2**key[0]);
function safe64(d:DataView,p:number){const x=d.getBigUint64(p,true);if(x>BigInt(Number.MAX_SAFE_INTEGER))throw Error("COPC 64-bit offset exceeds exact range");return Number(x)}
export function parseCopcHierarchy(bytes:ArrayBuffer,ancestor:Key):Entry[]{
 if(!bytes.byteLength||bytes.byteLength%32||bytes.byteLength>1024*1024)throw Error("Invalid COPC hierarchy page");
 const d=new DataView(bytes),out:Entry[]=[],seen=new Set<string>();
 for(let at=0;at<d.byteLength;at+=32){
  const key=[0,4,8,12].map(x=>d.getInt32(at+x,true)) as unknown as Key;
  const offset=safe64(d,at+16),size=d.getInt32(at+24,true),count=d.getInt32(at+28,true);
  if(!validKey(key)||!within(key,ancestor)||seen.has(id(key))||count< -1||count===0&&(offset!==0||size!==0)||count!==0&&(offset<589||size<=0)||count===-1&&(size%32||size>1024*1024))throw Error("Invalid COPC hierarchy entry");
  seen.add(id(key));out.push({key,offset,bytes:size,count});
 }
 if(!seen.has(id(ancestor)))throw Error("COPC hierarchy page has no root");
 return out;
}
export async function readRemoteCopc(raw:string,crs:4326|3857,depth:number,signal:AbortSignal,fetcher:typeof fetch=fetch,decode=decodeLazChunk,region?:CloudRegion):Promise<CopcPreview>{
 if(!Number.isInteger(depth)||depth<0||depth>12)throw Error("COPC depth must be 0–12");
 const reader=boundedRangeReader(copcUrl(raw),signal,fetcher),first=await reader.read(0,589),d=new DataView(first);
 if(ascii(d,0,4)!=="LASF"||d.getUint8(24)!==1||d.getUint8(25)!==4||d.getUint16(94,true)!==375||ascii(d,377,16)!=="copc"||d.getUint16(393,true)!==1||d.getUint16(395,true)!==160||!(d.getUint16(6,true)&16))throw Error("Invalid COPC 1.0 header");
 const format=d.getUint8(104)&63,length=d.getUint16(105,true),offset=d.getUint32(96,true),datasetCount=safe64(d,247);
 if(![6,7,8].includes(format)||d.getUint8(104)!==(128|format)||length<({6:30,7:36,8:38} as Record<number,number>)[format]!||length>512||offset<589||offset>1024*1024||offset>=reader.size||d.getUint32(107,true)!==0||datasetCount<1)throw Error("Unsupported COPC records");
 const rootOffset=safe64(d,469),rootBytes=safe64(d,477),evlr=safe64(d,235),evlrCount=d.getUint32(243,true);
 const info=[429,437,445,453,461].map(at=>d.getFloat64(at,true));
 if(!info.every(Number.isFinite)||info[3]!<=0||info[4]!<=0||evlr<=offset||evlr>=reader.size||!evlrCount||evlrCount>64)throw Error("Invalid COPC info or EVLR");
 // Verify the entire metadata header chain; reject projection EVLRs rather
 // than silently dropping conflicting CRS metadata during subset decoding.
 const [cx,cy,cz,half]=info as [number,number,number,number,number];
 const bounds=[cx-half,cy-half,cz-half,cx+half,cy+half,cz+half],window=nativeRegion(region,crs);
 const hierarchyRanges:{start:number;end:number}[]=[];let ev=evlr;
 for(let i=0;i<evlrCount;i++){
  const b=new DataView(await reader.read(ev,60)),user=ascii(b,2,16),record=b.getUint16(18,true),n=safe64(b,20),start=ev+60,end=start+n;
  if(!Number.isSafeInteger(end)||end>reader.size)throw Error("Invalid COPC EVLR extent");
  if(user==="LASF_Projection")throw Error("Remote COPC requires projection metadata in header VLRs");
  if(user==="copc"&&record===1000)hierarchyRanges.push({start,end});
  ev=end;
 }
 if(hierarchyRanges.length!==1)throw Error("Missing or ambiguous COPC hierarchy EVLR");
 const hr=hierarchyRanges[0]!;
 const prefix=new Uint8Array(offset);prefix.set(new Uint8Array(first));if(offset>589)prefix.set(new Uint8Array(await reader.read(589,offset-589)),589);
 const entries=new Map<string,Entry>(),pages=new Set<string>(),ranges:{start:number;end:number}[]=[];
 let complete=true;
 const claim=(start:number,n:number,lo:number,hi:number)=>{const end=start+n;if(!Number.isSafeInteger(end)||start<lo||end>hi||ranges.some(r=>r.start<end&&start<r.end))throw Error("COPC overlapping or invalid hierarchy/chunk extent");ranges.push({start,end})};
 async function page(key:Key,at:number,n:number){
  if(pages.has(id(key)))throw Error("COPC hierarchy cycle");
  if(pages.size>=MAX_NODES||n>reader.remaining){complete=false;return}
  if(n<32||n%32||n>1024*1024)throw Error("Invalid COPC page size");
  claim(at,n,hr.start,hr.end);pages.add(id(key));
  const rows=parseCopcHierarchy(await reader.read(at,n),key);
  for(const row of rows){
   if(row.key[0]>depth||!intersectsCloudNode(row.key,bounds,window)){complete=false;continue}
   if(row.count===-1){if(id(row.key)===id(key))throw Error("Self-referencing COPC hierarchy page");await page(row.key,row.offset,row.bytes)}
   else{if(entries.has(id(row.key)))throw Error("Duplicate COPC node");entries.set(id(row.key),row)}
  }
 }
 await page(root,rootOffset,rootBytes);
 if(!entries.has(id(root)))throw Error("COPC root hierarchy unavailable");
 const sorted=[...entries.values()].sort((a,b)=>a.key[0]-b.key[0]||id(a.key).localeCompare(id(b.key)));
 const selected=new Set<string>(),chunks:Uint8Array[]=[],planned:Entry[]=[];let count=0,nodeCount=0,reserved=0;
 for(const e of sorted){
  signal.throwIfAborted();
  if(e.key[0]>0&&!selected.has(id(parent(e.key)))){complete=false;continue}
  if(!e.count){selected.add(id(e.key));continue}
  if(nodeCount>=MAX_NODES||count+e.count>MAX_POINTS||(count+e.count)*length>LAS_MAX_BYTES||e.bytes>reader.remaining-reserved){complete=false;continue}
  claim(e.offset,e.bytes,offset,evlr);
  planned.push(e);reserved+=e.bytes;count+=e.count;nodeCount++;selected.add(id(e.key));
 }
 if(!count||count>datasetCount||complete&&count!==datasetCount)throw Error("COPC empty, oversized or inconsistent point selection");
 // Fetch at most four chunks concurrently, decode in deterministic order in
 // the same isolated WASM worker; do not retain a second raw chunk cache.
 for(let i=0;i<planned.length;i+=4){
  const batch=planned.slice(i,i+4);
  const loaded=await boundedParallel(batch,4,async e=>reader.read(e.offset,e.bytes),signal);
  for(let j=0;j<batch.length;j++){signal.throwIfAborted();const e=batch[j]!,raw=await decode(loaded[j]!,e.count,format,length);if(raw.byteLength!==e.count*length)throw Error("COPC decoder returned wrong record count");chunks.push(new Uint8Array(raw))}
 }
 const records=new Uint8Array(count*length);let pos=0;for(const chunk of chunks){records.set(chunk,pos);pos+=chunk.byteLength}
 // Preserve all original VLRs/scale/offset, pin the actual decoded subset.
 const h=new DataView(prefix.buffer);h.setBigUint64(235,0n,true);h.setUint32(243,0,true);h.setBigUint64(247,BigInt(count),true);
 const cloud=parseLasRecords(prefix.buffer,crs,records.buffer);
 return {...cloud,datasetCount,nodeCount,complete:complete&&count===datasetCount};
}
