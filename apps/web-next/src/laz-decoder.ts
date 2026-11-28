import createLazPerf from "laz-perf/lib/worker/laz-perf.js";
import wasmUrl from "laz-perf/lib/worker/laz-perf.wasm?url";
import {LAS_MAX_BYTES,parseLasRecords,type LasCloud} from "./local-las";
type Reader={open(p:number,n:number):void;getCount():number;getPointLength():number;getPointFormat():number;getPoint(p:number):void;delete():void};
type ChunkReader={open(format:number,length:number,p:number):void;getPoint(p:number):void;delete():void};
export type LazModule={_malloc(n:number):number;_free(p:number):void;HEAPU8:Uint8Array;LASZip:new()=>Reader;ChunkDecoder:new()=>ChunkReader};
let pending:Promise<LazModule>|undefined;
export async function decoder(){
 if(!pending)pending=Promise.resolve((createLazPerf as unknown as (options:{locateFile:(name:string)=>string})=>Promise<LazModule>)({locateFile:()=>wasmUrl})).catch(e=>{pending=undefined;throw e});
 return pending;
}
export function lazHeader(bytes:ArrayBuffer){
 if(bytes.byteLength<227||bytes.byteLength>LAS_MAX_BYTES)throw Error("LAZ file size limit");
 const d=new DataView(bytes);if(d.getUint32(0)!==0x4c415346||d.getUint8(24)!==1||![2,3,4].includes(d.getUint8(25)))throw Error("Unsupported LAZ header");
 const minor=d.getUint8(25),header=d.getUint16(94,true),offset=d.getUint32(96,true),raw=d.getUint8(104),format=raw&63,length=d.getUint16(105,true),legacy=d.getUint32(107,true);
 const minLengths:Record<number,number>={0:20,1:28,2:26,3:34,6:30,7:36,8:38};
 if(header<(minor===4?375:minor===3?235:227)||header>offset||offset>=bytes.byteLength||!(raw&128)||raw&64||!Object.hasOwn(minLengths,format)||length<minLengths[format]!||length>512||format>=6&&minor!==4)throw Error("Unsupported LAZ point records");
 const extended=minor===4?d.getBigUint64(247,true):0n;
 if(extended>2000000n||extended&&legacy&&extended!==BigInt(legacy)||format>=6&&(legacy!==0||extended===0n))throw Error("Invalid LAZ count");
 const count=extended?Number(extended):legacy;
 if(count<1||count>2000000||count*length>LAS_MAX_BYTES)throw Error("LAZ decompressed point budget");
 return {count,format,length,offset};
}
export async function decodeLaz(bytes:ArrayBuffer,crs:4326|3857):Promise<LasCloud>{
 return decodeLazWith(bytes,crs,await decoder());
}
export function decodeLazWith(bytes:ArrayBuffer,crs:4326|3857,mod:LazModule):LasCloud{
 const {count,format,length}=lazHeader(bytes);
 let src=0,dest=0,reader:Reader|undefined;
 try{
  src=mod._malloc(bytes.byteLength);dest=mod._malloc(length);if(!src||!dest)throw Error("LAZ allocation failed");
  mod.HEAPU8.set(new Uint8Array(bytes),src);reader=new mod.LASZip();reader.open(src,bytes.byteLength);
  if(reader.getCount()!==count||reader.getPointLength()!==length||reader.getPointFormat()!==format)throw Error("LAZ header/decoder mismatch");
  const raw=new Uint8Array(count*length);
  for(let i=0;i<count;i++){reader.getPoint(dest);raw.set(mod.HEAPU8.subarray(dest,dest+length),i*length)}
  return parseLasRecords(bytes,crs,raw.buffer);
 }finally{reader?.delete();if(dest)mod._free(dest);if(src)mod._free(src)}
}
export async function decodeLazChunk(bytes:ArrayBuffer,count:number,format:number,length:number):Promise<ArrayBuffer>{
 if(!Number.isInteger(count)||count<1||count>500000||!Number.isInteger(length)||length<30||length>512||![6,7,8].includes(format)||count*length>LAS_MAX_BYTES||!bytes.byteLength||bytes.byteLength>LAS_MAX_BYTES)throw Error("COPC chunk budget");
 return decodeLazChunkWith(bytes,count,format,length,await decoder());
}
export function decodeLazChunkWith(bytes:ArrayBuffer,count:number,format:number,length:number,mod:LazModule):ArrayBuffer{
 if(!Number.isInteger(count)||count<1||count>500000||!Number.isInteger(length)||length<30||length>512||![6,7,8].includes(format)||count*length>LAS_MAX_BYTES||!bytes.byteLength||bytes.byteLength>LAS_MAX_BYTES)throw Error("COPC chunk budget");
 let src=0,dest=0,reader:ChunkReader|undefined;
 try{src=mod._malloc(bytes.byteLength);dest=mod._malloc(length);if(!src||!dest)throw Error("COPC allocation failed");
  mod.HEAPU8.set(new Uint8Array(bytes),src);reader=new mod.ChunkDecoder();reader.open(format,length,src);
  const raw=new Uint8Array(count*length);for(let i=0;i<count;i++){reader.getPoint(dest);raw.set(mod.HEAPU8.subarray(dest,dest+length),i*length)}return raw.buffer;
 }finally{reader?.delete();if(dest)mod._free(dest);if(src)mod._free(src)}
}
