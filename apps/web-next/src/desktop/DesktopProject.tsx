import {useCallback,useEffect,useRef,useState} from "react";
import LocalWorkbench from "./LocalWorkbench";
import {desktop,type LayerInfo,type ProjectState,type ViewState,type LayerStyle} from "./transport";
export default function DesktopProject({rootHandle}:{rootHandle:string}){
 const [layers,setLayers]=useState<LayerInfo[]>([]),[active,setActive]=useState("root"),[handles,setHandles]=useState<Record<string,string>>({root:rootHandle});
 const [visible,setVisible]=useState<string[]>(["root"]),[name,setName]=useState(""),[error,setError]=useState(""),[busy,setBusy]=useState(false);
 const [catalog,setCatalog]=useState<Awaited<ReturnType<typeof desktop.inspectGPKG>>>(null),[table,setTable]=useState("");
 const [presentation,setPresentation]=useState<ProjectState>();
 const view=useRef<ViewState>({lon:106.7,lat:11,zoom:5,bearing:0,pitch:0});
 const initialView=useRef(view.current);
 const current=useRef(handles);current.current=handles;const alive=useRef(true);const lock=useRef(false);
 const reload=useCallback(async()=>{const x=await desktop.listLayers(rootHandle);if(alive.current)setLayers(x)},[rootHandle]);
 const run=useCallback(async(f:()=>Promise<void>)=>{
   if(lock.current)return;lock.current=true;setBusy(true);setError("");
   try{await f()}catch(e){if(alive.current)setError(String(e))}
   finally{lock.current=false;if(alive.current)setBusy(false)}
 },[]);
 useEffect(()=>{alive.current=true;void run(async()=>{
   await reload();const s=await desktop.projectState(rootHandle);
   if(!alive.current)return;
   view.current=s.view;initialView.current=s.view;
   const ids=s.layers.filter(l=>l.visible).map(l=>l.id);
   const opened:Record<string,string>={root:rootHandle};
   try{
     for(const id of ids.filter(i=>i!=="root").slice(0,7)){
       const w=await desktop.openLayer(rootHandle,id);if(!alive.current){await desktop.close(w.handle).catch(()=>{});throw Error("WORKSPACE_CLOSED")}
       opened[id]=w.handle;
     }
   }catch(e){for(const [id,h]of Object.entries(opened))if(id!=="root")await desktop.close(h).catch(()=>{});throw e}
   current.current=opened;setHandles(opened);setVisible(ids.filter(id=>!!opened[id]));setActive(opened[s.active]?s.active:"root");setPresentation(s);
 });return()=>{alive.current=false;for(const [id,h]of Object.entries(current.current))if(id!=="root")void desktop.close(h).catch(()=>{})}},[reload,run,rootHandle]);
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
 const order=presentation?.layers.map(l=>l.id)??[];
 const ordered=[...new Set([...order,"root",...layers.map(l=>l.id)])].filter(id=>id==="root"||layers.some(l=>l.id===id));
 const styleFor=(id:string):LayerStyle=>presentation?.layers.find(l=>l.id===id)?.style??{color:"#1976a3",opacity:.25};
 const changeStyle=(id:string,style:LayerStyle)=>setPresentation(p=>p?{...p,layers:[...p.layers.filter(l=>l.id!==id),{id,visible:visible.includes(id),style}]}:p);
 const move=(id:string,d:number)=>setPresentation(p=>{if(!p)return p;const i=ordered.indexOf(id),j=i+d;if(j<0||j>=ordered.length)return p;const ids=[...ordered];[ids[i],ids[j]]=[ids[j]!,ids[i]!];return {...p,layers:ids.map(k=>({id:k,visible:visible.includes(k),style:styleFor(k)}))}});
 const save=async()=>{if(!presentation)return;const next:ProjectState={...presentation,view:view.current,active,layers:ordered.map(id=>({id,visible:visible.includes(id),style:styleFor(id)}))};setPresentation(await desktop.saveProjectState(rootHandle,next))};
 const overlays=ordered.filter(id=>visible.includes(id)).filter(id=>id!==active&&!!handles[id]).map(id=>({id,handle:handles[id]!,name:id==="root"?"Lớp mặc định":layers.find(l=>l.id===id)?.name??id,style:styleFor(id)}));
 return <section className="ovg-project-layer-stack">
   <div className="ovg-project-layers" aria-label="Quản lý lớp">
     <strong>Lớp dự án</strong>
     {ordered.map(id=>({id,name:id==="root"?"Lớp mặc định":layers.find(l=>l.id===id)?.name??id})).map(l=><span key={l.id} className="ovg-layer-chip">
       <input aria-label={"Hiển thị "+l.name} type="checkbox" checked={visible.includes(l.id)} disabled={busy||active===l.id} onChange={()=>void run(()=>toggle(l.id))}/>
       <button disabled={busy} aria-pressed={active===l.id} onClick={()=>void run(()=>select(l.id))}>{l.name}</button>
       <button disabled={busy||!presentation} aria-label={"Đưa lớp "+l.name+" lên"} onClick={()=>move(l.id,-1)}>↑</button><button disabled={busy||!presentation} aria-label={"Đưa lớp "+l.name+" xuống"} onClick={()=>move(l.id,1)}>↓</button>
     </span>)}
     <input aria-label="Tên lớp mới" value={name} maxLength={128} onChange={e=>setName(e.target.value)}/><button disabled={busy||!name.trim()} onClick={()=>void run(create)}>Tạo lớp</button>
     <button disabled={busy} onClick={()=>void run(async()=>{const c=await desktop.inspectGPKG(rootHandle);if(c){setCatalog(c);setTable(c.layers[0]?.table??"")}})}>Nhập lớp từ GPKG</button>
     {catalog&&<><select aria-label="Bảng nguồn GPKG" value={table} onChange={e=>setTable(e.target.value)}>{catalog.layers.map(l=><option key={l.table} value={l.table}>{l.table} · EPSG:{l.epsg}</option>)}</select><button disabled={busy||!table} onClick={()=>void run(importSelected)}>Tạo lớp từ bảng</button></>}
     <label>Màu lớp <input type="color" value={styleFor(active).color} disabled={!presentation} onChange={e=>changeStyle(active,{...styleFor(active),color:e.target.value})}/></label><label>Độ mờ <input type="range" min="0" max="1" step=".05" value={styleFor(active).opacity} disabled={!presentation} onChange={e=>changeStyle(active,{...styleFor(active),opacity:Number(e.target.value)})}/></label><button disabled={busy||!presentation} onClick={()=>void run(save)}>Lưu cấu hình dự án</button>
     {error&&<span role="alert">{error}</span>}
   </div>
   {presentation&&handles[active]&&<LocalWorkbench key={handles[active]} handle={handles[active]!} overlays={overlays} initialView={view.current} onView={v=>{view.current=v}} layerStyle={styleFor(active)}/>}
 </section>;
}
