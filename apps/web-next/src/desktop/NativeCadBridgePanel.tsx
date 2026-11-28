import {useEffect,useRef,useState} from "react";
import {nativeCad,type CadEditorStatus,type CadVersion,type PersistentAsset} from "./transport";
export default function NativeCadBridgePanel({handle,revision,assets,onChanged}:{handle:string;revision:string;assets:PersistentAsset[];onChanged:()=>Promise<void>}){
 const [status,setStatus]=useState<Awaited<ReturnType<typeof nativeCad.status>>>(),[busy,setBusy]=useState(false),[message,setMessage]=useState(""),[editor,setEditor]=useState<CadEditorStatus>();
 const [versions,setVersions]=useState<CadVersion[]>([]);
 useEffect(()=>{let active=true;void nativeCad.versions(handle).then(v=>{if(active)setVersions(v)}).catch(e=>{if(active)setMessage(String(e))});return()=>{active=false}},[handle,revision]);
 const request=useRef<string|null>(null),alive=useRef(true),generation=useRef(0),lock=useRef(false);
 useEffect(()=>{alive.current=true;const g=++generation.current;
  void nativeCad.status().then(s=>{if(alive.current&&g===generation.current)setStatus(s)}).catch(e=>{if(alive.current)setMessage(String(e))});
  void nativeCad.editList(handle).then(s=>{if(alive.current&&g===generation.current)setEditor(s[0])}).catch(e=>{if(alive.current)setMessage(String(e))});
  return()=>{alive.current=false;generation.current++;if(request.current)void nativeCad.cancel(request.current).catch(()=>{})}
 },[handle]);
 useEffect(()=>{
  if(editor?.state!=="editing")return;
  let active=true,timer:ReturnType<typeof setTimeout>;
  const poll=async()=>{try{const s=await nativeCad.editPoll(handle,editor.session);if(!active)return;setEditor(s);if(s.state==="editing")timer=setTimeout(()=>void poll(),1500)}catch(e){if(active)setMessage(String(e))}};
  timer=setTimeout(()=>void poll(),1500);return()=>{active=false;clearTimeout(timer)}
 },[handle,editor?.session,editor?.state]);
 const run=async(f:()=>Promise<void>)=>{if(lock.current)return;lock.current=true;setBusy(true);setMessage("");try{await f()}catch(e){if(alive.current)setMessage(String(e))}finally{lock.current=false;if(alive.current)setBusy(false)}};
 const convert=()=>run(async()=>{if(!status?.available||!window.confirm("Chuyển đổi bản sao DWG/DXF? File gốc không bị thay đổi."))return;
  const id=crypto.randomUUID();request.current=id;try{const r=await nativeCad.convert(handle,id);if(r&&alive.current)setMessage("Đã xuất "+r.format.toUpperCase()+" · "+r.bytes+" byte · SHA-256 "+r.sha256)}finally{request.current=null}
 });
 const open=()=>run(async()=>{if(!status?.available||editor||!window.confirm("Mở bản sao CAD trong cửa sổ OpenVietCAD riêng. Lưu vào bản sao đang mở trước khi đóng editor; GIS sẽ yêu cầu nơi lưu kết quả."))return;
  const s=await nativeCad.editOpen(handle);if(s&&alive.current)setEditor(s);
 });
 const openStored=(asset:string)=>run(async()=>{if(editor||!status?.available||!window.confirm("Mở bản sao tài liệu CAD trong dự án?"))return;
  const s=await nativeCad.editProject(handle,asset,revision);if(alive.current)setEditor(s);
 });
 const adopt=()=>run(async()=>{if(!editor||editor.state==="editing"||(!editor.changed&&editor.parent))return;
  if(!window.confirm(editor.state==="recoverable"?"Lưu bản phục hồi chưa được nghiệm thu vào dự án?":"Lưu bản CAD mới vào dự án, giữ bản cha và không gán CRS tự động?"))return;
  const r=await nativeCad.adopt(handle,editor.session,revision,editor.sha256);
  if(alive.current){setEditor(undefined);setMessage("Đã lưu phiên bản CAD "+r.id);await onChanged();}
 });
 const save=()=>run(async()=>{if(!editor||editor.state==="editing"||!editor.changed)return;
  if(!window.confirm(editor.state==="recoverable"?"Editor kết thúc bất thường. Xuất bản sao phục hồi chưa được nghiệm thu?":"Xuất bản vẽ đã chỉnh sửa thành file mới?"))return;
  const r=await nativeCad.editSave(handle,editor.session,editor.sha256);if(r&&alive.current){setEditor(undefined);setMessage("Đã lưu bản sao "+r.format.toUpperCase()+" · SHA-256 "+r.sha256)}
 });
 const discard=()=>run(async()=>{if(!editor||editor.state==="editing"||!window.confirm("Bỏ bản sao chỉnh sửa? File gốc vẫn giữ nguyên."))return;await nativeCad.editDiscard(handle,editor.session);if(alive.current)setEditor(undefined)});
 return <details><summary>OpenVietCAD native editor</summary>
 <p>{status?.available?"Có sidecar CAD":"Chưa đóng gói sidecar CAD"} · {status?.source_revision.slice(0,12)}</p>
 <p>Editor riêng chỉnh sửa DWG/DXF; GIS quản lý bản sao và xuất kết quả. Không ghi đè file nguồn. Chưa nhúng canvas CAD vào bản đồ.</p>
 <button disabled={!status?.available||busy||!!editor} onClick={()=>void open()}>Mở editor DWG / DXF</button>
 {assets.filter(a=>a.kind==="cad-native-dwg"||a.kind==="cad-native-dxf").map(a=><div key={a.id}><span>{a.kind.endsWith("dwg")?"DWG":"DXF"} · {a.id.slice(0,8)} · cha: {versions.find(v=>v.asset===a.id)?.parent.slice(0,8)??"gốc"}</span> <button disabled={!status?.available||busy||!!editor} onClick={()=>void openStored(a.id)}>Chỉnh sửa phiên bản</button></div>)}
 <button disabled={!status?.available||busy} onClick={()=>void convert()}>Chuyển đổi DWG / DXF</button>
 {request.current&&busy&&<button onClick={()=>{if(request.current)void nativeCad.cancel(request.current).catch(()=>{})}}>Hủy chuyển đổi</button>}
 {editor&&<><p>{editor.state==="editing"?"Đang mở editor. Lưu và đóng cửa sổ CAD để nhận kết quả.":editor.state==="recoverable"?"Editor kết thúc bất thường; chỉ xuất phục hồi khi xác nhận.":editor.changed?"Có bản vẽ đã sửa.":"Bản vẽ chưa thay đổi."}</p>
 <button disabled={busy||editor.state==="editing"||(!editor.changed&&!!editor.parent)} onClick={()=>void adopt()}>Lưu phiên bản vào dự án</button>
 <button disabled={busy||editor.state==="editing"||!editor.changed} onClick={()=>void save()}>Lưu bản sao mới</button>
 <button disabled={busy||editor.state==="editing"} onClick={()=>void discard()}>Bỏ phiên chỉnh sửa</button>
 <button disabled={busy} onClick={()=>void run(async()=>setEditor(await nativeCad.editPoll(handle,editor.session)))}>Kiểm tra trạng thái</button></>}
 <p role="status">{message}</p></details>
}
