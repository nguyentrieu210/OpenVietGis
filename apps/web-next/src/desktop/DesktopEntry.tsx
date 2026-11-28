import {useCallback,useEffect,useRef,useState} from "react";
import LocalWorkbench from "./LocalWorkbench";
import {desktop,type Capabilities,type Project,type Opened} from "./transport";
import "./desktop.css";

export default function DesktopEntry(){
  const [caps,setCaps]=useState<Capabilities>();
  const [projects,setProjects]=useState<Project[]>([]);
  const [opened,setOpened]=useState<Opened>();
  const [busy,setBusy]=useState(false);
  const [message,setMessage]=useState("");
  const [result,setResult]=useState("");
  const [jobActive,setJobActive]=useState(false);
  const active=useRef<Opened|undefined>(undefined);
  const running=useRef(false);
  const mounted=useRef(true);
  const job=useRef<{handle:string;id:string}|undefined>(undefined);
  const refresh=useCallback(async()=>setProjects(await desktop.list()),[]);
  const execute=useCallback(async (f:()=>Promise<void>)=>{
    if(running.current)return;
    running.current=true;setBusy(true);setMessage("");
    try{await f()}catch(e){if(mounted.current)setMessage(String(e))}
    finally{running.current=false;if(mounted.current)setBusy(false)}
  },[]);
  const close=useCallback(async()=>{
    const w=active.current;if(!w)return;
    await desktop.close(w.handle);active.current=undefined;setOpened(undefined);setResult("");
  },[]);
  const open=useCallback(async(id:string)=>{
    if(active.current?.id===id)return;
    await close();const w=await desktop.open(id);
    active.current=w;setOpened(w);
  },[close]);
  const create=useCallback(async()=>{
    const p=await desktop.create();await refresh();await open(p.id);
  },[refresh,open]);
  useEffect(()=>{
    mounted.current=true;
    void execute(async()=>{const c=await desktop.capabilities();if(c.protocol_version!==1||!c.standalone)throw new Error("INCOMPATIBLE_HOST");setCaps(c);await refresh()});
    let unlisten:(()=>void)|undefined;let disposed=false;
    void desktop.menu(id=>{if(id==="new")void execute(create);if(id==="close-workspace")void execute(close)})
      .then(fn=>{if(disposed)fn();else unlisten=fn}).catch(e=>setMessage(String(e)));
    return()=>{disposed=true;mounted.current=false;unlisten?.();const w=active.current;if(w)void desktop.close(w.handle).catch(()=>{})};
  },[execute,refresh,create,close]);
  const transform=()=>execute(async()=>{
    const w=active.current;if(!w)return;setResult("");
    const id=await desktop.transform(w.handle,4326,3857,[106.7,11]);
    job.current={handle:w.handle,id};setJobActive(true);
    try{
      for(let i=0;i<200;i++){
        if(!mounted.current||job.current?.id!==id)return;
        const p=await desktop.poll(w.handle,id);
        if(job.current?.id!==id)return;
        if(p.status==="complete"){setResult(p.xy.map(x=>x.toFixed(3)).join(", "));return}
        await new Promise(r=>setTimeout(r,50));
      }
      await desktop.cancel(w.handle,id);throw new Error("JOB_TIMEOUT");
    }finally{if(job.current?.id===id){job.current=undefined;if(mounted.current)setJobActive(false)}}
  });
  const cancel=async()=>{
    const j=job.current;if(!j)return;job.current=undefined;setJobActive(false);
    try{await desktop.cancel(j.handle,j.id)}catch(e){setMessage(String(e))}
  };
  return <div className="ovg-desktop">
    <header><strong>OpenVietGIS Desktop</strong><span>Standalone · Wave 2</span></header>
    <div className="ovg-desktop-body">
      <aside aria-label="Không gian làm việc">
        <h2>Dự án cục bộ</h2>
        <button disabled={!caps||busy} onClick={()=>void execute(create)}>Tạo dự án</button>
        <button disabled={!caps||busy} onClick={()=>void execute(async()=>{const p=await desktop.restoreSnapshot();if(p){await refresh();await open(p.id)}})}>Khôi phục bản sao GPKG</button>
        <button disabled={!caps||busy} onClick={()=>void execute(refresh)}>Làm mới</button>
        <ul>{projects.map(p=><li key={p.id}><button title={p.id} disabled={busy} aria-current={opened?.id===p.id} onClick={()=>void execute(()=>open(p.id))}>{p.id.slice(0,8)}</button></li>)}</ul>
        <button disabled={!opened||busy} onClick={()=>void execute(close)}>Đóng dự án</button>
        <h2>Khả năng</h2>
        <p>Cloud, đồng bộ và MVT/raster tile: chưa kích hoạt.</p>
        <button disabled={!opened||busy||!caps?.transform} onClick={()=>void transform()}>Thử chuyển CRS 4326 → 3857</button>
        <button disabled={!jobActive} onClick={()=>void cancel()}>Hủy tác vụ</button>
        {result&&<output>{result}</output>}
      </aside>
      <main>{opened&&caps?.local_edit&&caps.viewport?<LocalWorkbench key={opened.handle} handle={opened.handle}/>:<p>Chọn hoặc tạo dự án để mở bản đồ cục bộ.</p>}</main>
    </div>
    <footer role="status">{message|| (busy?"Đang xử lý…":opened? "Đang mở "+opened.id:"Chọn hoặc tạo dự án, không cần tài khoản.")}</footer>
  </div>;
}
