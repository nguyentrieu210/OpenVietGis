import {useEffect,useState} from "react";
import {listBuildingEntrances,observeBuildingEntrance,reviewBuildingEntrance,type HouseNumberingBuilding,type HouseNumberingRoute,type BuildingEntrance,type EntranceState} from "./api";
import {hnClient,uiError,withCsrf} from "./client";

type Props={organizationId:string;projectId:string;buildings:HouseNumberingBuilding[];routes:HouseNumberingRoute[];sources:{id:string;name:string;geometry_family:string}[];onGoto?:((source:string,feature:string)=>void)|undefined};
const labels:Record<EntranceState,string>={observed:"Chờ xác nhận",confirmed:"Đã xác nhận",rejected:"Bác bỏ",retired:"Ngừng sử dụng"};
export function EntrancePanel({organizationId:org,projectId:project,buildings,routes,sources,onGoto}:Props){
 const [building,setBuilding]=useState(""),[route,setRoute]=useState(""),[source,setSource]=useState(""),[feature,setFeature]=useState(""),[evidence,setEvidence]=useState(""),[number,setNumber]=useState("");
 const [items,setItems]=useState<BuildingEntrance[]>([]),[error,setError]=useState(""),[busy,setBusy]=useState(false),[reason,setReason]=useState("");
 useEffect(()=>{let alive=true;setItems([]);setError("");if(building)listBuildingEntrances(hnClient,org,project,building).then(r=>{if(alive)setItems(r.items)}).catch(async e=>{if(alive)setError(await uiError(e))});return()=>{alive=false}},[org,project,building]);
 const run=async(fn:()=>Promise<unknown>)=>{setBusy(true);setError("");try{await withCsrf(fn);setItems((await listBuildingEntrances(hnClient,org,project,building)).items)}catch(e){setError(await uiError(e))}finally{setBusy(false)}};
 const current=buildings.find(b=>b.id===building);
 return <section className="hn-card"><h2>Lối vào công trình / nhà góc đường</h2><p>Ghi nhận từng lối vào theo điểm GIS và tuyến thực tế. Lối vào khác tuyến chính hoặc chưa xác nhận cần xử lý trước khi đánh số; không tự tạo địa chỉ pháp lý.</p>
 <div className="hn-form">
 <label>Công trình<select disabled={busy} value={building} onChange={e=>{setBuilding(e.target.value);setRoute(buildings.find(b=>b.id===e.target.value)?.route_id??"");setReason("")}}><option value="">Chọn công trình</option>{buildings.map(b=><option key={b.id} value={b.id}>{b.parcel_ref||b.linked_feature_id} · {b.id.slice(0,8)}</option>)}</select></label>
 <label>Tuyến tiếp cận<select disabled={busy} value={route} onChange={e=>setRoute(e.target.value)}><option value="">Chọn tuyến</option>{routes.map(r=><option key={r.id} value={r.id}>{r.name}</option>)}</select></label>
 <label>Lớp điểm lối vào<select disabled={busy} value={source} onChange={e=>setSource(e.target.value)}><option value="">Chọn lớp điểm</option>{sources.filter(s=>s.geometry_family==="Point").map(s=><option key={s.id} value={s.id}>{s.name}</option>)}</select></label>
 <label>Feature ID<input value={feature} maxLength={128} disabled={busy} onChange={e=>setFeature(e.target.value)}/></label>
 <label>Số quan sát (nếu có)<input value={number} maxLength={200} disabled={busy} onChange={e=>setNumber(e.target.value)}/></label>
 <label>Tham chiếu bằng chứng<input value={evidence} maxLength={500} disabled={busy} onChange={e=>setEvidence(e.target.value)}/></label>
 <button type="button" disabled={busy||!building||!route||!source||!feature.trim()||!evidence.trim()} onClick={()=>void run(async()=>{await observeBuildingEntrance(hnClient,org,project,building,{route_id:route,source_id:source,feature_id:feature,evidence_ref:evidence,observed_number:number.trim()?number:null});setFeature("");setEvidence("");setNumber("")})}>Ghi nhận lối vào</button></div>
 <label>Lý do xác nhận / bác bỏ / ngừng sử dụng<input value={reason} disabled={busy} maxLength={500} onChange={e=>setReason(e.target.value)}/></label>
 <table className="hn-table"><thead><tr><th>Tuyến / điểm</th><th>Số quan sát</th><th>Trạng thái</th><th>Thao tác</th></tr></thead><tbody>{items.map(i=><tr key={i.id}><td>{i.route_name} · {i.feature_id}{current?.route_id!==i.route_id&&<small> · Khác tuyến chính</small>}</td><td>{i.observed_number??"—"}</td><td>{labels[i.state]}{!i.geometry_ready&&" · Điểm GIS không khả dụng"}</td><td>{onGoto&&<button type="button" onClick={()=>onGoto(i.source_id,i.feature_id)}>Xem điểm</button>}{(i.state==="observed"?["confirmed","rejected"] as const:i.state==="confirmed"?["retired"] as const:[]).map(state=><button key={state} type="button" disabled={busy||!reason.trim()} onClick={()=>void run(async()=>{await reviewBuildingEntrance(hnClient,org,project,i.id,{expected_revision:i.revision,state,reason});setReason("")})}>{labels[state]}</button>)}</td></tr>)}</tbody></table>
 {error&&<p role="alert">{error}</p>}</section>
}
