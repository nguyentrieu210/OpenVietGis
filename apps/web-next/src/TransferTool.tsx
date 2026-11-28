import React,{Suspense,useEffect,useState} from "react";
import {ApiError,getIntegrationHubAdapterProfile,listAllowlistedHosts,registerAllowlistedHost,listExternalConnections,createExternalConnection,loadCsrfToken,type IntegrationAdapterProfileResponse,type ExternalConnection} from "@openvietgis/client-sdk";
import {BatchExport} from "./IntegrationCatalog";
import {ImportPanel} from "./LiveApp";
import {ExternalPreview} from "./workbench-registry";

// Spec 058: QGIS-style tools replacing the legacy Import/Export and External sources pages.
import {apiClient as client} from "./workbenches/app/api";
const reloadMap=()=>window.dispatchEvent(new CustomEvent("ovg:menu-command",{detail:"RELOAD"}));
import {Tabs as SharedTabs} from "./ui/primitives";
function Tabs<T extends string>(props:{tabs:[T,string][];tab:T;set:(t:T)=>void}){return <SharedTabs {...props} className="tool-tabs"/>}

export function TransferTool({org,pid}:{org:string;pid:string}){
 const [tab,setTab]=useState<"import"|"export">(()=>{try{const t=sessionStorage.getItem("ovg:transfer-tab");sessionStorage.removeItem("ovg:transfer-tab");return t==="export"?"export":"import"}catch{return "import"}}),[busy,setBusy]=useState(false),[err,setErr]=useState(""),[profile,setProfile]=useState<IntegrationAdapterProfileResponse|null>(null);
 const [loadFailed,setLoadFailed]=useState(false);
 useEffect(()=>{if(tab!=="export"||profile||loadFailed)return;let live=true;getIntegrationHubAdapterProfile(client,org,pid,"qgis").then(p=>{if(live)setProfile(p)}).catch(()=>{if(live){setErr("Không tải được danh sách lớp để xuất.");setLoadFailed(true)}});return()=>{live=false}},[tab,org,pid,profile,loadFailed]);
 const action=async(fn:()=>Promise<void>)=>{if(busy)return;setBusy(true);setErr("");try{await loadCsrfToken(client);await fn()}catch(e){setErr(e instanceof ApiError?"Không thực hiện được ("+e.status+").":e instanceof Error?e.message:"Không thực hiện được.")}finally{setBusy(false)}};
 return <div><Tabs tabs={[["import","Nhập tệp vào dự án"],["export","Xuất dữ liệu"]]} tab={tab} set={setTab}/>
  {err&&<p className="error" role="alert">{err}</p>}
  {tab==="import"?<ImportPanel org={org} pid={pid} busy={busy} action={action} refresh={reloadMap}/>
  :profile?<BatchExport client={client} org={org} projectId={pid} adapterKey="qgis" layers={profile.layers}/>:loadFailed?<button type="button" onClick={()=>{setErr("");setLoadFailed(false)}}>Thử tải lại</button>:<p>Đang tải danh sách lớp…</p>}
 </div>;
}

