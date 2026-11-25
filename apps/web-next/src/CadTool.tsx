import React,{Suspense,useEffect,useState} from "react";
import {ApiError,cogoTraverse,listCadConversions,loadCsrfToken} from "@openvietgis/client-sdk";
import {CadImport} from "./workbench-registry";

// Spec 058: CAD tool as three QGIS-style tabs instead of one long page.
import {apiClient as client} from "./workbenches/app/api";
type Tab="import"|"cogo"|"history";
const DIRECTION:Record<string,string>={import:"Nhập",export:"Xuất"};
const STATUS:Record<string,string>={completed:"Hoàn tất",succeeded:"Hoàn tất",failed:"Lỗi",pending:"Đang xử lý",running:"Đang xử lý"};

export function CadTool({org,pid}:{org:string;pid:string}){
 const [tab,setTab]=useState<Tab>("import"),[rev,setRev]=useState(0);
 return <div className="tool-tabs-shell">
  <div className="tool-tabs" role="tablist">{([["import","Nhập bản vẽ DXF"],["cogo","Tính COGO"],["history","Lịch sử chuyển đổi"]] as [Tab,string][]).map(([k,t])=><button key={k} type="button" role="tab" aria-selected={tab===k} className={tab===k?"active":""} onClick={()=>setTab(k)}>{t}</button>)}</div>
  <div className="tool-tab-pane" role="tabpanel">
   {tab==="import"&&<Suspense fallback={<p>Đang tải…</p>}><CadImport organizationId={org} projectId={pid} onImported={()=>setRev(x=>x+1)}/></Suspense>}
   {tab==="cogo"&&<Cogo org={org} pid={pid}/>}
   {tab==="history"&&<History org={org} pid={pid} rev={rev}/>}
  </div>
 </div>;
}

function Cogo({org,pid}:{org:string;pid:string}){
 const [v,setV]=useState({x:"500000",y:"2300000",d:"100",a:"90"}),[out,setOut]=useState(""),[err,setErr]=useState(""),[busy,setBusy]=useState(false);
 const field=(k:keyof typeof v,label:string,unit:string)=><label className="form-row"><span>{label}</span><input inputMode="decimal" value={v[k]} onChange={e=>setV(s=>({...s,[k]:e.target.value}))}/><small>{unit}</small></label>;
 const calc=async()=>{const nums=[v.x,v.y,v.d,v.a].map(Number);if(nums.some(x=>!Number.isFinite(x))){setErr("Nhập số hợp lệ cho cả 4 ô.");return}setBusy(true);setErr("");setOut("");try{const r=await cogoTraverse(client,org,pid,{origin:[nums[0]!,nums[1]!],distance:nums[2]!,azimuth_degrees:nums[3]!},await loadCsrfToken(client));setOut("X = "+r.endpoint[0].toFixed(4)+"   Y = "+r.endpoint[1].toFixed(4)+"   (ΔX "+r.delta[0].toFixed(4)+", ΔY "+r.delta[1].toFixed(4)+")")}catch(e){setErr(e instanceof ApiError?"Không tính được ("+e.status+").":"Không tính được.")}finally{setBusy(false)}};
 return <form className="tool-form" onSubmit={e=>{e.preventDefault();void calc()}}>
  <p>Tính tọa độ điểm cuối từ điểm gốc, khoảng cách và góc phương vị (mặt phẳng, cùng đơn vị với tọa độ nhập).</p>
  {field("x","Tọa độ gốc X","m")}{field("y","Tọa độ gốc Y","m")}{field("d","Khoảng cách","m")}{field("a","Góc phương vị","độ, từ Bắc theo chiều kim đồng hồ")}
  {out&&<output className="tool-result">{out}</output>}{err&&<p className="error">{err}</p>}
  <div className="tool-buttons"><button type="submit" className="primary" disabled={busy}>{busy?"Đang tính…":"Tính"}</button></div>
 </form>;
}

function History({org,pid,rev}:{org:string;pid:string;rev:number}){
 const [items,setItems]=useState<Awaited<ReturnType<typeof listCadConversions>>["items"]>([]),[err,setErr]=useState(""),[loading,setLoading]=useState(true);
 useEffect(()=>{let live=true;setLoading(true);listCadConversions(client,org,pid).then(r=>{if(live)setItems(r.items)}).catch(()=>{if(live)setErr("Không tải được lịch sử.")}).finally(()=>{if(live)setLoading(false)});return()=>{live=false}},[org,pid,rev]);
 return <>{err&&<p className="error">{err}</p>}<div className="cat-scroll"><table className="cat-table"><thead><tr><th>Hướng</th><th>Tệp</th><th>Định dạng</th><th>CRS</th><th>Trạng thái</th></tr></thead><tbody>{items.map(c=><tr key={c.id}><td>{DIRECTION[c.direction]??c.direction}</td><td title={c.filename}>{c.filename}</td><td>{c.cad_format}</td><td>EPSG:{c.crs_code}</td><td>{STATUS[c.status]??c.status}</td></tr>)}{!items.length&&<tr><td colSpan={5} className="empty">{loading?"Đang tải…":"Chưa có lần chuyển đổi nào."}</td></tr>}</tbody></table></div></>;
}
