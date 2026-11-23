/**
 * Host boundary for the dedicated-origin OpenCAD relay. Only file bytes cross the
 * boundary: no GIS token, cookies, project identifiers or API URLs.
 */
export type CadFormat="dwg"|"dxf";
export type CadEditorHandle={requestSave():void;requestSelect(handle:string):boolean;readonly supportsSelection:boolean;dispose():void};
type OpenInput={origin:string;name:string;bytes:ArrayBuffer;readOnly:boolean};
type Events={
 onDirty(dirty:boolean):void;
 onSaving(saving:boolean):void;
 onSave(bytes:ArrayBuffer,format:CadFormat):Promise<void>;
 onError(message:string):void;
 onSelection?(handle:string|null):void;
};
const max=25*1024*1024;
const caps=["dwg","dxf","readOnly","save","saveId","dirty"];
export const isCadHandle=(v:unknown):v is string=>typeof v==="string"&&/^[0-9a-fA-F]{1,32}$/.test(v);
export function cadRelayOrigin(value:unknown):string|null{
 if(typeof value!=="string"||!value)return null;
 try{const u=new URL(value);return u.origin===value&&u.origin!==location.origin&&(u.protocol==="https:"||(u.protocol==="http:"&&u.hostname==="127.0.0.1"))?u.origin:null}catch{return null}
}
export function openCadEditor(host:HTMLElement,input:OpenInput,events:Events,signal:AbortSignal):Promise<CadEditorHandle>{
 const origin=cadRelayOrigin(input.origin);
 if(!origin||signal.aborted||!/^[^/\\\x00-\x1f\x7f]{1,175}\.(dwg|dxf)$/i.test(input.name)||!(input.bytes instanceof ArrayBuffer)||input.bytes.byteLength<1||input.bytes.byteLength>max)return Promise.reject(Error("Invalid CAD editor configuration or file"));
 const format=input.name.split(".").at(-1)!.toLowerCase() as CadFormat;
 const channel=Array.from(crypto.getRandomValues(new Uint8Array(16)),b=>b.toString(16).padStart(2,"0")).join("");
 const frame=document.createElement("iframe");
 frame.title="OpenCAD Studio · "+input.name;
 frame.referrerPolicy="no-referrer";frame.setAttribute("sandbox","allow-scripts allow-same-origin");
 frame.style.cssText="display:block;width:100%;height:100%;border:0;background:#1e1e1e";
 let disposed=false,ready=false,loaded=false,saving=false,requested=false,loads=0,selectionSupported=false;
 const seen=new Set<string>();
 let resolveReady:(handle:CadEditorHandle)=>void,rejectReady:(error:Error)=>void;
 const promise=new Promise<CadEditorHandle>((resolve,reject)=>{resolveReady=resolve;rejectReady=reject});
 const send=(type:string,data:Record<string,unknown>={},transfer:Transferable[]=[])=>frame.contentWindow?.postMessage({source:"ovg-cad-host",channel,type,...data},origin,transfer);
 const timer=setTimeout(()=>fail("CAD editor did not open in time"),120000);
 let saveTimer:ReturnType<typeof setTimeout>|undefined;
 function clearRequest(){requested=false;clearTimeout(saveTimer)}
 function dispose(){if(disposed)return;disposed=true;clearTimeout(timer);clearRequest();signal.removeEventListener("abort",abort);window.removeEventListener("message",receive);frame.remove()}
 function fail(message:string){const wasLoaded=loaded;dispose();if(wasLoaded){events.onError(message);events.onSaving(false)}else rejectReady(Error(message))}
 function abort(){fail("CAD editor closed")}
 const handle:CadEditorHandle={dispose,get supportsSelection(){return selectionSupported},requestSelect(value){
  if(disposed||!loaded||!selectionSupported||!isCadHandle(value)||value.length>16||!/[1-9a-f]/i.test(value))return false;
  send("select",{handle:value.toUpperCase()});return true;
 },requestSave(){
  if(disposed||!loaded||input.readOnly||saving||requested)return;
  requested=true;events.onSaving(true);send("request-save",{format});
  saveTimer=setTimeout(()=>{clearRequest();events.onSaving(false);events.onError("CAD chưa trả về bản lưu. Hãy thử lại.")},30000);
 }};
 async function save(data:{saveId:string;format:CadFormat;buffer:ArrayBuffer}){
  clearRequest();saving=true;seen.add(data.saveId);events.onSaving(true);events.onDirty(true);
  try{await events.onSave(data.buffer,data.format);if(!disposed)send("saved",{ok:true,saveId:data.saveId})}
  catch{if(!disposed){send("saved",{ok:false,saveId:data.saveId});events.onError("Không lưu được phiên bản CAD. Bản sửa vẫn chưa được lưu.")}}
  finally{saving=false;if(!disposed)events.onSaving(false)}
 }
 function receive(e:MessageEvent){
  if(disposed||e.origin!==origin||e.source!==frame.contentWindow)return;
  const d=e.data;if(!d||d.source!=="ovg-cad-relay"||d.channel!==channel)return;
  if(d.type==="ready"&&!ready&&d.version==="2026.46"&&Array.isArray(d.capabilities)&&caps.every(c=>d.capabilities.includes(c))){
   ready=true;selectionSupported=d.capabilities.includes("selection");send("open",{name:input.name,readOnly:input.readOnly,buffer:input.bytes},[input.bytes]);
  }else if(d.type==="loaded"&&ready&&!loaded&&d.name===input.name){
   loaded=true;clearTimeout(timer);resolveReady(handle);
  }else if(d.type==="selection"&&loaded&&selectionSupported&&(d.handle===null||isCadHandle(d.handle))){
   events.onSelection?.(d.handle===null?null:d.handle.toUpperCase());
  }else if(d.type==="dirty"&&ready&&typeof d.dirty==="boolean"){
   if(input.readOnly&&d.dirty){fail("Read-only CAD modified");return}
   events.onDirty(d.dirty||saving);
  }else if(d.type==="save"&&loaded&&typeof d.saveId==="string"&&/^save-[0-9]{1,20}$/.test(d.saveId)){
   if(seen.has(d.saveId))return;
   if(input.readOnly||saving||seen.size>=64||d.format!==format||!(d.buffer instanceof ArrayBuffer)||!d.buffer.byteLength||d.buffer.byteLength>max){send("saved",{ok:false,saveId:d.saveId});if(requested){clearRequest();events.onSaving(false)}return}
   void save(d);
  }else if(d.type==="error"){
   if(!loaded||d.code==="read_only")fail("CAD engine rejected drawing");
   else{if(requested){clearRequest();events.onSaving(false)}events.onError("CAD thông báo lỗi; kiểm tra bản vẽ trước khi tiếp tục.")}
  }
 }
 frame.addEventListener("load",()=>{if(++loads>1)fail("CAD iframe unexpectedly reloaded")});
 window.addEventListener("message",receive);signal.addEventListener("abort",abort,{once:true});
 frame.src=origin+"/relay.html#"+channel;host.append(frame);
 if(signal.aborted)abort();return promise;
}
