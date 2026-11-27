import React,{useEffect,useRef,useState} from "react";
import {createPortal} from "react-dom";
import {useDesktopDialog} from "./studio-ui";
import {GisIcon} from "./GisIcon";
type Item={id:string;name:string};
type Props={orgs:Item[];projects:Item[];org:string;project:string;setOrg:(id:string)=>void;setProject:(id:string)=>void;createOrg:()=>void;createProject:()=>void;permissions:()=>void};
const low=(s:string)=>s.toLocaleLowerCase("vi");
const initials=(n:string)=>n.trim().split(/\s+/).slice(0,2).map(w=>w[0]??"").join("").toUpperCase()||"?";
const hue=(id:string)=>[...id].reduce((h,c)=>(h*31+c.charCodeAt(0))%360,0);

// Spec 058: one "workspace" dialog — organisations on the left, the chosen organisation's projects as cards.
function WorkspaceDialog({p,close}:{p:Props;close:()=>void}){
 const [q,setQ]=useState(""),[pq,setPq]=useState(""),search=useRef<HTMLInputElement>(null),panel=useRef<HTMLDivElement>(null);
 useDesktopDialog(panel,close);
 useEffect(()=>{search.current?.focus()},[]);
 const orgs=p.orgs.filter(o=>low(o.name).includes(low(q))),projects=p.projects.filter(x=>low(x.name).includes(low(pq))),current=p.orgs.find(o=>o.id===p.org);
 return createPortal(<div className="qgis-studio ovg-ws-shade" onMouseDown={e=>{if(e.target===e.currentTarget)close()}}><div ref={panel} tabIndex={-1} className="ovg-ws" role="dialog" aria-modal="true" aria-label="Tổ chức và dự án">
  <header><b>Tổ chức và dự án</b><button type="button" aria-label="Đóng" onClick={close}>×</button></header>
  <div className="ovg-ws-body">
   <nav className="ovg-ws-orgs" aria-label="Tổ chức"><div className="ovg-ws-search"><GisIcon name="search" size={14}/><input ref={search} value={q} onChange={e=>setQ(e.target.value)} placeholder="Tìm tổ chức…" aria-label="Tìm tổ chức"/></div>
    <div className="ovg-ws-list">{orgs.map(o=><button type="button" key={o.id} aria-current={o.id===p.org} className={o.id===p.org?"active":""} onClick={()=>p.setOrg(o.id)}><span className="ovg-ws-av" style={{background:"hsl("+hue(o.id)+" 45% 42%)"}}>{initials(o.name)}</span><span>{o.name}</span></button>)}{!orgs.length&&<small>Không có tổ chức phù hợp</small>}</div>
    <button type="button" className="ovg-ws-new" onClick={()=>{close();p.createOrg()}}>＋ Tạo tổ chức</button></nav>
   <section className="ovg-ws-projects" aria-label="Dự án">
    <div className="ovg-ws-head"><div><small>Dự án của</small><b>{current?.name??"—"}</b></div><div className="ovg-ws-search"><GisIcon name="search" size={14}/><input value={pq} onChange={e=>setPq(e.target.value)} placeholder="Tìm dự án…" aria-label="Tìm dự án"/></div></div>
    <div className="ovg-ws-grid">{projects.map(x=><button type="button" key={x.id} className={"ovg-ws-card"+(x.id===p.project?" active":"")} onClick={()=>{p.setProject(x.id);close()}}><span className="ovg-ws-thumb" style={{"--h":hue(x.id)} as React.CSSProperties}><GisIcon name="layers" size={18}/></span><b>{x.name}</b><small>{x.id===p.project?"Đang mở":"Mở dự án"}</small></button>)}
     <button type="button" className="ovg-ws-card ovg-ws-add" disabled={!p.org} onClick={()=>{close();p.createProject()}}><span className="ovg-ws-thumb plus">＋</span><b>Dự án mới</b><small>trong {current?.name??"tổ chức"}</small></button></div>
    {!p.projects.length&&<p className="ovg-ws-empty">Tổ chức này chưa có dự án.</p>}
    <footer><button type="button" disabled={!p.org} onClick={()=>{close();p.permissions()}}><GisIcon name="lock" size={14}/> Phân quyền và thành viên</button></footer>
   </section>
  </div></div></div>,document.body);
}
export function WorkspacePicker(p:Props){
 const [open,setOpen]=useState(false),org=p.orgs.find(o=>o.id===p.org),project=p.projects.find(x=>x.id===p.project);
 return <div className="ovg-workspace-picker"><button type="button" className="ovg-picker-trigger ovg-ws-trigger" aria-haspopup="dialog" aria-expanded={open} title="Đổi tổ chức hoặc dự án" onClick={()=>setOpen(true)}><span className="org">{org?.name??"Chọn tổ chức"}</span><span className="ovg-workspace-divider">/</span><b>{project?.name??"Chọn dự án"}</b><GisIcon name="chevron" size={12}/></button>
  <button type="button" className="ovg-access-shortcut" title="Phân quyền tổ chức và dự án" aria-label="Phân quyền tổ chức và dự án" disabled={!p.org} onClick={()=>p.permissions()}><GisIcon name="lock" size={15}/><span>Phân quyền</span></button>
  {open&&<WorkspaceDialog p={p} close={()=>setOpen(false)}/>}</div>;
}
