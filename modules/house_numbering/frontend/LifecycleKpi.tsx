import {getHouseNumberingLifecycleKpi} from "./api";
import {hnClient} from "./client";
import {useReloadable} from "./hooks";

export function LifecycleKpi({organizationId:org,projectId:project}:{organizationId:string;projectId:string}){
 const k=useReloadable(()=>getHouseNumberingLifecycleKpi(hnClient,org,project),[org,project]);
 const d=k.data;
 const pct=(n:number,den:number)=>den===0?"—":(100*n/den).toLocaleString("vi-VN",{maximumFractionDigits:1})+"%";
 return <section className="hn-card"><h3>Tiến độ theo công trình</h3>
 <button type="button" onClick={()=>void k.reload()} disabled={k.loading}>Cập nhật KPI</button>
 {k.loading?<p>Đang tải số liệu…</p>:k.error?<p role="alert">{k.error}</p>:d?<><p>Tổng công trình đang hoạt động: <strong>{d.active_buildings.toLocaleString("vi-VN")}</strong>. Chưa xác minh điều kiện: {d.unknown.toLocaleString("vi-VN")}; không đủ điều kiện: {d.ineligible.toLocaleString("vi-VN")}; còn sai khác: {d.unresolved_buildings.toLocaleString("vi-VN")}.</p>
 <table className="hn-table"><thead><tr><th>Công đoạn</th><th>Công trình</th><th>Mẫu số</th><th>Tỷ lệ</th></tr></thead><tbody>
 {([["Đủ điều kiện",d.eligible,d.active_buildings],["Có số được duyệt",d.approved_buildings,d.eligible],["Có chứng nhận ghi nhận còn hiệu lực trong sổ",d.registered_buildings,d.eligible],["Đã lắp biển",d.installed_buildings,d.eligible]] as [string,number,number][]).map(([label,n,den])=><tr key={label}><td>{label}</td><td>{n.toLocaleString("vi-VN")}</td><td>{den.toLocaleString("vi-VN")}</td><td>{pct(n,den)}</td></tr>)}
 </tbody></table><p className="hn-muted">Đếm mỗi công trình một lần; không cộng dồn hồ sơ cấp lại, bản vô hiệu hoặc số không còn khớp địa chỉ. Mẫu số bằng 0 hiển thị dấu —, không phải 100%.</p></>:null}</section>
}