const PROTOCOL:Record<ExternalConnection["protocol"],string>={wms:"WMS",wmts_xyz:"WMTS / XYZ",cog_geotiff:"GeoTIFF (COG)",wfs_ogc_features:"WFS / OGC Features"};
export function ExternalTool({org,pid}:{org:string;pid:string}){
 const [items,setItems]=useState<ExternalConnection[]>([]),[rev,setRev]=useState(0),[f,setF]=useState({protocol:"wms" as ExternalConnection["protocol"],url:"",layer:"",name:"",attribution:"",license:"Theo điều khoản của nhà cung cấp"}),[busy,setBusy]=useState(false),[err,setErr]=useState(""),[msg,setMsg]=useState(""),[preview,setPreview]=useState<string|null>(null);
 useEffect(()=>{let live=true;listExternalConnections(client,org,pid).then(r=>{if(live)setItems(r.items)}).catch(()=>{if(live)setErr("Không tải được danh sách kết nối.")});return()=>{live=false}},[org,pid,rev]);
 const host=(()=>{try{return new URL(f.url).hostname.toLowerCase()}catch{return ""}})();
 const add=async(e:React.FormEvent)=>{e.preventDefault();if(!host){setErr("URL không hợp lệ, cần dạng https://máy-chủ/…");return}setBusy(true);setErr("");setMsg("");try{const csrf=await loadCsrfToken(client);const allow=(await listAllowlistedHosts(client,org)).items.find(h=>h.hostname===host)??await registerAllowlistedHost(client,org,{hostname:host},csrf);await createExternalConnection(client,org,pid,{allowlist_id:allow.id,name:f.name.trim()||PROTOCOL[f.protocol]+" · "+host,protocol:f.protocol,endpoint_url:f.url.trim(),layer_identifier:f.layer.trim(),attribution:f.attribution.trim()||host,license_text:f.license.trim()},csrf);setMsg("Đã thêm kết nối. Bấm Xem thử để kiểm tra trước khi dùng.");setF(s=>({...s,url:"",layer:"",name:""}));setRev(x=>x+1)}catch(ex){setErr(ex instanceof ApiError?(ex.status===403?"Không có quyền thêm nguồn ngoài.":ex.status===422?"Máy chủ từ chối cấu hình (kiểm tra URL, giao thức, tên lớp).":"Không thêm được ("+ex.status+")."):"Không thêm được.")}finally{setBusy(false)}};
 const row=(label:string,el:React.ReactNode)=><label className="form-row"><span>{label}</span>{el}<small/></label>;
 return <div className="ext-tool">
  <form className="tool-form" onSubmit={add}><h3>Kết nối mới</h3>
   {row("Giao thức",<select value={f.protocol} onChange={e=>setF(s=>({...s,protocol:e.target.value as ExternalConnection["protocol"]}))}>{Object.entries(PROTOCOL).map(([k,v])=><option key={k} value={k}>{v}</option>)}</select>)}
   {row("URL dịch vụ",<input required type="url" placeholder="https://…/wms" value={f.url} onChange={e=>setF(s=>({...s,url:e.target.value}))}/>)}
   {row(f.protocol==="wfs_ogc_features"?"Tên lớp (feature type)":"Tên lớp",<input value={f.layer} onChange={e=>setF(s=>({...s,layer:e.target.value}))} placeholder={f.protocol==="wmts_xyz"?"có thể để trống với XYZ":""}/>)}
   {row("Tên hiển thị",<input value={f.name} onChange={e=>setF(s=>({...s,name:e.target.value}))} placeholder={host?PROTOCOL[f.protocol]+" · "+host:"tự đặt theo máy chủ"}/>)}
   <details><summary>Nâng cao: ghi công, giấy phép</summary>{row("Ghi công",<input value={f.attribution} onChange={e=>setF(s=>({...s,attribution:e.target.value}))} placeholder={host||"© nhà cung cấp"}/>)}{row("Giấy phép",<input value={f.license} onChange={e=>setF(s=>({...s,license:e.target.value}))}/>)}</details>
   {err&&<p className="error" role="alert">{err}</p>}{msg&&<p className="cat-ok" role="status">{msg}</p>}
   <div className="tool-buttons"><button type="submit" className="primary" disabled={busy||!f.url}>{busy?"Đang thêm…":"Thêm kết nối"}</button></div>
  </form>
  <h3>Kết nối của dự án <small>{items.length}</small></h3>
  <div className="cat-scroll"><table className="cat-table"><thead><tr><th>Tên</th><th>Giao thức</th><th>Máy chủ</th><th>Trạng thái</th><th/></tr></thead><tbody>
   {items.map(c=><React.Fragment key={c.id}><tr><td title={c.name}>{c.name}</td><td>{PROTOCOL[c.protocol]}</td><td title={c.endpoint_url}>{(()=>{try{return new URL(c.endpoint_url).hostname}catch{return c.endpoint_url}})()}</td><td>{c.availability==="revoked"?"Đã thu hồi":c.availability==="unverified"?"Chưa kiểm tra":"Sẵn sàng"}</td><td className="act"><button type="button" onClick={()=>setPreview(p=>p===c.id?null:c.id)}>{preview===c.id?"Ẩn":"Xem thử"}</button></td></tr>
    {preview===c.id&&<tr><td colSpan={5}><Suspense fallback={<p>Đang tải…</p>}><ExternalPreview connection={c} organizationId={org} projectId={pid}/></Suspense></td></tr>}</React.Fragment>)}
   {!items.length&&<tr><td colSpan={5} className="empty">Chưa có kết nối nào.</td></tr>}
  </tbody></table></div>
 </div>;
}
