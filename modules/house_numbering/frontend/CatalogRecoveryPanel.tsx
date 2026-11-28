import {uiConfirm} from "@openvietgis/spatial-workspace/plugin/ui-dialog";
import {useState} from "react";
import {hnClient,uiError,withCsrf} from "./client";
type Alias={id:string;token:string;route_id:string;route_name:string;root_name:string;prefix:string|null;road:string;alley:string;code:string;count:number;near:number;observations:number;matching_observations:number;reviewable:boolean;source_name:string|null;planned_name:string|null;source_id:string;feature_id:string};
type SourceMatch={road:string;alley:string;count:number;sources:{source_id:string;feature_id:string;name:string;code:string;reason:string}[]};
type Report={counts:Record<string,number>;total_groups:number;filtered_groups:number;items:Alias[];next_offset:number|null;source_matches:SourceMatch[]};
export function CatalogRecoveryPanel({endpoint,mapping,source,onGoto,onBusy,onDone,disabled}:{endpoint:string;mapping:Record<string,string>;source:string;onGoto?:((source:string,feature:string)=>void)|undefined;onBusy:(busy:boolean)=>void;onDone:(msg:string)=>void;disabled:boolean}){
 const [report,setReport]=useState<Report|null>(null),[search,setSearch]=useState(""),[offset,setOffset]=useState(0),[selected,setSelected]=useState<Record<string,string>>({}),[reason,setReason]=useState(""),[verified,setVerified]=useState(false),[busy,setBusy]=useState(false),[error,setError]=useState("");
 const body={source_id:source||null,mapping};
 const count=Object.keys(selected).length;
 async function load(page=0){
  setBusy(true);onBusy(true);setError("");setSelected({});setVerified(false);setReport(null);
  try{const p=await withCsrf(()=>hnClient.request<Report>(endpoint+"/catalog-recovery/preview",{method:"POST",body:JSON.stringify({...body,search,offset:page,limit:100})}));setReport(p);setOffset(page);}
  catch(e){setError(await uiError(e));}finally{setBusy(false);onBusy(false)}
 }
 async function apply(){
  if(!report||!count||!verified||reason.trim().length<10||!(await uiConfirm(`Duyệt ${count} tên đối chiếu theo mã nguồn? Không đổi tên chính thức hoặc tự gán nhà.`)))return;
  setBusy(true);onBusy(true);setError("");
  try{const result=await withCsrf(()=>hnClient.request<{aliases:number;routes:number;potential_houses:number}>(endpoint+"/catalog-recovery/apply",{method:"POST",body:JSON.stringify({...body,selections:selected,reason:reason.trim()})}));
    setReport(null);setSelected({});setVerified(false);onDone(`Đã duyệt ${result.aliases} tên đối chiếu cho ${result.routes} tuyến. ${result.potential_houses} nhà có thể xem trước gán tuyến. Chưa ghi số nhà.`);
  }catch(e){setError(await uiError(e));}finally{setBusy(false);onBusy(false)}
 }
 function choose(row:Alias,yes:boolean){
  setSelected(old=>{const next={...old};if(yes&&Object.keys(next).length<100)next[row.id]=row.token;else if(!yes)delete next[row.id];return next});setVerified(false);
 }
 return <section className="hn-card" style={{marginTop:12}}>
  <h3>Phục hồi danh mục địa chỉ</h3>
  <p className="hn-muted">Tìm nhà có mã tuyến thật nhưng tên đường khảo sát khác tên danh mục. Duyệt tên đối chiếu chỉ áp dụng cho đúng mã và đúng tiền tố; không đổi tên chính thức, không bổ sung hình học giả.</p>
  <div className="hn-actions"><button className="hn-btn" type="button" disabled={busy||disabled} onClick={()=>void load()}>Rà soát địa chỉ thiếu tuyến</button><label>Tìm tên / mã tuyến<input aria-label="Tìm tên hoặc mã tuyến" disabled={busy||disabled} value={search} onChange={e=>setSearch(e.target.value)} /></label></div>
  {error&&<p role="alert" className="hn-error">{error}</p>}
  {report&&<>
    <p><b>{(report.counts.total??0).toLocaleString("vi-VN")} nhà</b> thiếu đường/hẻm khớp danh mục · {(report.counts.same_prefix??0).toLocaleString("vi-VN")} cùng tiền tố · {(report.counts.near??0).toLocaleString("vi-VN")} nhà trong 35 m · {(report.counts.prefix_mismatch??0).toLocaleString("vi-VN")} sai tiền tố cần phục hồi cây tuyến.</p>
    <p>{report.filtered_groups} / {report.total_groups} cụm tên đối chiếu. Mỗi dòng là một tên khảo sát trên đúng một mã tuyến. Không tự chọn theo số lượng.</p>
    <div style={{overflow:"auto",maxHeight:430}}><table className="hn-table"><thead><tr><th>Duyệt</th><th>Tên tuyến chính thức</th><th>Tên khảo sát / mã nguồn</th><th>Bằng chứng</th><th>Bản đồ</th></tr></thead><tbody>{report.items.map(g=><tr key={g.id}>
     <td><input type="checkbox" aria-label={"Duyệt tên "+g.road+" "+g.code} disabled={busy||disabled||!g.reviewable} checked={!!selected[g.id]} onChange={e=>choose(g,e.target.checked)}/></td>
     <td><b>{g.root_name}{g.prefix?" / "+g.prefix:""}</b><small style={{display:"block"}}>Nguồn: {g.source_name||"—"}{g.planned_name?" · Quy hoạch: "+g.planned_name:""}</small></td>
     <td>{g.road}{g.alley?" / "+g.alley:""}<small style={{display:"block"}}>{g.code}</small></td>
     <td><b>{g.near}/{g.count}</b> nhà ≤35m<span style={{display:'block'}}/><small>{g.matching_observations}/{g.observations} bản khảo sát cùng mã có cùng tên/hẻm</small></td>
     <td>{onGoto&&<button className="hn-btn secondary" type="button" onClick={()=>onGoto(g.source_id,g.feature_id)}>Xem tuyến nguồn</button>}</td>
    </tr>)}</tbody></table></div>
    <div className="hn-actions"><button className="hn-btn secondary" type="button" disabled={busy||disabled||offset===0} onClick={()=>void load(Math.max(0,offset-100))}>Lô trước</button><button className="hn-btn secondary" type="button" disabled={busy||disabled||report.next_offset===null} onClick={()=>void load(report.next_offset??0)}>Lô sau</button><span>Đã chọn {count}/100 tên</span></div>
    {count>0&&<><label>Lý do xác minh tên đối chiếu<textarea aria-label="Lý do xác minh tên đối chiếu" disabled={busy||disabled} maxLength={500} value={reason} onChange={e=>{setReason(e.target.value);setVerified(false)}}/></label><label><input type="checkbox" disabled={busy||disabled} checked={verified} onChange={e=>setVerified(e.target.checked)}/> Đã kiểm tra mã, tiền tố và tuyến thực tế; tên đối chiếu đúng.</label><button className="hn-btn" type="button" disabled={busy||disabled||!verified||reason.trim().length<10} onClick={()=>void apply()}>Lưu {count} tên đối chiếu</button></>}
    <h4>Tuyến nguồn chưa nhập khớp địa chỉ</h4><p>Phát hiện {(report.counts.source_path_houses??0).toLocaleString("vi-VN")} nhà có địa chỉ khớp tuyến nguồn chưa vào danh mục (có thể trùng nhóm sai tiền tố). Dùng phần Import / phục hồi cây tuyến để kiểm tra quan hệ cha–con.</p>
    {!!report.source_matches.length&&<div style={{overflow:"auto",maxHeight:260}}><table className="hn-table"><thead><tr><th>Địa chỉ thiếu</th><th>Số nhà</th><th>Tuyến nguồn</th></tr></thead><tbody>{report.source_matches.map(g=><tr key={g.road+":"+g.alley}><td>{g.road} {g.alley?"/ "+g.alley:""}</td><td>{g.count}</td><td>{g.sources.map(s=><div key={s.source_id+":"+s.feature_id}>{s.name} · {s.reason||"Chưa nhập"}{onGoto&&<button className="hn-btn secondary" type="button" onClick={()=>onGoto(s.source_id,s.feature_id)}>Xem tuyến</button>}</div>)}</td></tr>)}</tbody></table></div>}
  </>}
 </section>
}
