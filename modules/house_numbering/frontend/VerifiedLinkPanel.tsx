import {uiConfirm} from "@openvietgis/spatial-workspace/plugin/ui-dialog";
import {useState} from "react";
import {hnClient,uiError,withCsrf} from "./client";
type Status="ready"|"linked"|"other_address"|"blocked"|"stale";
type Item={building_id:string;source_id:string;feature_id:string;decision:"belongs"|"other_address";verified_road:string;verified_prefix:string;evidence:string;distance_m:number|null;raw_road:string|null;raw_alley:string|null;status:Status};
type Plan={state:"current"|"not_promoted"|"stale_route"|"overflow";route:{id:string;name:string;prefix:string;status:string}|null;counts:Record<Status,number>;items:Item[];preview_token:string|null;next_offset:number|null};
const labels:Record<Status,string>={ready:"Sẵn sàng liên kết",linked:"Đã liên kết đúng tuyến",other_address:"Thuộc địa chỉ khác",blocked:"Bị khóa hoặc đã gán tuyến khác",stale:"Bằng chứng đã thay đổi"};
export function VerifiedLinkPanel({endpoint,id,onGoto,onLinked}:{endpoint:string;id:string;onGoto?:((source:string,feature:string)=>void)|undefined;onLinked?:()=>void}){
 const path=endpoint+"/geometry-reviews/"+id+"/verified-links";
 const [plan,setPlan]=useState<Plan|null>(null),[offset,setOffset]=useState(0),[busy,setBusy]=useState(false),[error,setError]=useState(""),[notice,setNotice]=useState(""),[selected,setSelected]=useState<string[]>([]),[confirmed,setConfirmed]=useState(false),[filter,setFilter]=useState<"all"|Status>("all");
 async function preview(page=0){setBusy(true);setError("");setSelected([]);setConfirmed(false);try{setPlan(await hnClient.request<Plan>(path+"?offset="+page+"&limit=100"));setOffset(page)}catch(e){setPlan(null);setError(await uiError(e))}finally{setBusy(false)}}
 function toggle(id:string,checked:boolean){setSelected(old=>checked?[...old,id]:old.filter(x=>x!==id));setConfirmed(false)}
 async function apply(){
  if(!plan?.preview_token||!selected.length||!confirmed||busy)return;
  if(!(await uiConfirm("Liên kết "+selected.length+" nhà đã xác minh vào "+plan.route?.name+"? Không tạo số nhà.")))return;
  setBusy(true);setError("");setNotice("");
  try{const result=await withCsrf(()=>hnClient.request<{linked:number;numbers_created:number}>(path,{method:"POST",body:JSON.stringify({preview_token:plan.preview_token,building_ids:selected,confirmed:true})}));setNotice("Đã liên kết "+result.linked+" nhà. Chưa cấp số; có thể chọn tuyến tại tab Đánh số.");onLinked?.();await preview(offset)}
  catch(e){setError(await uiError(e));setConfirmed(false)}finally{setBusy(false)}
 }
 return <section style={{border:"1px solid var(--ovg-border,#dbe3ec)",borderRadius:8,padding:8,marginTop:8}}>
 <h4>Liên kết nhà đã xác minh</h4>
 <p className="hn-muted">Chỉ nhà có kết luận thuộc tuyến, bằng chứng hiện hành, chưa liên kết và không có số được bảo vệ. Không gán nhà thuộc địa chỉ khác. Không sửa khảo sát hay tạo số ở bước này.</p>
 <button type="button" className="hn-btn secondary" disabled={busy} onClick={()=>void preview(0)}>Xem trước liên kết nhà đã xác minh</button>
 {error&&<p role="alert" className="hn-error">{error}</p>}{notice&&<p role="status">{notice}</p>}
 {plan&&plan.state!=="current"&&<p role="alert">{plan.state==="not_promoted"?"Chưa tìm thấy tuyến đã import khớp hoàn toàn.":plan.state==="stale_route"?"Hình học hoặc cây địa chỉ đã đổi, chưa thể liên kết.":"Vượt giới hạn dữ liệu; cần đối soát lại."}</p>}
 {plan?.state==="current"&&<><p><b>{plan.route?.name}</b> · {plan.route?.status==="confirmed"?"Tuyến đã xác nhận":"Tuyến dự thảo trong danh mục"}</p>
 <div style={{display:"flex",flexWrap:"wrap",gap:8}}>{Object.entries(labels).map(([k,v])=><span key={k} className="hn-badge">{v}: {plan.counts[k as Status]}</span>)}</div>
 <label>Lọc trạng thái liên kết<select aria-label="Lọc trạng thái liên kết" value={filter} onChange={e=>setFilter(e.target.value as typeof filter)}><option value="all">Tất cả</option>{Object.entries(labels).map(([k,v])=><option key={k} value={k}>{v}</option>)}</select></label>
 <div className="hn-actions"><button type="button" className="hn-btn secondary" disabled={busy||!plan.items.some(x=>x.status==="ready")} onClick={()=>{setSelected(plan.items.filter(x=>x.status==="ready").map(x=>x.building_id));setConfirmed(false)}}>Chọn tất cả nhà sẵn sàng trong lô</button><button type="button" className="hn-btn secondary" disabled={busy||!selected.length} onClick={()=>{setSelected([]);setConfirmed(false)}}>Bỏ chọn nhà</button></div>
 <div style={{maxHeight:280,overflow:"auto"}}>{plan.items.filter(x=>filter==="all"||x.status===filter).map(item=><div key={item.building_id} style={{borderBottom:"1px solid var(--ovg-border,#dbe3ec)",padding:5}}>
 <label><input type="checkbox" aria-label={"Liên kết nhà "+item.feature_id} checked={selected.includes(item.building_id)} disabled={busy||item.status!=="ready"} onChange={e=>toggle(item.building_id,e.target.checked)}/> <b>{item.feature_id}</b> · {labels[item.status]} </label><small style={{display:"block"}}>Địa chỉ xác minh: {item.verified_road} / {item.verified_prefix||"mặt tiền"} · {item.distance_m??"—"} m</small>
 <details><summary>Căn cứ xác minh</summary><p>{item.evidence}</p><small>Khảo sát gốc: {item.raw_road||"—"} / {item.raw_alley||"—"}</small></details>
 {onGoto&&<button type="button" className="hn-btn secondary" onClick={()=>onGoto(item.source_id,item.feature_id)}>Xem nhà</button>}
 </div>)}</div>
 <div className="hn-actions"><button type="button" className="hn-btn secondary" disabled={busy||offset===0} onClick={()=>void preview(Math.max(0,offset-100))}>Lô liên kết trước</button><button type="button" className="hn-btn secondary" disabled={busy||plan.next_offset===null} onClick={()=>void preview(plan.next_offset??0)}>Lô liên kết sau</button></div>
 <label><input type="checkbox" checked={confirmed} disabled={busy||!selected.length} onChange={e=>setConfirmed(e.target.checked)}/> Tôi xác nhận liên kết {selected.length} nhà đã chọn vào đúng tuyến</label>
 <button type="button" className="hn-btn" disabled={busy||!selected.length||!confirmed} onClick={()=>void apply()}>Liên kết {selected.length} nhà đã chọn</button>
 {plan.counts.linked>0&&<p className="hn-muted">Nhà đã liên kết xuất hiện trong danh sách mục tiêu đánh số nếu vẫn đủ điều kiện. Mở tab Đánh số và chọn tuyến {plan.route?.name}.</p>}
 </>}
 </section>
}
