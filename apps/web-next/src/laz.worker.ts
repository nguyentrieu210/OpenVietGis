import type {CloudRegion} from "./cloud-region";
import {decodeLaz} from "./laz-decoder";
import {readRemoteCopc} from "./remote-copc";
type Request={kind:"file";bytes:ArrayBuffer;crs:4326|3857}|{kind:"copc";url:string;depth:number;crs:4326|3857;region?:CloudRegion};
const ctx=self as unknown as {onmessage:((e:MessageEvent<Request>)=>void)|null;postMessage:(data:unknown)=>void};
ctx.onmessage=async e=>{try{const r=e.data,cloud=r.kind==="copc"?await readRemoteCopc(r.url,r.crs,r.depth,new AbortController().signal,fetch,undefined,r.region):await decodeLaz(r.bytes,r.crs);ctx.postMessage({ok:true,cloud})}catch(error){ctx.postMessage({ok:false,error:error instanceof Error?error.message:String(error)})}};
