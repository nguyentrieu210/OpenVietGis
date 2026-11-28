import React,{Suspense,useEffect,useState} from "react";
import {MemoryRouter,Routes,Route} from "react-router-dom";
import {QueryClient,QueryClientProvider} from "@tanstack/react-query";
import {CatalogManager} from "./CatalogManager";
import {CadTool} from "./CadTool";
import {TransferTool,ExternalTool} from "./TransferTool";
import {Edit,Query,Process,Cartography,Hub,Field} from "./workbench-registry";
import "./desktop-dialog.css";
import {workbenchTitles,type WorkbenchId} from "./workbench-contract";
class WorkbenchBoundary extends React.Component<{children:React.ReactNode},{failed:boolean}>{
 state={failed:false};static getDerivedStateFromError(){return {failed:true}}
 render(){return this.state.failed?<div className="ovg-tool-error" role="alert"><strong>Không thể mở công cụ</strong><p>Kiểm tra kết nối hoặc thử tải lại công cụ. Bản đồ và các công cụ khác vẫn hoạt động.</p><button type="button" onClick={()=>this.setState({failed:false})}>Thử lại</button></div>:this.props.children}
}
function Return({close}:{close:()=>void}){useEffect(close,[]);return null}
export function CoreWorkbench({tool,org,pid,close}:{tool:WorkbenchId;org:string;pid:string;close:()=>void}){
 const [queryClient]=useState(()=>new QueryClient({defaultOptions:{queries:{retry:false,staleTime:0}}}));
 const [wide,setWide]=useState(true);
 useEffect(()=>setWide(true),[tool]);
 const path="/app/organizations/"+encodeURIComponent(org)+"/projects/"+encodeURIComponent(pid);
 const component=tool==="spatial"?<CatalogManager org={org} pid={pid}/>:tool==="cad"?<CadTool org={org} pid={pid}/>:tool==="transfers"?<TransferTool org={org} pid={pid}/>:tool==="external"?<ExternalTool org={org} pid={pid}/>:tool==="editing"?<Edit/>:tool==="queries"?<Query/>:["processing","buffer","clip","dissolve","difference","intersection","union","reproject","spatial_join"].includes(tool)?<Process/>:tool==="cartography"?<Cartography/>:tool==="field"?<Field/>:<Hub embeddedTab={tool as "cad"|"external"|"collab"|"ops"}/>;

 return <aside className={"panel core-workbench "+(wide?"wide":"")} role="dialog" aria-modal="false" aria-label={workbenchTitles[tool]}>
 <header><div><small>GIS CORE · CÔNG CỤ</small><b>{workbenchTitles[tool]}</b></div><button type="button" aria-expanded={wide} onClick={()=>setWide(v=>!v)}>{wide?"Thu gọn":"Mở rộng"}</button><button aria-label="Đóng công cụ GIS" onClick={close}>✕</button></header>
 <div className="core-workbench-body"><QueryClientProvider client={queryClient}><MemoryRouter initialEntries={[tool==="field"?"/offline-field/"+encodeURIComponent(org)+"/"+encodeURIComponent(pid):path+"/"+tool+(["buffer","clip","dissolve","difference","intersection","union","reproject","spatial_join"].includes(tool)?"?algorithm="+tool:"")]}><WorkbenchBoundary key={tool}><Suspense fallback={<div className="ovg-tool-loading" role="status"><progress aria-label="Đang tải công cụ"/><span>Đang mở công cụ GIS…</span></div>}><Routes>
 <Route path="/offline-field/:organizationId/:projectId" element={<Field/>}/>
 <Route path="/app/organizations/:organizationId/projects/:projectId/map" element={<Return close={close}/>}/>
 <Route path="/app/organizations/:organizationId/projects" element={<Return close={close}/>}/>
 <Route path="/app/organizations/:organizationId/projects/:projectId/:workbench" element={component}/>
 <Route path="*" element={<Return close={close}/>}/>
 </Routes></Suspense></WorkbenchBoundary></MemoryRouter></QueryClientProvider></div></aside>
}
