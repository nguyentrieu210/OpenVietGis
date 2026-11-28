import {prepareSnapRecords,type SnapRecords} from "./snap-index-core";
self.onmessage=(e:MessageEvent<SnapRecords>)=>{
 try{const index=prepareSnapRecords(e.data);const transfer=[index.vertices,index.segments].flatMap(p=>[p.records.buffer,p.order.buffer,p.boxes.buffer,p.links.buffer] as ArrayBuffer[]);(self as unknown as {postMessage:(value:unknown,transfer:Transferable[])=>void}).postMessage({ok:true,index},transfer)}
 catch{self.postMessage({ok:false})}
};
