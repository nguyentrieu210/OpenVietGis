import React,{useEffect,useRef,useState} from "react";
import {getPluginV2Runtime,invokePluginV2,setPluginV2Grant,loadCsrfToken,type ApiClient,type PluginV2Runtime,type PluginV2Widget} from "@openvietgis/client-sdk";

export function DeclarativePluginPanel({client,org,project,manifest,canManage,onClose}:{client:ApiClient;org:string;project:string;manifest:string;canManage:boolean;onClose:()=>void}){
 const [runtime,setRuntime]=useState<PluginV2Runtime|null>(null),[values,setValues]=useState<Record<string,string>>({}),[output,setOutput]=useState(""),[error,setError]=useState(""),[busy,setBusy]=useState(false);
 const active=useRef<AbortController|null>(null),flight=useRef(false),generation=useRef(0),runtimeIdentity=useRef("");
 useEffect(()=>{
  const ctrl=new AbortController();active.current=ctrl;setRuntime(null);setValues({});setOutput("");setError("");setBusy(false);flight.current=false;runtimeIdentity.current="";
  const refresh=()=>{if(flight.current)return;const revision=++generation.current;void getPluginV2Runtime(client,org,project,manifest,ctrl.signal).then(r=>{if(!ctrl.signal.aborted&&!flight.current&&revision===generation.current){const identity=r.release_digest+":"+r.lock_digest+":"+r.grant_revision;if(runtimeIdentity.current!==identity||!r.granted)setOutput("");runtimeIdentity.current=identity;setRuntime(r);setError("")}}).catch(()=>{if(!ctrl.signal.aborted&&!flight.current&&revision===generation.current){setRuntime(null);setOutput("");setError("Plugin khai báo chưa được bật hoặc không còn đủ quyền.")}})};
  refresh();const timer=window.setInterval(refresh,15000);
  return()=>{window.clearInterval(timer);ctrl.abort();active.current=null};
 },[client,org,project,manifest]);
 const execute=async(actionId:string)=>{
  const ctrl=active.current;if(!runtime||!runtime.granted||!ctrl||ctrl.signal.aborted||flight.current)return;
  const action=runtime.document.actions.find(a=>a.id===actionId);
  if(!action||action.consent!=="read")return;
  const input:Record<string,unknown>=Object.create(null) as Record<string,unknown>;
  for(const [key,field] of Object.entries(action.input)){
   const value=Object.hasOwn(values,key)?values[key]:undefined;
   if(value===undefined||value===""){if(field.required){setError("Vui lòng nhập "+key);return}continue}
   if(field.kind==="text")input[key]=value;
   else if(field.kind==="boolean"){if(value!=="true"&&value!=="false"){setError("Giá trị boolean không hợp lệ");return}input[key]=value==="true"}
   else {const n=Number(value);if(!Number.isFinite(n)||(field.kind==="integer"&&!Number.isSafeInteger(n))){setError("Giá trị số không hợp lệ");return}input[key]=n}
  }
  ++generation.current;flight.current=true;setBusy(true);setError("");setOutput("");
  try{
   await loadCsrfToken(client);if(ctrl.signal.aborted)return;
   const result=await invokePluginV2(client,org,project,runtime,actionId,input,crypto.randomUUID(),ctrl.signal);
   if(!ctrl.signal.aborted)setOutput(JSON.stringify(result.output,null,2));
  }catch{if(!ctrl.signal.aborted){setError("Không thể thực hiện. Quyền hoặc phiên bản plugin có thể đã thay đổi; hãy đóng và mở lại.");setRuntime(null)}}
  finally{if(!ctrl.signal.aborted){flight.current=false;setBusy(false)}}
 };
 const changeGrant=async(enabled:boolean)=>{
  const ctrl=active.current;if(!canManage||!runtime||!ctrl||ctrl.signal.aborted||flight.current)return;
  ++generation.current;flight.current=true;setBusy(true);setError("");setOutput("");
  try{await loadCsrfToken(client);if(ctrl.signal.aborted)return;
   const result=await setPluginV2Grant(client,org,project,runtime,enabled,ctrl.signal);
   if(!ctrl.signal.aborted){runtimeIdentity.current=runtime.release_digest+":"+runtime.lock_digest+":"+result.grant_revision;setRuntime({...runtime,granted:result.granted,grant_revision:result.grant_revision})}
  }catch{if(!ctrl.signal.aborted)setError("Không thể cập nhật quyền. Hãy mở lại plugin.")}
  finally{if(!ctrl.signal.aborted){flight.current=false;setBusy(false)}}
 };
 const render=(nodes:PluginV2Widget[],depth=1):React.ReactNode=>depth>16?null:nodes.map(node=>{
  switch(node.kind){
   case "text":return <p key={node.id}>{node.text}</p>;
   case "group":return <div key={node.id}>{render(node.children,depth+1)}</div>;
   case "input":return <label key={node.id}>{node.label}<input maxLength={4096} value={Object.hasOwn(values,node.field)?values[node.field]:""} onChange={e=>setValues(old=>({...old,[node.field]:e.target.value}))}/></label>;
   case "button":return <button key={node.id} disabled={busy} onClick={()=>void execute(node.action)}>{node.label}</button>;
   default:return null;
  }
 });
 return <section className="partner-form" aria-label="Plugin khai báo v2"><button onClick={onClose}>Đóng plugin v2</button><p className="live-note">Runtime thử nghiệm · chỉ đọc · không chạy mã tải về.</p>{!runtime&&!error&&<p>Đang xác minh plugin…</p>}{runtime&&<div><p>Quyền thực thi: {runtime.granted?"Đã cấp":"Chưa cấp"}</p><ul>{runtime.permissions.map(p=><li key={p}>{p.endsWith(".map_view")?"Đọc tên lớp, loại hình học, số đối tượng và mã lớp trong dự án":p}</li>)}</ul>{runtime.dependency_lock.length>0&&<details><summary>Phiên bản đã khóa ({runtime.dependency_lock.length})</summary><ul>{runtime.dependency_lock.map(p=><li key={p.key}>{p.key} · {p.version}</li>)}</ul></details>}{canManage&&<button disabled={busy} onClick={()=>void changeGrant(!runtime.granted)}>{runtime.granted?"Thu hồi quyền":"Cấp quyền cho phiên bản này"}</button>}{runtime.granted&&render(runtime.document.widgets)}</div>}{error&&<p role="alert">{error}</p>}{output&&<pre aria-live="polite">{output}</pre>}</section>
}
