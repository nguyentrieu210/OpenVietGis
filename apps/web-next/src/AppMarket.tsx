import {Button,Input,Actions} from "./ui/primitives";
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
type Props={installed:Manifest[];activations:ExtensionActivation[];choice:string;busy:boolean;canManage:boolean;open:(key:string)=>void;toggle:(m:Manifest,on:boolean)=>void;openIntegrations:()=>void;children?:React.ReactNode};
export function AppMarket({installed,activations,choice,busy,canManage,open,toggle,openIntegrations,children}:Props){
 const [q,setQ]=useState(""),[cat,setCat]=useState("Tất cả"),[view,setView]=useState<"list"|"cards">("list"),[selected,setSelected]=useState("");
 const cats=["Tất cả","Đã cài","Chưa cài",...new Set(installed.map(m=>meta(m).category)),"Kết nối"];
 const low=(s:string)=>s.toLocaleLowerCase("vi");
 const state=(m:Manifest)=>{const a=activations.find(x=>x.extension_key===m.extension_key&&x.status==="active");return a?(compatibleActivation(m,a)?"on":"bad"):"off"};
 const apps=installed.filter(m=>(cat==="Tất cả"||(cat==="Đã cài"?state(m)!=="off":cat==="Chưa cài"?state(m)==="off":meta(m).category===cat))&&low(m.display_name+" "+meta(m).tagline).includes(low(q)));
 const detail=apps.find(m=>m.extension_key===selected)??apps[0];
 return <div className="ovg-market ovg:min-w-0 ovg:w-full">
  <div className="ovg-market-top"><div className="ovg-market-view"><b>Quản lý và cài đặt plugin</b><Button aria-pressed={view==="list"} onClick={()=>setView("list")}>Danh sách</Button><Button aria-pressed={view==="cards"} onClick={()=>setView("cards")}>Thẻ</Button></div><Input type="search" placeholder="Tìm ứng dụng…" value={q} onChange={e=>setQ(e.target.value)} aria-label="Tìm ứng dụng"/><div className="ovg-market-cats" role="tablist">{cats.map(c=><button type="button" key={c} role="tab" aria-selected={cat===c} onClick={()=>setCat(c)}>{c}</button>)}</div></div>
  {cat!=="Kết nối"&&view==="list"&&<div className="ovg-plugin-manager"><div className="ovg-plugin-list" role="group" aria-label="Danh sách plugin">{apps.map(m=>{const s=state(m);return <button type="button" key={m.extension_key} aria-pressed={detail?.extension_key===m.extension_key} onClick={()=>setSelected(m.extension_key)}><span className="ovg-plugin-list-status" aria-hidden="true">{s==="on"?"✓":s==="bad"?"!":"○"}</span><span>{m.display_name}<small>{s==="on"?(choice===m.extension_key?"Đang sử dụng":"Đã cài"):s==="bad"?"Không tương thích":"Chưa cài"} · v{m.version}</small></span></button>})}{!apps.length&&<p>Không có plugin phù hợp.</p>}</div><div className="ovg-plugin-details ovg:min-w-0">{detail&&<><h3>{detail.display_name}</h3><p>{detail.description??meta(detail).tagline}</p><dl><dt>Phiên bản</dt><dd>{detail.version}</dd><dt>Nguồn</dt><dd>Plugin chính thức</dd><dt>Trạng thái</dt><dd>{state(detail)==="on"?"Đã cài vào dự án":state(detail)==="bad"?"Không tương thích":"Chưa cài"}</dd></dl><Actions className="ovg-plugin-manager-actions">{state(detail)==="on"?<><button className="primary" onClick={()=>open(detail.extension_key)}>Mở</button>{choice===detail.extension_key&&<button onClick={()=>open("")}>Ngừng sử dụng</button>}<button disabled={busy||!canManage} onClick={()=>toggle(detail,false)}>Gỡ khỏi dự án</button></>:state(detail)==="off"?<button className="primary" disabled={busy||!canManage} onClick={()=>toggle(detail,true)}>Cài vào dự án</button>:<small>Cần phiên bản tương thích.</small>}</Actions><small>Ngừng sử dụng chỉ trở về GIS Core trong phiên này; gỡ plugin là thao tác cấp dự án.</small></>}</div></div>}
  {cat!=="Kết nối"&&view==="cards"&&<div className="ovg-market-grid">
   {(cat==="Tất cả"||cat==="Đã cài")&&!q&&<article className="ovg-app"><span className="ovg-app-icon" style={{background:"#0f4c81"}}>🗺️</span><div><b>GIS Core</b><small>Có sẵn</small><p>Bản đồ, lớp dữ liệu, thuộc tính, nhập và biên tập</p></div><footer><span className="ovg-app-badge on">Đang dùng</span><button type="button" className={!choice?"primary":""} onClick={()=>open("")}>Mở</button></footer></article>}
   {apps.map(m=>{const s=state(m),x=meta(m);return <article className="ovg-app" key={m.extension_key}><span className="ovg-app-icon" style={{background:x.color}}>{x.icon}</span><div><b>{m.display_name}</b><small>{x.category} · v{m.version} · Chính thức</small><p>{x.tagline}</p></div>
    <footer>{s==="on"?<><span className="ovg-app-badge on">Đã cài</span><button type="button" className={choice===m.extension_key?"primary":""} onClick={()=>open(m.extension_key)}>Mở</button><button type="button" disabled={busy||!canManage} onClick={()=>toggle(m,false)}>Gỡ</button></>:s==="bad"?<span className="ovg-app-badge bad">Phiên bản không khớp</span>:<button type="button" className="primary" disabled={busy||!canManage} title={canManage?"Cài cho cả dự án":"Chỉ quản trị dự án được cài"} onClick={()=>toggle(m,true)}>{busy?"Đang cài…":"Cài vào dự án"}</button>}</footer></article>})}
   {!apps.length&&<p className="ovg-market-empty">Không có ứng dụng phù hợp.</p>}
  </div>}
  {(cat==="Tất cả"||cat==="Kết nối")&&!q&&<><h4>Kết nối phần mềm</h4><div className="ovg-market-grid"><article className="ovg-app"><span className="ovg-app-icon" style={{background:"#ea580c"}}>⇄</span><div><b>QGIS, AutoCAD, PostGIS, Địa chính, 3D</b><small>Kết nối · Chính thức</small><p>Đồng bộ dự án với phần mềm máy bàn và hệ thống khác</p></div><footer><button type="button" className="primary" onClick={openIntegrations}>Mở trung tâm kết nối</button></footer></article></div></>}
  {(cat==="Tất cả")&&!q&&children&&<><h4>Ứng dụng đối tác</h4>{children}</>}
  {!canManage&&<p className="ovg-market-note">Mọi thành viên mở được ứng dụng đã cài; chỉ quản trị dự án được cài hoặc gỡ.</p>}
 </div>;
}
