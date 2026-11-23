import type {PreparedEngine} from "./coordinator";
export type CadReadOnlyInput=Readonly<{name:string;bytes:ArrayBuffer;engineReviewed:true}>;
const limit=25*1024*1024;
export function prepareCadReadOnly(host:HTMLElement,input:CadReadOnlyInput,signal:AbortSignal,onIntegrityFailure:()=>void):Promise<PreparedEngine>{
 if(input.engineReviewed!==true) return Promise.reject(Error("CAD deployment not reviewed"));
 if(!/^[^/\\\x00-\x1f]{1,180}\.(dwg|dxf)$/i.test(input.name)||!(input.bytes instanceof ArrayBuffer)||!input.bytes.byteLength||input.bytes.byteLength>limit)return Promise.reject(Error("Invalid CAD input"));
 if(signal.aborted)return Promise.reject(new DOMException("Aborted","AbortError"));
 // Only this pinned deployment path; never forward session tokens or credentials.
 const origin=location.origin,url=new URL("/cad-engine/app/?embed=1",origin);
 const frame=document.createElement("iframe");frame.title="CAD · chỉ đọc";frame.style.cssText="position:absolute;inset:0;width:100%;height:100%;border:0;visibility:hidden;pointer-events:none;z-index:5";
 frame.referrerPolicy="no-referrer";
 let disposed=false,active=false,loaded=false,opened=false,timer:ReturnType<typeof setTimeout>|undefined;
 let resolveReady:(engine:PreparedEngine)=>void,rejectReady:(err:Error)=>void;
 const ready=new Promise<PreparedEngine>((resolve,reject)=>{resolveReady=resolve;rejectReady=reject});
 const dispose=()=>{if(disposed)return;disposed=true;active=false;clearTimeout(timer);signal.removeEventListener("abort",abort);window.removeEventListener("message",receive);frame.remove()};
 const fail=(error:Error)=>{const wasActive=active;dispose();if(wasActive)onIntegrityFailure();else rejectReady(error)};
 const abort=()=>fail(new DOMException("Aborted","AbortError"));
 function receive(event:MessageEvent){
  if(disposed||event.origin!==origin||event.source!==frame.contentWindow||event.data?.source!=="opencadstudio-embed")return;
  const d=event.data;
  if(d.type==="ocs:ready"&&!opened){
   opened=true;const buffer=input.bytes.slice(0);
   frame.contentWindow?.postMessage({source:"opencadstudio-host",type:"ocs:open",name:input.name,buffer,readOnly:true},origin,[buffer]);
  }else if(d.type==="ocs:loaded"&&opened&&!loaded){
   if(d.name!==input.name){fail(Error("CAD reported unexpected document"));return}
   loaded=true;clearTimeout(timer);
   resolveReady({activate(){if(disposed||!loaded)throw Error("CAD unavailable");active=true;frame.style.visibility="visible";frame.style.pointerEvents="auto"},dispose});
  }else if(d.type==="ocs:error"||d.type==="ocs:save"||(d.type==="ocs:dirty"&&d.dirty!==false)){
   fail(Error("CAD read-only integrity failure"));
  }
 }
 timer=setTimeout(()=>fail(Error("CAD engine timeout")),60000);
 signal.addEventListener("abort",abort,{once:true});window.addEventListener("message",receive);
 frame.src=url.href;host.append(frame);if(signal.aborted)abort();
 return ready;
}
