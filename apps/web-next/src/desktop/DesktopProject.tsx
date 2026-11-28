import {useCallback,useEffect,useRef,useState} from "react";
import LocalWorkbench from "./LocalWorkbench";
import {desktop,type LayerInfo} from "./transport";
export default function DesktopProject({rootHandle}:{rootHandle:string}){
 const [layers,setLayers]=useState<LayerInfo[]>([]),[active,setActive]=useState("root"),[handles,setHandles]=useState<Record<string,string>>({root:rootHandle});
 const [visible,setVisible]=useState<string[]>(["root"]),[name,setName]=useState(""),[error,setError]=useState(""),[busy,setBusy]=useState(false);
 const [catalog,setCatalog]=useState<Awaited<ReturnType<typeof desktop.inspectGPKG>>>(null),[table,setTable]=useState("");
 const current=useRef(handles);current.current=handles;const alive=useRef(true);const lock=useRef(false);
 const reload=useCallback(async()=>{const x=await desktop.listLayers(rootHandle);if(alive.current)setLayers(x)},[rootHandle]);
 const run=useCallback(async(f:()=>Promise<void>)=>{
   if(lock.current)return;lock.current=true;setBusy(true);setError("");
   try{await f()}catch(e){if(alive.current)setError(String(e))}
   finally{lock.current=false;if(alive.current)setBusy(false)}
 },[]);
 useEffect(()=>{alive.current=true;void run(reload);return()=>{alive.current=false;for(const [id,h]of Object.entries(current.current))if(id!=="root")void desktop.close(h).catch(()=>{})}},[reload,run]);
 const ensure=async(id:string)=>{
   if(current.current[id])return current.current[id]!;
   const w=await desktop.openLayer(rootHandle,id);
   if(!alive.current){await desktop.close(w.handle).catch(()=>{});throw Error("WORKSPACE_CLOSED")}
   const next={...current.current,[id]:w.handle};current.current=next;setHandles(next);return w.handle;
 };
 const select=async(id:string)=>{await ensure(id);setActive(id);setVisible(v=>v.includes(id)?v:[...v,id])};
 const toggle=async(id:string)=>{
   if(id===active)return;
   if(visible.includes(id)){
     setVisible(v=>v.filter(x=>x!==id));
     if(id!=="root"&&current.current[id]){
       await desktop.close(current.current[id]!);const next={...current.current};delete next[id];current.current=next;setHandles(next);
     }
   }else{await ensure(id);setVisible(v=>[...v,id])}
 };
 const create=async()=>{
   const n=name.trim();if(!n)return;
   const x=await desktop.createLayer(rootHandle,n);setName("");await reload();await select(x.id);
 };
 const importSelected=async()=>{
   if(!catalog||!table)return;
   const x=await desktop.createLayer(rootHandle,table,catalog.id,table);
   await reload();await select(x.id);
 };
 const overlays=visible.filter(id=>id!==active&&!!handles[id]).map(id=>({id,handle:handles[id]!,name:id==="root"?"Lớp mặc định":layers.find(l=>l.id===id)?.name??id}));
 return <section className="ovg-project-layer-stack">
   <div className="ovg-project-layers" aria-label="Quản lý lớp">
     <strong>Lớp dự án</strong>
     {[{id:"root",name:"Lớp mặc định"},...layers].map(l=><span key={l.id} className="ovg-layer-chip">
       <input aria-label={"Hiển thị "+l.name} type="checkbox" checked={visible.includes(l.id)} disabled={busy||active===l.id} onChange={()=>void run(()=>toggle(l.id))}/>
       <button disabled={busy} aria-pressed={active===l.id} onClick={()=>void run(()=>select(l.id))}>{l.name}</button>
     </span>)}
     <input aria-label="Tên lớp mới" value={name} maxLength={128} onChange={e=>setName(e.target.value)}/><button disabled={busy||!name.trim()} onClick={()=>void run(create)}>Tạo lớp</button>
     <button disabled={busy} onClick={()=>void run(async()=>{const c=await desktop.inspectGPKG(rootHandle);if(c){setCatalog(c);setTable(c.layers[0]?.table??"")}})}>Nhập lớp từ GPKG</button>
     {catalog&&<><select aria-label="Bảng nguồn GPKG" value={table} onChange={e=>setTable(e.target.value)}>{catalog.layers.map(l=><option key={l.table} value={l.table}>{l.table} · EPSG:{l.epsg}</option>)}</select><button disabled={busy||!table} onClick={()=>void run(importSelected)}>Tạo lớp từ bảng</button></>}
     {error&&<span role="alert">{error}</span>}
   </div>
   {handles[active]&&<LocalWorkbench key={handles[active]} handle={handles[active]!} overlays={overlays}/>}
 </section>;
}
