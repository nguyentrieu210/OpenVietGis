import {useState} from "react";
import {hnClient,uiError,withCsrf} from "./client";
type Input={building_id:string;cluster:string};
type Distance={distance_m:number;within_limit:boolean};
type Candidate={route_id:string;name:string;root_name:string;prefix:string|null;status:string;within_limit:number;mean_distance_m:number|null};
type Row={building_id:string;source_id:string;feature_id:string;road:string;alley:string;code:string;eligible:boolean;blocked_reason:string|null;distances:Record<string,Distance>;old_distances:Record<string,Distance>};
type Plan={cluster:string;items:Row[];candidates:Candidate[];original_routes:Candidate[];recommended_route_id:string|null;preview_token:string;max_distance_m:number};
function path(c:Candidate){return c.root_name+(c.prefix?" / "+c.prefix:"");}
export function ClusterCorrectionPanel({endpoint,cluster,items,mapping,onDone,onGoto,onBusy}:{endpoint:string;cluster:string;items:Input[];mapping:Record<string,string>;onDone:(assigned:number)=>void;onGoto?:((source:string,feature:string)=>void)|undefined;onBusy:(busy:boolean)=>void}){
 const [plan,setPlan]=useState<Plan|null>(null),[target,setTarget]=useState(""),[chosen,setChosen]=useState<string[]>([]),[reason,setReason]=useState(""),[checked,setChecked]=useState(false),[busy,setBusy]=useState(false),[error,setError]=useState("");
 const ids=items.filter(i=>i.cluster===cluster).map(i=>i.building_id);
 const body={cluster,building_ids:ids,mapping,max_distance_m:35};
 const selectTarget=(p:Plan,rid:string)=>{setTarget(rid);setChosen(p.items.filter(i=>i.eligible&&i.distances[rid]?.within_limit).map(i=>i.building_id));setChecked(false)};
 async function preview(){setBusy(true);onBusy(true);setError("");setPlan(null);setChosen([]);setTarget("");setChecked(false);try{
   const res=await withCsrf(()=>hnClient.request<Plan>(endpoint+"/correction/preview",{method:"POST",body:JSON.stringify(body)}));setPlan(res);
   if(res.recommended_route_id)selectTarget(res,res.recommended_route_id);
 }catch(e){setError(await uiError(e));}finally{setBusy(false);onBusy(false)}}
 async function apply(){if(!plan||!target||!checked||!chosen.length||reason.trim().length<10||!window.confirm(`Xác nhận sửa liên kết tuyến của ${chosen.length} công trình? Mã khảo sát gốc giữ nguyên.`))return;setBusy(true);onBusy(true);setError("");try{
   const res=await withCsrf(()=>hnClient.request<{assigned:number}>(endpoint+"/correction/apply",{method:"POST",body:JSON.stringify({...body,route_id:target,selected_ids:chosen,reason:reason.trim(),preview_token:plan.preview_token})}));
   onDone(res.assigned);setPlan(null);
 }catch(e){setError(await uiError(e));}finally{setBusy(false);onBusy(false)}}
 const available=plan?.items.filter(i=>i.eligible&&i.distances[target]?.within_limit)??[];
 return <section className="hn-card" style={{marginTop:12,borderColor:"#0d9488"}}>
  <h3>Sửa liên kết mã tuyến theo cụm</h3>
  <p className="hn-muted">{ids.length} nhà trong lô được đối chiếu đúng tên đường/hẻm với cây tuyến. Không đổi thuộc tính khảo sát, không cấp số nhà; chỉ sửa liên kết tuyến nghiệp vụ. Giới hạn 35 m.</p>
  <button type="button" className="hn-btn" disabled={busy||!ids.length} onClick={()=>void preview()}>{busy?"Đang kiểm tra…":"Xem trước sửa mã tuyến cả cụm"}</button>
  {error&&<p role="alert" className="hn-error">{error}</p>}
  {plan&&<>
   <p><b>Mã nguồn:</b> {plan.items[0]?.code||"—"} · {plan.original_routes.map(path).join("; ")||"Chưa có tuyến"}</p>
   <p><b>Địa chỉ khảo sát:</b> {plan.items[0]?.road}{plan.items[0]?.alley?" / "+plan.items[0]?.alley:""}</p>
   {!plan.candidates.length?<p className="hn-warning">Không có tuyến khớp đường/hẻm. Hãy phục hồi hoặc sửa danh mục tuyến trước.</p>:<>
    {plan.candidates.length>1&&<p className="hn-warning">Có nhiều tuyến trùng địa chỉ. Phải xác minh tuyến đích; không tự chọn theo khoảng cách.</p>}
    <label>Tuyến đích đã đối soát<select aria-label="Tuyến đích đã đối soát" disabled={busy} value={target} onChange={e=>selectTarget(plan,e.target.value)}>
     <option value="">Chọn tuyến đúng</option>{plan.candidates.map(c=><option key={c.route_id} value={c.route_id}>{path(c)} · {c.within_limit}/{plan.items.length} nhà ≤35m · {c.mean_distance_m===null?"—":c.mean_distance_m+"m TB"} · {c.status==="draft"?"Dự thảo":"Xác nhận"}</option>)}
    </select></label>
    {target&&<>
     <p><b>{available.length}</b> nhà đạt khoảng cách và không bị khóa · {plan.items.length-available.length} nhà chưa thể sửa trong lô này.</p>
     <div className="hn-actions"><button type="button" className="hn-btn secondary" disabled={busy} onClick={()=>{setChosen(available.map(r=>r.building_id));setChecked(false)}}>Chọn tất cả nhà đạt</button><button type="button" className="hn-btn secondary" disabled={busy} onClick={()=>{setChosen([]);setChecked(false)}}>Bỏ chọn</button></div>
     <div style={{overflow:"auto",maxHeight:350}}><table className="hn-table"><thead><tr><th>Chọn</th><th>Nhà</th><th>Khoảng cách mã cũ</th><th>Khoảng cách tuyến đích</th><th>Trạng thái</th><th>Bản đồ</th></tr></thead><tbody>{plan.items.map(r=>{
      const d=r.distances[target];const may=r.eligible&&!!d?.within_limit;
      return <tr key={r.building_id}><td><input type="checkbox" aria-label={"Chọn sửa "+r.feature_id} disabled={busy||!may} checked={chosen.includes(r.building_id)} onChange={e=>{setChosen(s=>e.target.checked?[...s,r.building_id]:s.filter(i=>i!==r.building_id));setChecked(false)}}/></td><td>{r.feature_id}</td><td>{Object.values(r.old_distances).map(x=>x.distance_m+" m").join(", ")||"—"}</td><td>{d?d.distance_m+" m":"—"}</td><td>{may?"Đủ điều kiện":r.blocked_reason==="protected_number"?"Có số bảo vệ":r.blocked_reason==="ineligible"?"Không đủ điều kiện cấp":r.blocked_reason?"Thiếu hình học":"Xa tuyến"}</td><td>{onGoto&&<button type="button" className="hn-btn secondary" onClick={()=>onGoto(r.source_id,r.feature_id)}>Xem nhà</button>}</td></tr>
     })}</tbody></table></div>
     <label>Lý do đối soát<textarea aria-label="Lý do đối soát" disabled={busy} maxLength={500} value={reason} onChange={e=>{setReason(e.target.value);setChecked(false)}} placeholder="Ví dụ: Đã đối chiếu địa chỉ với cây tuyến và cửa vào thực tế."/></label>
     <label><input type="checkbox" disabled={busy} checked={checked} onChange={e=>setChecked(e.target.checked)}/> Tôi đã xác minh đúng tuyến đích và danh sách nhà được chọn.</label>
     <button type="button" className="hn-btn" disabled={busy||!checked||reason.trim().length<10||!chosen.length} onClick={()=>void apply()}>Xác nhận sửa {chosen.length} nhà</button>
    </>}
   </>}
  </>}
 </section>
}
