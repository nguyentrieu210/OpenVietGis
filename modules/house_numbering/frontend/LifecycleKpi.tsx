import {useState} from "react";
import {getHouseNumberingLifecycleBreakdown,getHouseNumberingSurveyStaffKpi,type LifecycleGroup,type SurveyStaffKpi} from "./api";
import {hnClient,uiError} from "./client";
import {useReloadable} from "./hooks";

const fmt=(n:number)=>n.toLocaleString("vi-VN");
const pct=(n:number,den:number)=>den===0?"—":(100*n/den).toLocaleString("vi-VN",{maximumFractionDigits:1})+"%";
const isoDate=(d:Date)=>d.toISOString().slice(0,10);
function GroupTable({rows}:{rows:LifecycleGroup[]}){
 return <div style={{overflowX:"auto"}}><table className="hn-table"><thead><tr><th>Địa bàn / tuyến</th><th>Công trình</th><th>Đã khảo sát</th><th>Chờ duyệt khảo sát</th><th>Đủ điều kiện</th><th>Đã duyệt số</th><th>Đã ghi chứng nhận</th><th>Lắp biển</th><th>Còn sai khác</th></tr></thead><tbody>
 {rows.map(r=><tr key={r.id??"unassigned"}><td>{r.name}</td><td>{fmt(r.active_buildings)}</td><td>{fmt(r.surveyed_buildings)} ({pct(r.surveyed_buildings,r.active_buildings)})</td><td>{fmt(r.awaiting_review_buildings)}</td><td>{fmt(r.eligible)}</td><td>{fmt(r.approved_buildings)} ({pct(r.approved_buildings,r.eligible)})</td><td>{fmt(r.registered_buildings)} ({pct(r.registered_buildings,r.eligible)})</td><td>{fmt(r.installed_buildings)} ({pct(r.installed_buildings,r.eligible)})</td><td>{fmt(r.unresolved_buildings)}</td></tr>)}
 </tbody></table></div>
}
function StaffKpi({org,project}:{org:string;project:string}){
 const [start,setStart]=useState(()=>isoDate(new Date(Date.now()-29*86400000))),[end,setEnd]=useState(()=>isoDate(new Date(Date.now()+86400000)));
 const [report,setReport]=useState<SurveyStaffKpi|null>(null),[busy,setBusy]=useState(false),[error,setError]=useState("");
 const load=async()=>{setBusy(true);setError("");setReport(null);try{setReport(await getHouseNumberingSurveyStaffKpi(hnClient,org,project,start,end))}catch(e){setError(await uiError(e))}finally{setBusy(false)}};
 return <section className="hn-card"><h3>KPI nhân viên khảo sát (quản lý)</h3>
 <div className="hn-form"><label>Từ ngày UTC<input type="date" value={start} disabled={busy} onChange={e=>{setStart(e.target.value);setReport(null)}}/></label><label>Đến trước ngày UTC<input type="date" value={end} disabled={busy} onChange={e=>{setEnd(e.target.value);setReport(null)}}/></label>
 <button type="button" disabled={busy||!start||!end||start>=end} onClick={()=>void load()}>{busy?"Đang tải…":"Xem KPI nhân viên"}</button></div>
 {error&&<p role="alert">{error}</p>}
 {report&&<><p>Toàn dự án trong kỳ: {fmt(report.distinct_observed_buildings)} công trình ghi nhận; {fmt(report.distinct_accepted_buildings)} công trình có phiếu được chấp nhận.</p>
 <div style={{overflowX:"auto"}}><table className="hn-table"><thead><tr><th>Người khảo sát</th><th>Phiếu gửi</th><th>Chấp nhận</th><th>Từ chối</th><th>Chờ duyệt</th><th>Công trình ghi nhận</th><th>Công trình chấp nhận</th><th>Tỷ lệ chấp nhận trên đã duyệt</th></tr></thead><tbody>
 {report.items.map(r=><tr key={r.actor_id}><td>{r.display_name}</td><td>{fmt(r.submitted_forms)}</td><td>{fmt(r.accepted_forms)}</td><td>{fmt(r.rejected_forms)}</td><td>{fmt(r.pending_forms)}</td><td>{fmt(r.observed_buildings)}</td><td>{fmt(r.accepted_buildings)}</td><td>{pct(r.accepted_forms,r.accepted_forms+r.rejected_forms)}</td></tr>)}</tbody></table></div>
 {report.items.length===0&&<p>Không có phiếu trong kỳ.</p>}</>}
 <p className="hn-muted">Thống kê theo thời điểm phiếu được ghi vào máy chủ, trạng thái duyệt hiện tại và người khảo sát ghi trên phiếu; không phải giờ công hay ngày chụp ảnh. Không cộng tổng số công trình của từng người để suy ra tổng dự án.</p></section>
}
export function LifecycleKpi({organizationId:org,projectId:project}:{organizationId:string;projectId:string}){
 const k=useReloadable(()=>getHouseNumberingLifecycleBreakdown(hnClient,org,project),[org,project]);
 const [dimension,setDimension]=useState<"areas"|"routes">("areas");
 const [search,setSearch]=useState("");
 const d=k.data?.total;
 return <><section className="hn-card"><h3>Tiến độ theo công trình</h3>
 <button type="button" onClick={()=>void k.reload()} disabled={k.loading}>Cập nhật KPI</button>
 {k.loading?<p>Đang tải số liệu…</p>:k.error?<p role="alert">{k.error}</p>:d&&k.data?<><p>Tổng công trình đang hoạt động: <strong>{fmt(d.active_buildings)}</strong>. Chưa xác minh điều kiện: {fmt(d.unknown)}; không đủ điều kiện: {fmt(d.ineligible)}; còn sai khác: {fmt(d.unresolved_buildings)}.</p>
 <table className="hn-table"><thead><tr><th>Công đoạn</th><th>Công trình</th><th>Mẫu số</th><th>Tỷ lệ</th></tr></thead><tbody>
 {([["Khảo sát được chấp nhận",d.surveyed_buildings,d.active_buildings],["Đủ điều kiện",d.eligible,d.active_buildings],["Có số được duyệt",d.approved_buildings,d.eligible],["Có chứng nhận ghi nhận còn hiệu lực trong sổ",d.registered_buildings,d.eligible],["Đã lắp biển",d.installed_buildings,d.eligible]] as [string,number,number][]).map(([label,n,den])=><tr key={label}><td>{label}</td><td>{fmt(n)}</td><td>{fmt(den)}</td><td>{pct(n,den)}</td></tr>)}
 </tbody></table>
 <label>Phân tích theo <select value={dimension} onChange={e=>{setDimension(e.target.value as "areas"|"routes");setSearch("")}}><option value="areas">Địa bàn trực tiếp</option><option value="routes">Tuyến đường</option></select></label>
 <label>Lọc tên <input value={search} onChange={e=>setSearch(e.target.value)}/></label>
 <GroupTable rows={k.data[dimension].filter(r=>r.name.toLocaleLowerCase("vi-VN").includes(search.toLocaleLowerCase("vi-VN").trim()))}/>
 <p className="hn-muted">Địa bàn tính theo gán trực tiếp, không cộng lồng cấp cha/con. Hai chiều phân tích là hai cách chia cùng một tổng, không cộng với nhau. Công trình chưa gán có nhóm riêng. Mẫu số bằng 0 hiển thị —.</p></>:null}</section><StaffKpi key={org+":"+project} org={org} project={project}/></>
}
