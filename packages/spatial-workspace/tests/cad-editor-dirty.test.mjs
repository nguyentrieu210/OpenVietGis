import {test} from "node:test";
import assert from "node:assert/strict";
import {openCadEditor} from "../src/cad-editor.ts";

test("CAD opening notifications never mark document dirty; actual post-load edits do",async()=>{
 const old={window:globalThis.window,document:globalThis.document,location:globalThis.location};
 const events=new EventTarget(),posted=[];
 const frame={contentWindow:{postMessage:(data)=>posted.push(data)},style:{},setAttribute(){},addEventListener(){},remove(){}};
 Object.assign(globalThis,{window:events,document:{createElement:()=>frame},location:{origin:"https://gis.example",pathname:"/app"}});
 const dirty=[],errors=[],controller=new AbortController();
 const origin="https://cad.example";
 const dispatch=(type,data={})=>{
   const e=new Event("message");
   Object.defineProperties(e,{origin:{value:origin},source:{value:frame.contentWindow},data:{value:{source:"ovg-cad-relay",channel:frame.src.split("#")[1],type,...data}}});
   events.dispatchEvent(e);
 };
 try{
 const promise=openCadEditor({append(){}},{origin,name:"test.dxf",bytes:new Uint8Array([1]).buffer,readOnly:false},{onDirty:v=>dirty.push(v),onSaving(){},onError:e=>errors.push(e),async onSave(){}},controller.signal);
 dispatch("ready",{version:"2026.46",capabilities:["dwg","dxf","readOnly","save","saveId","dirty"]});
 dispatch("dirty",{dirty:true});assert.deepEqual(dirty,[]);
 dispatch("loaded",{name:"test.dxf"});const handle=await promise;
 assert.deepEqual(dirty,[false]);
 dispatch("dirty",{dirty:true});assert.deepEqual(dirty,[false,true]);
 dispatch("dirty",{dirty:false});assert.deepEqual(dirty,[false,true,false]);
 assert.deepEqual(errors,[]);handle.dispose();
 }finally{controller.abort();Object.assign(globalThis,old)}
});
