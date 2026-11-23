import {useState} from "react";
import {hnClient,uiError,withCsrf} from "./client";
import {AddressBatchPanel,type BatchGroup} from "./AddressBatchPanel";
type Suggestion={key:string;origin:"staged"|"catalog";name:string;road:string;prefix:string;status:string;source_id:string;feature_id:string;distance_m:number;score:number;signals:string[];can_prefill:boolean;decision:"belongs"|"other_address"};
type Priority="strong"|"partial"|"conflict"|"unmatched"|"blocked";
type Item={suggestions?:{priority:Priority;candidates:Suggestion[]};building_id:string;source_id:string;feature_id:string;road:string;alley:string;code:string;kind:string;distance_m:number|null;eligible:boolean;can_belong:boolean;evidence_token:string;revision:number;status:"verified"|"blocked"|"pending";decision?:"belongs"|"other_address";verified_road?:string;verified_prefix?:string;evidence?:string};
type Report={batch_groups?:BatchGroup[];suggestion_counts?:Record<Priority,number>;name:string;expected_prefix:string;geometry_current:boolean;overflow:boolean;counts:{total:number;verified:number;pending:number;blocked:number};items:Item[];next_offset:number|null};
const priorities:Record<Priority,string>={strong:"Khớp địa chỉ, gần tuyến",partial:"Khớp một phần",conflict:"Có mâu thuẫn",unmatched:"Chưa đủ căn cứ",blocked:"Bị khóa"};
const signals:Record<string,string>={road_match:"Trùng đường",prefix_match:"Trùng toàn bộ hẻm",original_code:"Khớp mã khảo sát",within_35m:"Trong 35 m",within_80m:"Trong 80 m",far:"Xa tuyến",road_conflict:"Khác đường",prefix_conflict:"Khác hẻm",missing_road:"Thiếu đường",missing_prefix:"Thiếu hẻm",draft_route:"Tuyến dự thảo"};
const kinds:Record<string,string>={different_road:"Khác đường",different_branch:"Khác hẻm",descendant:"Hẻm con sâu hơn",missing_road:"Thiếu tên đường",missing_prefix:"Thiếu tiền tố"};
export function AddressVerificationPanel({endpoint,id,promoted,onGoto}:{endpoint:string;id:string;promoted:boolean;onGoto?:((source:string,feature:string)=>void)|undefined}){
 const path=endpoint+"/geometry-reviews/"+id+"/addresses";
 const [report,setReport]=useState<Report|null>(null),[offset,setOffset]=useState(0),[busy,setBusy]=useState(false),[error,setError]=useState(""),[notice,setNotice]=useState("");
 const [filter,setFilter]=useState<"all"|"pending"|Priority>("all");
 const [selected,setSelected]=useState<Item|null>(null),[decision,setDecision]=useState<"belongs"|"other_address">("belongs"),[road,setRoad]=useState(""),[prefix,setPrefix]=useState(""),[evidence,setEvidence]=useState(""),[confirmed,setConfirmed]=useState(false);
 async function load(page=0){setBusy(true);setError("");setSelected(null);try{setReport(await hnClient.request<Report>(path+"?offset="+page+"&limit=100"));setOffset(page)}catch(e){setError(await uiError(e))}finally{setBusy(false)}}
 function select(item:Item){setSelected(item);setDecision(item.decision??(item.can_belong?"belongs":"other_address"));setRoad(item.verified_road??"");setPrefix(item.verified_prefix??"");setEvidence(item.evidence??"");setConfirmed(false);setNotice("");}
 function useSuggestion(s:Suggestion){if(!s.can_prefill||promoted||busy)return;setDecision(s.decision);setRoad(s.road);setPrefix(s.prefix);setEvidence("");setConfirmed(false);setNotice("Đã điền gợi ý vào biểu mẫu. Chưa xác minh, cần kiểm tra bằng chứng thực tế.");}
 async function save(){if(!selected||!confirmed||busy||promoted)return;setBusy(true);setError("");setNotice("");try{await withCsrf(()=>hnClient.request(path+"/"+selected.building_id,{method:"PUT",body:JSON.stringify({evidence_token:selected.evidence_token,expected_revision:selected.revision,decision,verified_road:road.trim(),verified_prefix:prefix.trim(),evidence:evidence.trim()})}));setNotice("Đã lưu bằng chứng xác minh, giữ nguyên hồ sơ gốc.");await load(offset)}catch(e){setError(await uiError(e));setBusy(false)}}
 return <section style={{borderTop:"1px solid var(--ovg-border,#dbe3ec)",marginTop:8,paddingTop:8}}>
 <button type="button" className="hn-btn secondary" disabled={busy} onClick={()=>void load(offset)}>Mở hồ sơ xác minh từng nhà</button>
 {error&&<p role="alert" className="hn-error">{error}</p>}{notice&&<p role="status">{notice}</p>}
 {report&&<><p><b>Đã xác minh {report.counts.verified}/{report.counts.total}</b> · còn {report.counts.pending} · bị khóa {report.counts.blocked}</p>
 {!report.geometry_current?<p role="alert">Hình học hoặc tuyến cha đã thay đổi. Cần xem trước và lưu lại hình học.</p>:report.overflow?<p role="alert">Vượt giới hạn xử lý, cần rà soát dữ liệu nguồn.</p>:<>
 {report.counts.pending===0&&<p>{promoted?"Đã vào danh mục, hồ sơ chỉ đọc.":"Đã đủ xác minh địa chỉ. Xem trước lại cây tuyến, xác nhận đối soát và import tuyến theo quy trình chuẩn."}</p>}
 <p className="hn-muted">Chỉ xác minh bằng hồ sơ hoặc thực địa. Địa chỉ khác tuyến không được gán tự động. Tuyến dự kiến: {report.name} (prefix {report.expected_prefix}).</p>
 {report.suggestion_counts&&<><p className="hn-muted">Đề xuất trong lô: {report.suggestion_counts.strong} khớp địa chỉ · {report.suggestion_counts.partial} khớp một phần · {report.suggestion_counts.conflict} mâu thuẫn · {report.suggestion_counts.unmatched} chưa đủ căn cứ · {report.suggestion_counts.blocked} bị khóa</p><label>Lọc hồ sơ<select aria-label="Lọc hồ sơ đề xuất" value={filter} onChange={e=>setFilter(e.target.value as typeof filter)}><option value="all">Tất cả</option><option value="pending">Chưa xác minh</option>{Object.entries(priorities).map(([k,v])=><option key={k} value={k}>{v}</option>)}</select></label></>}
 {!promoted&&report.batch_groups&&<AddressBatchPanel path={path} groups={report.batch_groups} busy={busy} onBusy={setBusy} onSaved={async count=>{setNotice("Đã lưu xác minh "+count+" nhà theo nhóm. Dữ liệu khảo sát gốc không đổi.");await load(offset)}} onGoto={onGoto}/>}
 <div style={{maxHeight:220,overflow:"auto"}}>{report.items.filter(item=>filter==="all"||(filter==="pending"?item.status==="pending":item.suggestions?.priority===filter)).map(item=><div key={item.building_id} style={{padding:6,borderBottom:"1px solid var(--ovg-border,#dbe3ec)"}}>
 <div><b>{kinds[item.kind]??item.kind}</b> · {item.road||"—"} / {item.alley||"—"} · {item.distance_m??"—"} m · {item.status==="verified"?"Đã xác minh":item.status==="blocked"?"Bị khóa":"Chờ xác minh"}</div>
 {item.suggestions&&<small style={{display:"block"}}>Đề xuất: {priorities[item.suggestions.priority]} · {item.suggestions.candidates.length} phương án</small>}
 <small>Mã nguồn: {item.code} · Đối tượng {item.feature_id}</small> {onGoto&&<button type="button" className="hn-btn secondary" onClick={()=>onGoto(item.source_id,item.feature_id)}>Xem nhà trên bản đồ</button>} <button type="button" className="hn-btn secondary" disabled={busy||(!item.eligible&&item.status!=="verified")} onClick={()=>select(item)}>{promoted?"Xem bằng chứng":"Xác minh nhà"}</button>
 </div>)}</div>
 <div className="hn-actions"><button type="button" className="hn-btn secondary" disabled={busy||offset===0} onClick={()=>void load(Math.max(0,offset-100))}>Nhà lô trước</button><button type="button" className="hn-btn secondary" disabled={busy||report.next_offset===null} onClick={()=>void load(report.next_offset??0)}>Nhà lô sau</button></div>
 {selected&&<div style={{padding:8,border:"1px solid var(--ovg-border,#dbe3ec)"}}>
 <h4>Hồ sơ {selected.feature_id} · {kinds[selected.kind]}</h4>
 {selected.suggestions&&<section style={{background:"var(--ovg-bg,#f5f7fa)",padding:8,borderRadius:6,marginBottom:8}}><b>Địa chỉ đề xuất · {priorities[selected.suggestions.priority]}</b><p className="hn-muted">Xếp hạng tham khảo từ dữ liệu nguồn và khoảng cách; không phải kết luận. Thiếu tiền tố không đồng nghĩa mặt tiền.</p>
 {!selected.suggestions.candidates.length&&<p>Không có tuyến đủ dữ liệu để đối chiếu. Kiểm tra hồ sơ gốc.</p>}
 {selected.suggestions.candidates.map(s=><div key={s.key} style={{borderTop:"1px solid var(--ovg-border,#dbe3ec)",padding:"6px 0"}}><b>{s.road}{s.prefix?" / "+s.prefix:" (mặt tiền)"}</b> · {s.distance_m} m · {s.origin==="staged"?"Kho chờ":s.status==="confirmed"?"Danh mục xác nhận":"Danh mục dự thảo"}<small style={{display:"block"}}>{s.signals.map(x=>signals[x]??x).join(" · ")}</small><div className="hn-actions">{onGoto&&<button type="button" className="hn-btn secondary" onClick={()=>onGoto(s.source_id,s.feature_id)}>Xem tuyến</button>}{!promoted&&<button type="button" className="hn-btn secondary" disabled={busy||!s.can_prefill} onClick={()=>useSuggestion(s)}>Điền theo đề xuất</button>}</div></div>)}
 </section>}
 <label>Kết luận địa chỉ<select aria-label="Kết luận địa chỉ" value={decision} disabled={busy||promoted} onChange={e=>{setDecision(e.target.value as typeof decision);setConfirmed(false)}}><option value="belongs" disabled={!selected.can_belong}>Thuộc đúng tuyến đang xét</option><option value="other_address">Thuộc địa chỉ khác</option></select></label>
 <label>Tên đường đã xác minh<input aria-label="Tên đường đã xác minh" value={road} maxLength={200} disabled={busy||promoted} onChange={e=>{setRoad(e.target.value);setConfirmed(false)}}/></label>
 <label>Tiền tố hẻm đầy đủ<input aria-label="Tiền tố hẻm đầy đủ" value={prefix} maxLength={100} disabled={busy||promoted} onChange={e=>{setPrefix(e.target.value);setConfirmed(false)}} placeholder="VD 91/14; để trống nếu mặt tiền"/></label>
 <label>Căn cứ xác minh<textarea aria-label="Căn cứ xác minh" value={evidence} maxLength={500} disabled={busy||promoted} onChange={e=>{setEvidence(e.target.value);setConfirmed(false)}}/></label>
 {!promoted&&<><label><input type="checkbox" checked={confirmed} disabled={busy} onChange={e=>setConfirmed(e.target.checked)}/> Tôi đã kiểm tra căn cứ địa chỉ này</label><button type="button" className="hn-btn" disabled={busy||!confirmed||!road.trim()||evidence.trim().length<10||(decision==="belongs"&&(!selected.can_belong||!prefix.trim()))} onClick={()=>void save()}>Lưu xác minh nhà</button></>}
 <button type="button" className="hn-btn secondary" onClick={()=>setSelected(null)}>Đóng hồ sơ</button>
 </div>}
 </>}
 </>}
 </section>
}
