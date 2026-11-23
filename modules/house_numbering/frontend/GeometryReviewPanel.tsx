import {useEffect,useState} from "react";
import {AddressVerificationPanel} from "./AddressVerificationPanel";
import {hnClient,uiError,withCsrf} from "./client";
import {SurveyConflictPanel,type SurveyReview} from "./SurveyConflictPanel";
type Candidate={key:string;name:string;geometry_stage_eligible?:boolean;survey_evidence?:{referenced:number;unassigned:number;address_conflicts:number}};
type Ref={source_id:string;feature_id:string;code?:string};
type Saved={id:string;route_code:string;reason:string;status:"pending"|"promoted";revision:number;updated_at:string;snapshot:{name:string;resolved_prefix:string;source_id:string;feature_id:string;segments?:Ref[];parent_evidence?:{name:string;distance_m:number};survey_evidence?:{referenced:number;address_conflicts:number};survey_review?:SurveyReview}};
type Listing={items:Saved[];next_offset:number|null};
export function GeometryReviewPanel({endpoint,previewToken,candidates,request,disabled,onBusy,onGoto}:{endpoint:string;previewToken:string|null;candidates:Candidate[];request:()=>object;disabled:boolean;onBusy:(busy:boolean)=>void;onGoto?:((source:string,feature:string)=>void)|undefined}){
 const [selected,setSelected]=useState<string[]>([]),[reason,setReason]=useState(""),[busy,setBusy]=useState(false),[notice,setNotice]=useState(""),[error,setError]=useState(""),[listing,setListing]=useState<Listing|null>(null),[offset,setOffset]=useState(0);
 const ready=candidates.filter(r=>r.geometry_stage_eligible);
 useEffect(()=>{setSelected([]);setReason("");},[previewToken]);
 async function list(page=0){
  setBusy(true);onBusy(true);setError("");
  try{setListing(await hnClient.request<Listing>(endpoint+"/geometry-reviews?offset="+page+"&limit=100"));setOffset(page)}
  catch(e){setError(await uiError(e));}finally{setBusy(false);onBusy(false)}
 }
 async function stage(){
  if(!previewToken||disabled||!selected.length||reason.trim().length<10||!window.confirm("Chỉ lưu hình học chờ xác minh, không đưa vào danh mục cấp số. Tiếp tục?"))return;
  setBusy(true);onBusy(true);setError("");setNotice("");
  try{const r=await withCsrf(()=>hnClient.request<{staged:number;routes_created:number;buildings_assigned:number}>(endpoint+"/route-import/stage-geometry",{method:"POST",body:JSON.stringify({...request(),preview_token:previewToken,selected_keys:selected,reason:reason.trim()})}));setNotice("Đã lưu "+r.staged+" tuyến hình học chờ xác minh. Không tạo tuyến đánh số hoặc gán nhà.");setSelected([]);setReason("");setListing(await hnClient.request<Listing>(endpoint+"/geometry-reviews?offset=0&limit=100"));setOffset(0)}
  catch(e){setError(await uiError(e));}finally{setBusy(false);onBusy(false)}
 }
 return <section style={{border:"1px solid var(--ovg-border,#dbe3ec)",borderRadius:8,padding:10,margin:"12px 0"}}>
  <h3>Kho tuyến hình học chờ xác minh</h3>
  <p className="hn-muted">Lưu tuyến GIS đã nối đúng cha nhưng còn hồ sơ địa chỉ mâu thuẫn. Tuyến ở kho này không dùng để gán nhà, tạo số hoặc làm cha trong cây chính. Khi xử lý xong mâu thuẫn, import lại bằng quy trình chuẩn.</p>
  <button type="button" className="hn-btn secondary" disabled={busy||disabled} onClick={()=>void list()}>Mở kho tuyến chờ xác minh</button>
  {error&&<p role="alert" className="hn-error">{error}</p>}{notice&&<p role="status" style={{color:"#15803d"}}>{notice}</p>}
  {previewToken&&ready.length>0&&<div style={{marginTop:8}}>
   <b>{ready.length} tuyến đủ điều kiện lưu hình học</b>
   <p className="hn-muted">Giữ nguyên các lỗi khảo sát; không cần đánh dấu đã xác minh địa chỉ.</p>
   <div style={{maxHeight:230,overflow:"auto"}}>{ready.map(r=><label key={r.key} style={{display:"block",padding:5}}><input type="checkbox" aria-label={"Lưu hình học "+r.name} disabled={busy||disabled} checked={selected.includes(r.key)} onChange={e=>setSelected(old=>e.target.checked?(old.length<100?[...old,r.key]:old):old.filter(x=>x!==r.key))}/> {r.name} · {r.survey_evidence?.address_conflicts??0} hồ sơ mâu thuẫn</label>)}</div>
   {selected.length>0&&<><label>Lý do lưu hình học<textarea aria-label="Lý do lưu hình học" value={reason} onChange={e=>setReason(e.target.value)} maxLength={500} disabled={busy||disabled}/></label><button type="button" className="hn-btn" disabled={busy||disabled||reason.trim().length<10} onClick={()=>void stage()}>Lưu {selected.length} tuyến chờ xác minh</button></>}
  </div>}
  {listing&&<div style={{marginTop:10,maxHeight:380,overflow:"auto"}}>
   {!listing.items.length&&<p>Chưa có tuyến hình học chờ xác minh.</p>}
   {listing.items.map(r=><details key={r.id} style={{padding:8,borderBottom:"1px solid var(--ovg-border,#dbe3ec)"}}>
    <summary><b>{r.snapshot.name}</b> · {r.status==="promoted"?"Đã vào danh mục":"Chờ xác minh"} · {r.snapshot.survey_evidence?.address_conflicts??0} hồ sơ mâu thuẫn lúc lưu</summary>
    <p>Cha: {r.snapshot.parent_evidence?.name??"—"} · Điểm nối: {r.snapshot.parent_evidence?.distance_m??"—"} m · Rev {r.revision}</p>
    <p>Lý do: {r.reason}</p><small>Bằng chứng lúc lưu; cần xem trước lại để kiểm tra dữ liệu hiện tại.</small>
    {onGoto&&(r.snapshot.segments?.length?r.snapshot.segments:[r.snapshot]).map((s,i)=><button key={s.source_id+":"+s.feature_id} type="button" className="hn-btn secondary" onClick={()=>onGoto(s.source_id,s.feature_id)}>Xem đoạn {i+1}</button>)}
    {r.snapshot.survey_review&&<SurveyConflictPanel review={r.snapshot.survey_review} onGoto={onGoto}/>}
    <AddressVerificationPanel endpoint={endpoint} id={r.id} promoted={r.status==="promoted"} onGoto={onGoto}/>
   </details>)}
   <div className="hn-actions"><button type="button" className="hn-btn secondary" disabled={busy||disabled||offset===0} onClick={()=>void list(Math.max(0,offset-100))}>Lô trước</button><button type="button" className="hn-btn secondary" disabled={busy||disabled||listing.next_offset===null} onClick={()=>void list(listing.next_offset??0)}>Lô sau</button></div>
  </div>}
 </section>
}
