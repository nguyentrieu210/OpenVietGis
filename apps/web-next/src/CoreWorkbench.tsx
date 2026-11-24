import React,{Suspense,useEffect,useState} from "react";
import {MemoryRouter,Routes,Route} from "react-router-dom";
import {QueryClient,QueryClientProvider} from "@tanstack/react-query";
import {Spatial,Transfer,Edit,Query,Process,Cartography,Hub,Field} from "./legacy-workbench-bridge";
import {workbenchTitles,type WorkbenchId} from "./workbench-contract";
function Return({close}:{close:()=>void}){useEffect(close,[]);return null}
export function CoreWorkbench({tool,org,pid,close}:{tool:WorkbenchId;org:string;pid:string;close:()=>void}){
 const [queryClient]=useState(()=>new QueryClient({defaultOptions:{queries:{retry:false,staleTime:0}}}));
 const [wide,setWide]=useState(true);
 const path="/app/organizations/"+encodeURIComponent(org)+"/projects/"+encodeURIComponent(pid);
 const component=tool==="spatial"?<Spatial/>:tool==="transfers"?<Transfer/>:tool==="editing"?<Edit/>:tool==="queries"?<Query/>:["processing","buffer","clip","dissolve"].includes(tool)?<Process/>:tool==="cartography"?<Cartography/>:tool==="field"?<Field/>:<Hub embeddedTab={tool as "cad"|"external"|"collab"|"ops"}/>;

 return <aside className={"panel core-workbench "+(wide?"wide":"")} role="dialog" aria-modal="false" aria-label={workbenchTitles[tool]}>
 <header><div><small>GIS CORE · CÔNG CỤ</small><b>{workbenchTitles[tool]}</b></div><button onClick={()=>setWide(v=>!v)}>{wide?"Thu gọn":"Mở rộng"}</button><button aria-label="Đóng công cụ GIS" onClick={close}>✕</button></header>
 <div className="core-workbench-body"><QueryClientProvider client={queryClient}><MemoryRouter initialEntries={[tool==="field"?"/offline-field/"+encodeURIComponent(org)+"/"+encodeURIComponent(pid):path+"/"+tool+(["buffer","clip","dissolve"].includes(tool)?"?algorithm="+tool:"")]}><Suspense fallback={<p className="live-note">Đang mở công cụ GIS…</p>}><Routes>
 <Route path="/app/organizations/:organizationId/projects/:projectId/spatial" element={<Spatial/>}/>
 <Route path="/app/organizations/:organizationId/projects/:projectId/transfers" element={<Transfer/>}/>
 <Route path="/offline-field/:organizationId/:projectId" element={<Field/>}/>
 <Route path="/app/organizations/:organizationId/projects/:projectId/map" element={<Return close={close}/>}/>
 <Route path="/app/organizations/:organizationId/projects" element={<Return close={close}/>}/>
 <Route path="/app/organizations/:organizationId/projects/:projectId/:workbench" element={component}/>
 <Route path="*" element={<Return close={close}/>}/>
 </Routes></Suspense></MemoryRouter></QueryClientProvider></div></aside>
}
