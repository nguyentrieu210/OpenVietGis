import {uiConfirm} from "./ui-dialog";
import {registerCadCopilot,type CadAiState} from "./ai-copilot/cad-actions";
import {useEffect,useRef,useState} from "react";
import {createPortal} from "react-dom";
import {ApiClient,ApiError,loadCsrfToken} from "@openvietgis/client-sdk";
import {openCadEditor,isCadHandle,type CadEditorHandle} from "@openvietgis/spatial-workspace/cad-editor";
import {validLink,type CadLink} from "./TileFeaturePanel";
import {CadDraftExport,supportsCadDraft} from "./CadDraftExport";
import {CadDraftImport} from "./CadDraftImport";
export type CadFileVersion={id:string;filename:string;format:"dwg"|"dxf";version:number;byte_size:number;sha256:string;created_at:string};
async function digest(bytes:ArrayBuffer){return Array.from(new Uint8Array(await crypto.subtle.digest("SHA-256",bytes)),v=>v.toString(16).padStart(2,"0")).join("")}
export function CadEditorPanel({client,path,document,origin,writable,canEdit,close,expire,focusHandle,onNavigate,onBusy,onVerified}:{client:ApiClient;path:string;document:CadFileVersion;origin:string;writable:boolean;canEdit:boolean;close:(savedId?:string)=>void;onBusy?:(busy:boolean)=>void;onVerified?:(doc:CadFileVersion)=>void;expire:()=>void;focusHandle?:string|null;onNavigate?:(link:CadLink,mode:"gis"|"3d")=>void}){
 const mount=useRef<HTMLDivElement>(null),handle=useRef<CadEditorHandle|null>(null),current=useRef(document),permit=useRef(canEdit);
 permit.current=canEdit;
 const callbacks=useRef({onBusy,onVerified});callbacks.current={onBusy,onVerified};const engineStatus=useRef({dirty:false,saving:false});
 const publishBusy=()=>callbacks.current.onBusy?.(engineStatus.current.dirty||engineStatus.current.saving);
 const [attempt,setAttempt]=useState(0),[failed,setFailed]=useState(false),[elapsed,setElapsed]=useState(0);
 const [version,setVersion]=useState(document.version),[dirty,setDirty]=useState(false),[saving,setSaving]=useState(false),[ready,setReady]=useState(false),[message,setMessage]=useState("Đang tải OpenCADStudio và mở bản vẽ…");
 const root=path+"/"+document.id+"/versions";
 const [filename,setFilename]=useState(document.filename),filenameRef=useRef(filename);filenameRef.current=filename;
 useEffect(()=>{onBusy?.(dirty||saving)},[dirty,saving,onBusy]);
 const lookup=useRef<AbortController|null>(null),selectionSerial=useRef(0);
 const [supported,setSupported]=useState(false),[selectedHandle,setSelectedHandle]=useState<string|null>(null),[links,setLinks]=useState<CadLink[]>([]),[lookupMessage,setLookupMessage]=useState("");
 const aiState=useRef<CadAiState>({document_id:document.id,filename:document.filename,version:document.version,dirty:false,saving:false,writable:false,selected_handle:null});
 aiState.current={document_id:current.current.id,filename,version,dirty,saving,writable:writable&&canEdit,selected_handle:selectedHandle};
 useEffect(()=>{if(!ready)return;return registerCadCopilot(path,{state:()=>({...aiState.current}),select:value=>handle.current?.requestSelect(value)===true})},[path,ready]);
 const clearSelection=()=>{selectionSerial.current++;lookup.current?.abort();setLinks([]);setSelectedHandle(null);setLookupMessage("")};
 const selectHandle=(value:string|null)=>{
  const serial=++selectionSerial.current;lookup.current?.abort();setLinks([]);setSelectedHandle(value);setLookupMessage(value?"Đang tra liên kết GIS…":"");
  if(!value||!isCadHandle(value))return;
  if(!current.current.id)return;
  const revision=current.current.version,abort=new AbortController();lookup.current=abort;
  const getLinks=async(linkVersion:number)=>{
   const reply=await client.request<{items:CadLink[]}>(path+"/"+current.current.id+"/versions/"+linkVersion+"/links?handle="+encodeURIComponent(value),{signal:abort.signal,cache:"no-store"});
   return reply.items.filter(l=>validLink(l)&&l.document_id===current.current.id&&l.version===linkVersion&&l.handle.toUpperCase()===value.toUpperCase());
  };
  void (async()=>{
   let valid=await getLinks(revision);
   if(!valid.length&&revision>1&&current.current.format==="dxf"&&permit.current&&!abort.signal.aborted){
    valid=(await getLinks(revision-1)).filter(l=>l.original_revision&&supportsCadDraft(l));
   }
   if(abort.signal.aborted||selectionSerial.current!==serial||revision!==current.current.version)return;
   setLinks(valid);setLookupMessage(valid.length?"Đã tìm thấy "+valid.length+" liên kết GIS.":"Handle này chưa được định vị thành đối tượng GIS.");
  })().catch(e=>{if(abort.signal.aborted||selectionSerial.current!==serial)return;if(e instanceof ApiError&&e.status===401)expire();else setLookupMessage("Không tra được liên kết của phiên bản này.")});
 };
 useEffect(()=>{
  setReady(false);setDirty(false);setSaving(false);setFailed(false);setElapsed(0);setMessage("Đang đọc và kiểm tra bản vẽ…");
  const abort=new AbortController();const started=performance.now();const tick=setInterval(()=>setElapsed(Math.floor((performance.now()-started)/1000)),1000);
  const report=(text:string)=>{if(!abort.signal.aborted)setMessage(text)};
  void (async()=>{
   try{
    const r=document.version===0?await fetch(import.meta.env.BASE_URL+"cad/blank-r2018.dxf",{signal:abort.signal,cache:"force-cache"}):await client.requestRaw(root+"/"+document.version+"/download",{cache:"no-store",signal:abort.signal});
    if(!r.ok)throw Error("CAD file unavailable");const bytes=await r.arrayBuffer();if(document.version!==0&&(bytes.byteLength!==document.byte_size||await digest(bytes)!==document.sha256))throw Error("SHA-256 mismatch");
    if(abort.signal.aborted||!mount.current)return;
    report("Đang tải và khởi tạo engine CAD…");
    const h=await openCadEditor(mount.current,{origin,name:document.filename,bytes,readOnly:!writable},{
     onStage:()=>report("Engine sẵn sàng · đang mở bản vẽ…"),
     onSelection:value=>{if(!abort.signal.aborted)selectHandle(value)},
     onDirty:d=>{if(!abort.signal.aborted){engineStatus.current.dirty=d;publishBusy();setDirty(d)}},
     onSaving:s=>{if(!abort.signal.aborted){engineStatus.current.saving=s;publishBusy();setSaving(s)}},
     onError:m=>report(m),
     onSave:async(bytes,format)=>{
      if(abort.signal.aborted||!writable||!permit.current||format!==document.format)throw Error("CAD save denied");
      const old=current.current;if(old.version>=20)throw Error("CAD revision limit");
      const sha=await digest(bytes);await loadCsrfToken(client);
      if(abort.signal.aborted||!permit.current)throw Error("CAD save revoked");
      const name=old.version?old.filename:filenameRef.current.trim().replace(/\.dxf$/i,"")+".dxf";
      if(!/^[^/\\\x00-\x1f]{1,175}\.dxf$/i.test(name)&&old.version===0)throw Error("Tên bản vẽ không hợp lệ");
      const body=new FormData();body.set("file",new Blob([bytes]),name);if(old.version)body.set("expected_version",String(old.version));
      const saved=await client.request<CadFileVersion>(old.version?path+"/"+old.id+"/versions":path,{method:"POST",body,signal:abort.signal}).catch(e=>{if(e instanceof ApiError&&e.status===401&&!abort.signal.aborted)expire();throw e});
      if(!saved.id||(old.version>0&&saved.id!==old.id)||saved.filename!==name||saved.version!==old.version+1||saved.format!==old.format||saved.sha256!==sha||saved.byte_size!==bytes.byteLength)throw Error("Unexpected CAD save receipt");
      if(abort.signal.aborted)throw Error("CAD scope changed");
      current.current=saved;callbacks.current.onVerified?.(saved);setFilename(saved.filename);clearSelection();setVersion(saved.version);report("Đã lưu phiên bản "+saved.version+".");
     }
    },abort.signal);
    if(abort.signal.aborted){h.dispose();return}handle.current=h;setSupported(h.supportsSelection);
    if(focusHandle&&isCadHandle(focusHandle))h.requestSelect(focusHandle);
    clearInterval(tick);setReady(true);if(current.current.version>0)callbacks.current.onVerified?.(current.current);report(writable?"Có thể chỉnh sửa và lưu phiên bản mới.":"Chế độ chỉ đọc.");
   }catch(e){clearInterval(tick);if(!abort.signal.aborted){if(e instanceof ApiError&&e.status===401)expire();else{setFailed(true);report(e instanceof Error?e.message:"Không mở được editor hoặc không xác minh được bản vẽ.")}}}
  })();
  return()=>{clearInterval(tick);abort.abort();lookup.current?.abort();handle.current?.dispose();handle.current=null};
 },[client,root,document.version,origin,writable,attempt]);
 useEffect(()=>{if(writable&&!canEdit){handle.current?.dispose();close(current.current.id||undefined)}},[canEdit,writable]);
 useEffect(()=>{if(!dirty&&!saving)return;const warn=(e:BeforeUnloadEvent)=>{e.preventDefault();e.returnValue=""};window.addEventListener("beforeunload",warn);return()=>window.removeEventListener("beforeunload",warn)},[dirty,saving]);
 const toolbar=<div className="ovg-cad-inline-toolbar"><button disabled={saving} onClick={async ()=>{if(engineStatus.current.saving)return;if(!engineStatus.current.dirty||(await uiConfirm("Thoát và bỏ phần chỉnh sửa chưa lưu?"))){if(!engineStatus.current.saving)close(current.current.id||undefined)}}}>Bản vẽ</button>{version===0?<input aria-label="Tên bản vẽ" value={filename} maxLength={175} disabled={saving} onChange={e=>setFilename(e.target.value)}/>:<strong>{filename}</strong>}<small>{version?"v"+version:"Bản nháp"}</small><span className="ovg-cad-status" role="status" title={message}>{saving?"Đang lưu…":dirty?"Chưa lưu":!ready?message:message.startsWith("Đã lưu")?message:!writable?"Chỉ đọc":message.startsWith("Có thể chỉnh sửa")?"Sẵn sàng":message}</span><button disabled={!ready||!writable||!canEdit||saving||version>=20} onClick={()=>handle.current?.requestSave()}>Lưu</button><details className="ovg-cad-about"><summary>⋯</summary><div><strong>Giới thiệu CAD</strong><small>OpenVietGIS CAD · OpenVietCAD</small>{origin==="https://cad.gpcds.site"&&<a href={origin+"/ovg-cad-2bc3537/NOTICE.txt"} target="_blank" rel="noopener noreferrer">Giấy phép GPL / mã nguồn</a>}</div></details></div>;
 const slot=window.document.getElementById("ovg-workspace-topbar")??window.document.getElementById("ovg-cad-toolbar-slot");
 return <section className="ovg-cad-editor" data-ovg-cad-dirty={dirty||saving?true:undefined} data-ovg-cad-saving={saving?true:undefined}>{slot?createPortal(toolbar,slot):toolbar}<div className="ovg-cad-engine"><div ref={mount} style={{width:"100%",height:"100%"}}/>{!ready&&<div className="ovg-cad-loading" role="status"><b>{failed?"Không thể mở CAD":"Đang mở CAD"}</b><small>{message}</small>{!failed&&<><progress aria-label="Đang tải CAD"/><small>{elapsed} giây · Lần mở đầu tiên cần tải engine WASM.</small></>}{failed&&<button onClick={()=>setAttempt(v=>v+1)}>Thử lại</button>}<button onClick={()=>close(current.current.id||undefined)}>Về bản vẽ</button></div>}</div>
 {supported&&<aside className="ovg-cad-selection" aria-label="Lựa chọn CAD trực tiếp"><strong>CAD ↔ GIS ↔ 3D</strong>
 {!ready?<small>Đang kiểm tra engine…</small>:!supported?<small>Bản OpenCAD Studio này chưa phát lựa chọn CAD trực tiếp. Về kho CAD để tra handle thủ công.</small>:
 <small>{selectedHandle?"Handle đã chọn: "+selectedHandle:"Chọn một đối tượng trên bản vẽ CAD để tra liên kết GIS."}</small>}
 {lookupMessage&&<small role="status">{lookupMessage}</small>}
 {links.map(l=><div key={l.document_id+":"+l.version+":"+l.layer_id+":"+l.handle+":"+l.feature_id}><span>{l.layer_name} · {l.feature_id} · {l.handle}{!l.original_revision?" · dữ liệu GIS đã thay đổi":""}</span>
 <button disabled={dirty||saving||!ready||version!==l.version||!l.original_revision} onClick={()=>onNavigate?.(l,"gis")}>Mở trên GIS</button>
 <button disabled={dirty||saving||!ready||version!==l.version||!l.original_revision} onClick={()=>onNavigate?.(l,"3d")}>Mở trên 3D</button>{canEdit&&ready&&!dirty&&!saving&&version===l.version&&current.current.format==="dxf"&&supportsCadDraft(l)&&<CadDraftExport key={[l.conversion_id,l.content_revision,l.schema_revision].join(":")} client={client} path={path} document={current.current} link={l} expire={expire}/>}
 {canEdit&&ready&&!dirty&&!saving&&version===l.version+1&&current.current.format==="dxf"&&l.original_revision&&supportsCadDraft(l)&&<CadDraftImport key={[l.conversion_id,current.current.sha256].join(":")} client={client} path={path} document={current.current} link={l} expire={expire}/>}</div>)}
 {(dirty||saving)&&!!links.length&&<small>Cần lưu hoặc hủy thay đổi CAD trước khi chuyển chế độ.</small>}
 </aside>}</section>
}
