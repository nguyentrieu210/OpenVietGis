import {useEffect,useRef,useState} from "react";
import {ApiClient,ApiError,loadCsrfToken} from "@openvietgis/client-sdk";
import {openCadEditor,type CadEditorHandle} from "@openvietgis/spatial-workspace/cad-editor";
export type CadFileVersion={id:string;filename:string;format:"dwg"|"dxf";version:number;byte_size:number;sha256:string;created_at:string};
async function digest(bytes:ArrayBuffer){return Array.from(new Uint8Array(await crypto.subtle.digest("SHA-256",bytes)),v=>v.toString(16).padStart(2,"0")).join("")}
export function CadEditorPanel({client,path,document,origin,writable,canEdit,close,expire}:{client:ApiClient;path:string;document:CadFileVersion;origin:string;writable:boolean;canEdit:boolean;close:()=>void;expire:()=>void}){
 const mount=useRef<HTMLDivElement>(null),handle=useRef<CadEditorHandle|null>(null),current=useRef(document),permit=useRef(canEdit);
 permit.current=canEdit;
 const [version,setVersion]=useState(document.version),[dirty,setDirty]=useState(false),[saving,setSaving]=useState(false),[ready,setReady]=useState(false),[message,setMessage]=useState("Đang mở bản vẽ…");
 const root=path+"/"+document.id+"/versions";
 useEffect(()=>{
  const abort=new AbortController();
  const report=(text:string)=>{if(!abort.signal.aborted)setMessage(text)};
  void (async()=>{
   try{
    const r=await client.requestRaw(root+"/"+document.version+"/download",{cache:"no-store",signal:abort.signal});
    const bytes=await r.arrayBuffer();if(bytes.byteLength!==document.byte_size||await digest(bytes)!==document.sha256)throw Error("SHA-256 mismatch");
    if(abort.signal.aborted||!mount.current)return;
    const h=await openCadEditor(mount.current,{origin,name:document.filename,bytes,readOnly:!writable},{
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
      current.current=saved;setVersion(saved.version);report("Đã lưu phiên bản "+saved.version+".");
     }
    },abort.signal);
    if(abort.signal.aborted){h.dispose();return}handle.current=h;setReady(true);report(writable?"Có thể chỉnh sửa và lưu phiên bản mới.":"Chế độ chỉ đọc.");
   }catch(e){if(!abort.signal.aborted){if(e instanceof ApiError&&e.status===401)expire();else report("Không mở được editor hoặc không xác minh được bản vẽ.")}}
  })();
  return()=>{abort.abort();handle.current?.dispose();handle.current=null};
 },[client,root,document.version,origin,writable]);
 useEffect(()=>{if(writable&&!canEdit){handle.current?.dispose();close()}},[canEdit,writable]);
 useEffect(()=>{if(!dirty&&!saving)return;const warn=(e:BeforeUnloadEvent)=>{e.preventDefault();e.returnValue=""};window.addEventListener("beforeunload",warn);return()=>window.removeEventListener("beforeunload",warn)},[dirty,saving]);
 return <section className="ovg-cad-editor" data-ovg-cad-dirty={dirty||saving?true:undefined} data-ovg-cad-saving={saving?true:undefined}><header><strong>{document.filename} · v{version}</strong><span role="status">{saving?"Đang lưu…":dirty?"Chưa lưu · "+message:message}</span><button disabled={!ready||!writable||!canEdit||saving||version>=20} onClick={()=>handle.current?.requestSave()}>Lưu phiên bản</button><button disabled={saving} onClick={()=>{if(!dirty||window.confirm("Thoát và bỏ phần chỉnh sửa chưa lưu?"))close()}}>Về kho CAD</button></header><div ref={mount} className="ovg-cad-engine"/></section>
}
