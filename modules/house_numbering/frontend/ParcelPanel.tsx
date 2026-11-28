import {useState} from "react";
import {createParcel,listParcels,listParcelLinks,transitionParcelLink,type HouseNumberingBuilding,type ParcelLinkState} from "./api";
import {hnClient,uiError,withCsrf} from "./client";
import {useReloadable} from "./hooks";
const labels:Record<ParcelLinkState,string>={observed:"Chờ xác nhận",confirmed:"Đã xác nhận",rejected:"Bác bỏ",detached:"Đã ngừng liên kết"};
export function ParcelPanel({organizationId:org,projectId:project,buildings,sources,onGoto}:{organizationId:string;projectId:string;buildings:HouseNumberingBuilding[];sources:{id:string;name:string;geometry_family:string}[];onGoto?:((s:string,f:string)=>void)|undefined}){
 const [offset,setOffset]=useState(0),[source,setSource]=useState(""),[feature,setFeature]=useState(""),[reference,setReference]=useState(""),[evidence,setEvidence]=useState("");
 const [building,setBuilding]=useState(""),[parcel,setParcel]=useState(""),[reason,setReason]=useState(""),[busy,setBusy]=useState(false),[error,setError]=useState("");
 const parcels=useReloadable(()=>listParcels(hnClient,org,project,offset),[org,project,offset]);
 const links=useReloadable(()=>building?listParcelLinks(hnClient,org,project,building):Promise.resolve({items:[]}),[org,project,building]);
 const rows=links.data?.items??[],current=rows.find(r=>r.parcel_id===parcel);
 const run=async(fn:()=>Promise<void>)=>{setBusy(true);setError("");try{await withCsrf(fn)}catch(e){setError(await uiError(e))}finally{setBusy(false)}};
 const move=(id:string,revision:number,state:ParcelLinkState)=>void run(async()=>{await transitionParcelLink(hnClient,org,project,building,id,{expected_revision:revision,state,evidence_ref:reason.trim()});setReason("");await links.reload()});
 return <section className="hn-card"><h2>Sổ thửa đất và liên kết công trình</h2><p>Định danh thửa riêng với công trình; ghi nhận theo nguồn bản đồ và bằng chứng. Không xác lập quyền sử dụng đất, không tự thay đổi số nhà.</p>
 <div className="hn-form"><label>Lớp polygon<select value={source} disabled={busy} onChange={e=>setSource(e.target.value)}><option value="">Chọn lớp</option>{sources.filter(s=>s.geometry_family==="Polygon").map(s=><option key={s.id} value={s.id}>{s.name}</option>)}</select></label>
 <label>Feature ID<input value={feature} maxLength={128} disabled={busy} onChange={e=>setFeature(e.target.value)}/></label>
 <label>Tham chiếu thửa<input value={reference} maxLength={200} disabled={busy} onChange={e=>setReference(e.target.value)}/></label>
 <label>Bằng chứng nguồn<input value={evidence} maxLength={500} disabled={busy} onChange={e=>setEvidence(e.target.value)}/></label>
 <button type="button" disabled={busy||!source||!feature||!reference.trim()||!evidence.trim()} onClick={()=>void run(async()=>{await createParcel(hnClient,org,project,{source_id:source,feature_id:feature,reference,evidence_ref:evidence});setFeature("");setReference("");setEvidence("");await parcels.reload()})}>Đăng ký thửa</button></div>
 <label>Công trình<select value={building} disabled={busy} onChange={e=>{setBuilding(e.target.value);setReason("")}}><option value="">Chọn công trình</option>{buildings.map(b=><option key={b.id} value={b.id}>{b.linked_feature_id} · {b.id.slice(0,8)}</option>)}</select></label>
 <label>Thửa đã đăng ký<select value={parcel} disabled={busy||parcels.loading} onChange={e=>setParcel(e.target.value)}><option value="">Chọn thửa</option>{parcels.data?.items.map(p=><option key={p.id} value={p.id} disabled={!p.geometry_ready}>{p.reference} · {p.feature_id}</option>)}</select></label>
 <button type="button" disabled={busy||offset===0} onClick={()=>{setParcel("");setOffset(v=>Math.max(0,v-100))}}>Trang trước</button>
 <button type="button" disabled={busy||parcels.data?.next_offset==null} onClick={()=>{setParcel("");setOffset(v=>v+100)}}>Trang sau</button>
 <label>Bằng chứng / lý do<input value={reason} disabled={busy} maxLength={500} onChange={e=>setReason(e.target.value)}/></label>
 <button type="button" disabled={busy||links.loading||!!links.error||!building||!parcel||!reason.trim()||!!current&&!["rejected","detached"].includes(current.state)} onClick={()=>move(parcel,current?.revision??0,"observed")}>Ghi nhận liên kết</button>
 {links.loading?<p>Đang tải liên kết…</p>:<table className="hn-table"><thead><tr><th>Thửa</th><th>Trạng thái</th><th>Lịch sử / thao tác</th></tr></thead><tbody>{rows.map(r=><tr key={r.parcel_id}><td>{r.reference} · {r.feature_id}</td><td>{labels[r.state]}{!r.geometry_current&&" · Hình học đã thay đổi"}</td><td><details><summary>Lịch sử ({r.history.length})</summary>{r.history.map(h=><p key={h.revision}>{labels[h.state]} · {h.evidence_ref} · {new Date(h.occurred_at).toLocaleString("vi-VN")}</p>)}</details>
 {onGoto&&<button type="button" onClick={()=>onGoto(r.source_id,r.feature_id)}>Xem thửa</button>}
 {(r.state==="observed"?["confirmed","rejected"] as const:r.state==="confirmed"?["detached"] as const:[]).map(state=><button key={state} type="button" disabled={busy||!reason.trim()||(state==="confirmed"&&!r.geometry_current)} onClick={()=>move(r.parcel_id,r.revision,state)}>{labels[state]}</button>)}</td></tr>)}</tbody></table>}
 {(error||links.error||parcels.error)&&<p role="alert">{error||links.error||parcels.error}</p>}</section>
}
