import {useState} from "react";
import {listAddressHistory,retireAddress,linkAddressSuccessor} from "./api";
import {hnClient,uiError,withCsrf} from "./client";
import {useReloadable} from "./hooks";
export function AddressHistoryPanel({org,project,building,number,proposal,onChange}:{org:string;project:string;building:string;number:string;proposal:string;onChange:()=>Promise<void>}){
 const history=useReloadable(()=>listAddressHistory(hnClient,org,project,building),[org,project,building]);
 const [date,setDate]=useState(""),[decision,setDecision]=useState(""),[reason,setReason]=useState(""),[busy,setBusy]=useState(false),[error,setError]=useState("");
 const [confirm,setConfirm]=useState("");
 const rows=history.data?.items??[],current=rows.find(r=>r.id===number&&!r.retired_at);
 const run=async(fn:()=>Promise<unknown>)=>{setBusy(true);setError("");try{await withCsrf(fn);await history.reload();await onChange();setConfirm("");setDecision("");setReason("");}catch(e){setError(await uiError(e))}finally{setBusy(false)}};
 return <section className="hn-card"><h3>Lịch sử biến động số nhà</h3><p>Chỉ ghi nhận quyết định đã có hiệu lực. Ngừng sử dụng số cũ sẽ vô hiệu hóa chứng nhận trong hệ thống; số mới phải được duyệt qua phương án riêng. Không tự thu hồi quyết định pháp lý.</p>
 <label>Ngày hiệu lực<input type="date" value={date} disabled={busy} onChange={e=>setDate(e.target.value)}/></label>
 <label>Tham chiếu quyết định<input value={decision} maxLength={200} disabled={busy} onChange={e=>setDecision(e.target.value)}/></label>
 <label>Lý do ngừng sử dụng<input value={reason} maxLength={500} disabled={busy} onChange={e=>setReason(e.target.value)}/></label>
 {history.loading?<p>Đang tải…</p>:<table className="hn-table"><thead><tr><th>Số / tuyến</th><th>Trạng thái</th><th>Quyết định</th><th>Thao tác</th></tr></thead><tbody>{rows.map(r=><tr key={r.id}><td>{r.number_text} · {r.route_name}</td><td>{r.retired_at?"Đã ngừng sử dụng":"Đã duyệt, chưa ngừng sử dụng"}</td><td>{r.retirement&&<p>{r.retirement.effective_date} · {r.retirement.decision_ref} · {r.retirement.reason}</p>}{r.successor&&<p>Thay thế bằng: {rows.find(n=>n.id===r.successor?.new_number_id)?.number_text??"Số đã liên kết"} · {r.successor.decision_ref}</p>}</td><td>
 {!r.retired_at&&(confirm===r.id?<><button type="button" disabled={busy||!date||!decision.trim()||!reason.trim()} onClick={()=>void run(()=>retireAddress(hnClient,org,project,r.id,{expected_revision:r.revision,effective_date:date,decision_ref:decision.trim(),reason:reason.trim()}))}>Xác nhận ngừng sử dụng và vô hiệu hóa chứng nhận</button><button type="button" disabled={busy} onClick={()=>setConfirm("")}>Hủy</button></>:<button type="button" disabled={busy} onClick={()=>setConfirm(r.id)}>Ngừng sử dụng</button>)}
 {r.retirement&&!r.successor&&current&&current.id!==r.id&&<button type="button" disabled={busy||!date||!decision.trim()} onClick={()=>void run(()=>linkAddressSuccessor(hnClient,org,project,r.id,{new_number_id:current.id,proposal_id:proposal,effective_date:date,decision_ref:decision.trim()}))}>Liên kết số mới đang chọn: {current.number_text}</button>}
 </td></tr>)}</tbody></table>}
 {(error||history.error)&&<p role="alert">{error||history.error}</p>}</section>
}
