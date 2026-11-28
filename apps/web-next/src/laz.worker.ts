import {decodeLaz} from "./laz-decoder";
const ctx=self as unknown as {onmessage:((e:MessageEvent<{bytes:ArrayBuffer;crs:4326|3857}>)=>void)|null;postMessage:(data:unknown)=>void};
ctx.onmessage=async e=>{try{ctx.postMessage({ok:true,cloud:await decodeLaz(e.data.bytes,e.data.crs)})}catch(error){ctx.postMessage({ok:false,error:error instanceof Error?error.message:String(error)})}};
