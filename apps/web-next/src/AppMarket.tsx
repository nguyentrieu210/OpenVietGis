import React,{useState} from "react";
import type {ExtensionActivation} from "@openvietgis/client-sdk";
import {compatibleActivation,type Manifest} from "./official-module-registry";

// Spec 058: "Chợ ứng dụng" — official apps as cards with search and categories; install = activate for the project.
type Meta={icon:string;category:string;tagline:string;color:string};
const META:Record<string,Meta>={
 house_numbering:{icon:"🏠",category:"Quản lý đô thị",tagline:"Đánh số nhà, đối chiếu chứng nhận, ký duyệt theo tuyến",color:"#2b7bd0"},
 land_asset:{icon:"🌾",category:"Đất đai & tài sản",tagline:"Quản lý thửa đất, tài sản gắn với đất",color:"#16a34a"},
};
const meta=(m:Manifest):Meta=>META[m.extension_key]??{icon:"🧩",category:"Khác",tagline:m.description??"Ứng dụng mở rộng cho dự án",color:"#7c3aed"};
type Props={installed:Manifest[];activations:ExtensionActivation[];choice:string;busy:boolean;canManage:boolean;open:(key:string)=>void;toggle:(m:Manifest,on:boolean)=>void;openIntegrations:()=>void;configurePermissions?:(m:Manifest)=>void;children?:React.ReactNode};
export function AppMarket({installed,activations,choice,busy,canManage,open,toggle,openIntegrations,configurePermissions,children}:Props){
 const [q,setQ]=useState(""),[cat,setCat]=useState("Tất cả");
 const cats=["Tất cả","Đã cài",...new Set(installed.map(m=>meta(m).category)),"Kết nối"];
 const low=(s:string)=>s.toLocaleLowerCase("vi");
 const state=(m:Manifest)=>{const a=activations.find(x=>x.extension_key===m.extension_key&&x.status==="active");return a?(compatibleActivation(m,a)?"on":"bad"):"off"};
 const apps=installed.filter(m=>(cat==="Tất cả"||(cat==="Đã cài"?state(m)!=="off":meta(m).category===cat))&&low(m.display_name+" "+meta(m).tagline).includes(low(q)));
 return <div className="ovg-market">
  <div className="ovg-market-top"><input type="search" placeholder="Tìm ứng dụng…" value={q} onChange={e=>setQ(e.target.value)} aria-label="Tìm ứng dụng"/><div className="ovg-market-cats" role="tablist">{cats.map(c=><button type="button" key={c} role="tab" aria-selected={cat===c} onClick={()=>setCat(c)}>{c}</button>)}</div></div>
  {cat!=="Kết nối"&&<div className="ovg-market-grid">
   {(cat==="Tất cả"||cat==="Đã cài")&&!q&&<article className="ovg-app"><span className="ovg-app-icon" style={{background:"#0f4c81"}}>🗺️</span><div><b>GIS Core</b><small>Có sẵn</small><p>Bản đồ, lớp dữ liệu, thuộc tính, nhập và biên tập</p></div><footer><span className="ovg-app-badge on">Đang dùng</span><button type="button" className={!choice?"primary":""} onClick={()=>open("")}>Mở</button></footer></article>}
   {apps.map(m=>{const s=state(m),x=meta(m);return <article className="ovg-app" key={m.extension_key}><span className="ovg-app-icon" style={{background:x.color}}>{x.icon}</span><div><b>{m.display_name}</b><small>{x.category} · v{m.version} · Chính thức</small><p>{x.tagline}</p></div>
    <footer>{s==="on"?<><span className="ovg-app-badge on">Đã cài</span><button type="button" className={choice===m.extension_key?"primary":""} onClick={()=>open(m.extension_key)}>Mở</button>{canManage&&configurePermissions&&<button type="button" disabled={busy} onClick={()=>configurePermissions(m)}>Quyền</button>}<button type="button" disabled={busy||!canManage} onClick={()=>toggle(m,false)}>Gỡ</button></>:s==="bad"?<span className="ovg-app-badge bad">Phiên bản không khớp</span>:<button type="button" className="primary" disabled={busy||!canManage} title={canManage?"Cài cho cả dự án":"Chỉ quản trị dự án được cài"} onClick={()=>toggle(m,true)}>{busy?"Đang cài…":"Cài vào dự án"}</button>}</footer></article>})}
   {!apps.length&&<p className="ovg-market-empty">Không có ứng dụng phù hợp.</p>}
  </div>}
  {(cat==="Tất cả"||cat==="Kết nối")&&!q&&<><h4>Kết nối phần mềm</h4><div className="ovg-market-grid"><article className="ovg-app"><span className="ovg-app-icon" style={{background:"#ea580c"}}>⇄</span><div><b>QGIS, AutoCAD, PostGIS, Địa chính, 3D</b><small>Kết nối · Chính thức</small><p>Đồng bộ dự án với phần mềm máy bàn và hệ thống khác</p></div><footer><button type="button" className="primary" onClick={openIntegrations}>Mở trung tâm kết nối</button></footer></article></div></>}
  {(cat==="Tất cả")&&!q&&children&&<><h4>Ứng dụng đối tác</h4>{children}</>}
  {!canManage&&<p className="ovg-market-note">Mọi thành viên mở được ứng dụng đã cài; chỉ quản trị dự án được cài hoặc gỡ.</p>}
 </div>;
}
