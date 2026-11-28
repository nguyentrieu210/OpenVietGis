import {decodeNetcdf4} from "./netcdf4";
import type {LocalMesh} from "./local-mesh";
const ctx=self as unknown as {onmessage:((e:MessageEvent<{bytes:ArrayBuffer;mesh:LocalMesh;variable:string;componentY?:string}>)=>void)|null;postMessage(v:unknown):void};
ctx.onmessage=async e=>{try{const r=e.data;ctx.postMessage({ok:true,dataset:await decodeNetcdf4(r.bytes,r.mesh,r.variable,r.componentY)})}catch(error){ctx.postMessage({ok:false,error:error instanceof Error?error.message:String(error)})}};
