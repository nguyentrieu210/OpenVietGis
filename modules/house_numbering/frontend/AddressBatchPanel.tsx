import {uiConfirm} from "../../../apps/web-next/src/ui-dialog";
import {useEffect,useState} from "react";
import {hnClient,uiError,withCsrf} from "./client";
export type BatchGroup={key:string;kind:string;road:string;alley:string;code:string;verified_road:string;verified_prefix:string;count:number;distance_min:number;distance_max:number;members:{building_id:string;source_id:string;feature_id:string;road:string;alley:string;distance_m:number;evidence_token:string;revision:number}[]};
export function AddressBatchPanel({path,groups,busy,onBusy,onSaved,onGoto}:{path:string;groups:BatchGroup[];busy:boolean;onBusy:(value:boolean)=>void;onSaved:(count:number)=>Promise<void>;onGoto?:((source:string,feature:string)=>void)|undefined}){
 const [key,setKey]=useState(""),[selected,setSelected]=useState<string[]>([]),[evidence,setEvidence]=useState(""),[confirmed,setConfirmed]=useState(false),[error,setError]=useState("");
 useEffect(()=>{setKey("");setSelected([]);setEvidence("");setConfirmed(false);setError("")},[groups]);
 const group=groups.find(g=>g.key===key);
 function open(g:BatchGroup){setKey(g.key);setSelected([]);setEvidence("");setConfirmed(false);setError("")}
 function toggle(id:string,checked:boolean){setSelected(old=>checked?[...old,id]:old.filter(x=>x!==id));setConfirmed(false)}
 async function save(){
  if(!group||selected.length<2||evidence.trim().length<10||!confirmed||busy)return;
  const members=group.members.filter(m=>selected.includes(m.building_id)).map(m=>({building_id:m.building_id,evidence_token:m.evidence_token,expected_revision:m.revision}));
  if(!(await uiConfirm("Xác nhận căn cứ chung đã được kiểm tra cho "+members.length+" nhà đã chọn? Hồ sơ gốc và tuyến chính vẫn không thay đổi.")))return;
  onBusy(true);setError("");
  try{const result=await withCsrf(()=>hnClient.request<{verified:number}>(path+"/batch",{method:"POST",body:JSON.stringify({key:group.key,members,evidence:evidence.trim(),confirmed:true})}));
   setKey("");setSelected([]);setEvidence("");setConfirmed(false);await onSaved(result.verified);
  }catch(e){setError(await uiError(e));setConfirmed(false)}finally{onBusy(false)}
 }
 if(!groups.length)return null;
 return <section style={{border:"1px solid var(--ovg-border,#dbe3ec)",borderRadius:8,padding:8,marginBottom:8}}>
 <h4>Đối soát hàng loạt theo nhóm</h4>
 <p><b>{groups.length} nhóm · {groups.reduce((n,g)=>n+g.count,0)} nhà đủ điều kiện xem xét theo nhóm trong lô</b></p>
 <p className="hn-muted">Chỉ nhóm thiếu trường, cùng mã khảo sát và gần tuyến ≤35 m. Đây không phải kết luận địa chỉ; chỉ chọn các nhà đã có cùng căn cứ thực tế. Nhà mâu thuẫn xử lý riêng.</p>
 {error&&<p role="alert" className="hn-error">{error}</p>}
 <div style={{maxHeight:180,overflow:"auto"}}>{groups.map(g=><div key={g.key} style={{borderBottom:"1px solid var(--ovg-border,#dbe3ec)",padding:5}}>
 <b>{g.kind==="missing_prefix"?"Thiếu tiền tố":"Thiếu tên đường"} · {g.count} nhà</b> · {g.road||"—"} / {g.alley||"—"}<small style={{display:"block"}}>Mã nguồn: {g.code} · {g.distance_min}–{g.distance_max} m</small>
 <button type="button" className="hn-btn secondary" disabled={busy} onClick={()=>open(g)}>Mở nhóm {g.count} nhà</button></div>)}</div>
 {group&&<div style={{borderTop:"1px solid var(--ovg-border,#dbe3ec)",paddingTop:8}}>
 <b>Địa chỉ sẽ xác minh: {group.verified_road} / {group.verified_prefix}</b>
 <p className="hn-muted">Đã chọn {selected.length}/{group.count} nhà · Kết luận: thuộc tuyến đang xét.</p>
 <div className="hn-actions"><button type="button" className="hn-btn secondary" disabled={busy} onClick={()=>{setSelected(group.members.map(m=>m.building_id));setConfirmed(false)}}>Chọn tất cả nhà trong nhóm</button><button type="button" className="hn-btn secondary" disabled={busy} onClick={()=>{setSelected([]);setConfirmed(false)}}>Bỏ chọn</button></div>
 <div style={{maxHeight:250,overflow:"auto"}}>{group.members.map(m=><div key={m.building_id} style={{display:"flex",alignItems:"center",gap:6,padding:4}}>
 <label style={{flex:1}}><input type="checkbox" aria-label={"Chọn nhà "+m.feature_id} checked={selected.includes(m.building_id)} disabled={busy} onChange={e=>toggle(m.building_id,e.target.checked)}/> {m.feature_id} · {m.road||"—"} / {m.alley||"—"} · {m.distance_m} m</label>
 {onGoto&&<button type="button" className="hn-btn secondary" onClick={()=>onGoto(m.source_id,m.feature_id)}>Xem nhà</button>}</div>)}</div>
 <label>Căn cứ chung cho các nhà đã chọn<textarea aria-label="Căn cứ chung xác minh nhóm" value={evidence} disabled={busy} maxLength={500} onChange={e=>{setEvidence(e.target.value);setConfirmed(false)}} placeholder="Số biên bản, tài liệu hoặc kết quả đối chiếu thực địa"/></label>
 <label><input type="checkbox" checked={confirmed} disabled={busy||selected.length<2||evidence.trim().length<10} onChange={e=>setConfirmed(e.target.checked)}/> Tôi đã kiểm tra căn cứ chung áp dụng cho từng nhà được chọn</label>
 <div className="hn-actions"><button type="button" className="hn-btn" disabled={busy||selected.length<2||!confirmed||evidence.trim().length<10} onClick={()=>void save()}>Lưu xác minh {selected.length} nhà</button><button type="button" className="hn-btn secondary" disabled={busy} onClick={()=>{setKey("");setConfirmed(false)}}>Đóng nhóm</button></div>
 </div>}
 </section>
}
