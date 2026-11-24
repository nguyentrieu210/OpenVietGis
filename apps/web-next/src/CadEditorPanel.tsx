import {useEffect,useRef,useState} from "react";
import {ApiClient,ApiError,loadCsrfToken} from "@openvietgis/client-sdk";
import {openCadEditor,isCadHandle,type CadEditorHandle} from "@openvietgis/spatial-workspace/cad-editor";
import {validLink,type CadLink} from "./TileFeaturePanel";
export type CadFileVersion={id:string;filename:string;format:"dwg"|"dxf";version:number;byte_size:number;sha256:string;created_at:string};
async function digest(bytes:ArrayBuffer){return Array.from(new Uint8Array(await crypto.subtle.digest("SHA-256",bytes)),v=>v.toString(16).padStart(2,"0")).join("")}
export function CadEditorPanel({client,path,document,origin,writable,canEdit,close,expire,focusHandle,onNavigate}:{client:ApiClient;path:string;document:CadFileVersion;origin:string;writable:boolean;canEdit:boolean;close:()=>void;expire:()=>void;focusHandle?:string|null;onNavigate?:(link:CadLink,mode:"gis"|"3d")=>void}){
 const mount=useRef<HTMLDivElement>(null),handle=useRef<CadEditorHandle|null>(null),current=useRef(document),permit=useRef(canEdit);
 permit.current=canEdit;
 const [version,setVersion]=useState(document.version),[dirty,setDirty]=useState(false),[saving,setSaving]=useState(false),[ready,setReady]=useState(false),[message,setMessage]=useState("Đang tải OpenCADStudio và mở bản vẽ…");
 const root=path+"/"+document.id+"/versions";
 const lookup=useRef<AbortController|null>(null),selectionSerial=useRef(0);
 const [supported,setSupported]=useState(false),[selectedHandle,setSelectedHandle]=useState<string|null>(null),[links,setLinks]=useState<CadLink[]>([]),[lookupMessage,setLookupMessage]=useState("");
 const clearSelection=()=>{selectionSerial.current++;lookup.current?.abort();setLinks([]);setSelectedHandle(null);setLookupMessage("")};
 const selectHandle=(value:string|null)=>{
  const serial=++selectionSerial.current;lookup.current?.abort();setLinks([]);setSelectedHandle(value);setLookupMessage(value?"Đang tra liên kết GIS…":"");
  if(!value||!isCadHandle(value))return;
  const revision=current.current.version,abort=new AbortController();lookup.current=abort;
  void client.request<{items:CadLink[]}>(root+"/"+revision+"/links?handle="+encodeURIComponent(value),{signal:abort.signal,cache:"no-store"}).then(reply=>{
   if(abort.signal.aborted||selectionSerial.current!==serial||revision!==current.current.version)return;
   const valid=reply.items.filter(l=>validLink(l)&&l.document_id===document.id&&l.version===revision&&l.handle.toUpperCase()===value.toUpperCase());
   setLinks(valid);setLookupMessage(valid.length?"Đã tìm thấy "+valid.length+" liên kết GIS.":"Handle này chưa được định vị thành đối tượng GIS.");
  }).catch(e=>{if(abort.signal.aborted||selectionSerial.current!==serial)return;if(e instanceof ApiError&&e.status===401)expire();else setLookupMessage("Không tra được liên kết của phiên bản này.")});
 };
 useEffect(()=>{
  const abort=new AbortController();
  const report=(text:string)=>{if(!abort.signal.aborted)setMessage(text)};
  void (async()=>{
   try{
    const r=await client.requestRaw(root+"/"+document.version+"/download",{cache:"no-store",signal:abort.signal});
    const bytes=await r.arrayBuffer();if(bytes.byteLength!==document.byte_size||await digest(bytes)!==document.sha256)throw Error("SHA-256 mismatch");
    if(abort.signal.aborted||!mount.current)return;
    const h=await openCadEditor(mount.current,{origin,name:document.filename,bytes,readOnly:!writable},{
     onSelection:value=>{if(!abort.signal.aborted)selectHandle(value)},
     onDirty:d=>{if(!abort.signal.aborted)setDirty(d)},
     onSaving:s=>{if(!abort.signal.aborted)setSaving(s)},
     onError:m=>report(m),
     onSave:async(bytes,format)=>{
      if(abort.signal.aborted||!writable||!permit.current||format!==document.format)throw Error("CAD save denied");
      const old=current.current;if(old.version>=20)throw Error("CAD revision limit");
      const sha=await digest(bytes);await loadCsrfToken(client);
      if(abort.signal.aborted||!permit.current)throw Error("CAD save revoked");
      const body=new FormData();body.set("file",new Blob([bytes]),old.filename);body.set("expected_version",String(old.version));
      const saved=await client.request<CadFileVersion>(root,{method:"POST",body,signal:abort.signal}).catch(e=>{if(e instanceof ApiError&&e.status===401&&!abort.signal.aborted)expire();throw e});
      if(saved.id!==old.id||saved.version!==old.version+1||saved.format!==old.format||saved.sha256!==sha||saved.byte_size!==bytes.byteLength)throw Error("Unexpected CAD save receipt");
      if(abort.signal.aborted)throw Error("CAD scope changed");
      current.current=saved;clearSelection();setVersion(saved.version);report("Đã lưu phiên bản "+saved.version+".");
     }
    },abort.signal);
    if(abort.signal.aborted){h.dispose();return}handle.current=h;setSupported(h.supportsSelection);
    if(focusHandle&&isCadHandle(focusHandle))h.requestSelect(focusHandle);
    setReady(true);report(writable?"Có thể chỉnh sửa và lưu phiên bản mới.":"Chế độ chỉ đọc.");
   }catch(e){if(!abort.signal.aborted){if(e instanceof ApiError&&e.status===401)expire();else report("Không mở được editor hoặc không xác minh được bản vẽ.")}}
  })();
  return()=>{abort.abort();lookup.current?.abort();handle.current?.dispose();handle.current=null};
 },[client,root,document.version,origin,writable]);
 useEffect(()=>{if(writable&&!canEdit){handle.current?.dispose();close()}},[canEdit,writable]);
 useEffect(()=>{if(!dirty&&!saving)return;const warn=(e:BeforeUnloadEvent)=>{e.preventDefault();e.returnValue=""};window.addEventListener("beforeunload",warn);return()=>window.removeEventListener("beforeunload",warn)},[dirty,saving]);
 return <section className="ovg-cad-editor" data-ovg-cad-dirty={dirty||saving?true:undefined} data-ovg-cad-saving={saving?true:undefined}><header><strong>{document.filename} · v{version}</strong><small>OpenCADStudio 2026.46</small>{origin==="https://cad.gpcds.site"&&<a href={origin+"/ovg-cad-2026.46/NOTICE.txt"} target="_blank" rel="noopener noreferrer">GPL / mã nguồn</a>}<span role="status">{saving?"Đang lưu…":dirty?"Chưa lưu · "+message:message}</span><button disabled={!ready||!writable||!canEdit||saving||version>=20} onClick={()=>handle.current?.requestSave()}>Lưu phiên bản</button><button disabled={saving} onClick={()=>{if(!dirty||window.confirm("Thoát và bỏ phần chỉnh sửa chưa lưu?"))close()}}>Về kho CAD</button></header><div ref={mount} className="ovg-cad-engine"/>
 <aside className="ovg-cad-selection" aria-label="Lựa chọn CAD trực tiếp"><strong>CAD ↔ GIS ↔ 3D</strong>
 {!ready?<small>Đang kiểm tra engine…</small>:!supported?<small>Bản OpenCAD Studio này chưa phát lựa chọn CAD trực tiếp. Về kho CAD để tra handle thủ công.</small>:
 <small>{selectedHandle?"Handle đã chọn: "+selectedHandle:"Chọn một đối tượng trên bản vẽ CAD để tra liên kết GIS."}</small>}
 {lookupMessage&&<small role="status">{lookupMessage}</small>}
 {links.map(l=><div key={l.document_id+":"+l.version+":"+l.layer_id+":"+l.handle+":"+l.feature_id}><span>{l.layer_name} · {l.feature_id} · {l.handle}{!l.original_revision?" · dữ liệu GIS đã thay đổi":""}</span>
 <button disabled={dirty||saving||!ready||!l.original_revision} onClick={()=>onNavigate?.(l,"gis")}>Mở trên GIS</button>
 <button disabled={dirty||saving||!ready||!l.original_revision} onClick={()=>onNavigate?.(l,"3d")}>Mở trên 3D</button></div>)}
 {(dirty||saving)&&!!links.length&&<small>Cần lưu hoặc hủy thay đổi CAD trước khi chuyển chế độ.</small>}
 </aside></section>
}
