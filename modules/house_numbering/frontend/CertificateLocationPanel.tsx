import {useState} from "react";
import {listCertificateLocations,listHouseNumberingRoutes,type CertificateLocation} from "./api";
import {hnClient,uiError} from "./client";
import {useReloadable} from "./hooks";
const labels:Record<string,string>={estimated:"Vị trí ước tính",unsupported_number:"Chưa hỗ trợ dạng số",route_unavailable:"Tuyến chưa có hình học phù hợp",number_already_linked:"Số đã có công trình liên kết",missing_bracket:"Thiếu hai mốc cùng chẵn/lẻ",bracket_too_wide:"Khoảng số quá rộng",different_or_uncertain_side:"Khác phía hoặc phía không rõ",implausible_spacing:"Khoảng cách không hợp lý"};
export function CertificateLocationPanel({organizationId:org,projectId:project}:{organizationId:string;projectId:string}){
 const routes=useReloadable(()=>listHouseNumberingRoutes(hnClient,org,project),[org,project]);
 const [route,setRoute]=useState(""),[crs,setCrs]=useState(32648),[offset,setOffset]=useState(0),[next,setNext]=useState<number|null>(null),[items,setItems]=useState<CertificateLocation[]|null>(null),[busy,setBusy]=useState(false),[error,setError]=useState("");
 const run=async(start:number)=>{setBusy(true);setItems(null);setError("");try{const result=await listCertificateLocations(hnClient,org,project,route,crs,start);setItems(result.items);setOffset(start);setNext(result.next_offset)}catch(e){setError(await uiError(e))}finally{setBusy(false)}};
 return <section className="hn-card"><h2>Chứng nhận chưa ghép – vị trí tham khảo</h2><p>Ước tính dọc tuyến giữa hai số cùng chẵn/lẻ, cùng phía. Không ghi vào lớp công trình và không dùng làm căn cứ cấp số.</p>
 <label>Tuyến đã xác nhận<select value={route} disabled={busy} onChange={e=>{setRoute(e.target.value);setItems(null);setOffset(0)}}><option value="">Chọn tuyến</option>{routes.data?.items.filter(r=>r.status==="confirmed"&&r.route_type==="street").map(r=><option key={r.id} value={r.id}>{r.name}</option>)}</select></label>
 <label>Hệ mét<select value={crs} disabled={busy} onChange={e=>{setCrs(Number(e.target.value));setItems(null);setOffset(0)}}><option value={32648}>UTM 48N</option><option value={32649}>UTM 49N</option></select></label>
 <button type="button" disabled={!route||busy} onClick={()=>void run(0)}>{busy?"Đang tính…":"Xem vị trí ước tính"}</button>
 {items&&<><table className="hn-table"><thead><tr><th>Số chứng nhận</th><th>Kết quả</th><th>Mốc dưới / trên</th><th>Tọa độ tham khảo (WGS84)</th></tr></thead><tbody>{items.map(r=><tr key={r.evidence_id}><td>{r.number??"—"}</td><td>{labels[r.status]??r.status}</td><td>{r.lower_number??"—"} / {r.upper_number??"—"}</td><td>{r.latitude!=null&&r.longitude!=null?<>{r.latitude.toFixed(6)}, {r.longitude.toFixed(6)}</>:"—"}</td></tr>)}</tbody></table>{!items.length&&<p>Không có dòng chứng nhận chưa ghép trong trang này.</p>}<button type="button" disabled={busy||offset===0} onClick={()=>void run(Math.max(0,offset-200))}>Trang trước</button><button type="button" disabled={busy||next===null} onClick={()=>void run(next!)}>Trang sau</button></>}
 {(error||routes.error)&&<p role="alert">{error||routes.error}</p>}
 <p className="hn-muted">Chỉ hỗ trợ số nguyên trên tuyến đường đơn đã xác nhận. Chưa ngoại suy đầu/cuối tuyến, chưa tự ghép nhà và chưa lưu vị trí.</p></section>
}
