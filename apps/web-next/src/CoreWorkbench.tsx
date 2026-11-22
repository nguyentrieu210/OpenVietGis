import React,{lazy,Suspense,useEffect,useState} from "react";
import {MemoryRouter,Routes,Route} from "react-router-dom";
import {QueryClient,QueryClientProvider} from "@tanstack/react-query";
import "../../web/src/ui/tokens.css";
import "../../web/src/features/platform/platform.css";
const Spatial=lazy(()=>import("../../web/src/features/spatial/SpatialCatalogPage").then(m=>({default:m.SpatialCatalogPage})));
const Transfer=lazy(()=>import("../../web/src/features/transfers/ImportExportPage").then(m=>({default:m.ImportExportPage})));
const Edit=lazy(()=>import("../../web/src/features/platform/EditingWorkbenchPage").then(m=>({default:m.EditingWorkbenchPage})));
const Query=lazy(()=>import("../../web/src/features/platform/QueryWorkbenchPage").then(m=>({default:m.QueryWorkbenchPage})));
const Process=lazy(()=>import("../../web/src/features/platform/ProcessingWorkbenchPage").then(m=>({default:m.ProcessingWorkbenchPage})));
const Cartography=lazy(()=>import("../../web/src/features/platform/CartographyWorkbenchPage").then(m=>({default:m.CartographyWorkbenchPage})));
const Hub=lazy(()=>import("../../web/src/features/platform/PlatformHubPage").then(m=>({default:m.PlatformHubPage})));
const Field=lazy(()=>import("../../web/src/features/offline/OfflineFieldPage").then(m=>({default:m.OfflineFieldPage})));
export type WorkbenchId="spatial"|"transfers"|"editing"|"queries"|"processing"|"buffer"|"clip"|"dissolve"|"cartography"|"cad"|"external"|"collab"|"field"|"ops";
export const workbenchTitles:Record<WorkbenchId,string>={spatial:"Danh mục, nguồn dữ liệu & CRS",transfers:"Nhập / xuất dữ liệu GIS",editing:"Biên tập & lịch sử thay đổi",queries:"Truy vấn thuộc tính & không gian",processing:"Phân tích không gian & tác vụ",buffer:"Vùng đệm (Buffer)",clip:"Cắt lớp (Clip)",dissolve:"Gộp vùng (Dissolve)",cartography:"Ký hiệu, bố cục & in bản đồ",cad:"CAD / DXF & trắc địa COGO",external:"Nguồn ngoài & OGC",collab:"Cộng tác & phê duyệt",field:"Thực địa & đồng bộ ngoại tuyến",ops:"Vận hành & khả năng nền tảng"};
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
