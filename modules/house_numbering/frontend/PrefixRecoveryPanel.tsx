import {useState} from "react";
import {hnClient,uiError,withCsrf} from "./client";

type Ref={source_id:string;feature_id:string;name:string;route_id?:string;code?:string;reason?:string};
type Level={prefix:string;existing_count:number;source_count:number;existing:Ref[];sources:Ref[]};
type Group={id:string;road:string;alley:string;count:number;near:number;status:string;codes:{code:string;count:number}[];old_routes:(Ref&{prefix:string|null})[];chain:Level[];sources:Ref[];junction_m:number|null;geometry_connected:boolean;parent_candidates:(Ref&{distance_m:number|null;connected:boolean})[]};
type Report={counts:Record<string,number>;total_groups:number;filtered_groups:number;items:Group[];next_offset:number|null};
const states:Record<string,string>={connected_review:"Có nguồn nối đúng cha",root_review:"Có nguồn đường gốc",parent_missing:"Thiếu cấp cha",ambiguous_parent:"Trùng cấp cha",ambiguous_geometry:"Nhiều đoạn nguồn",missing_geometry:"Thiếu hình học tuyến",invalid_prefix:"Tiền tố không hợp lệ",existing_path_conflict:"Địa chỉ đã có tuyến – đối soát mã",disconnected:"Nguồn không nối cha",distance:"Nhà cách nguồn trên 35 m"};
export function PrefixRecoveryPanel({endpoint,mapping,source,onGoto,onBusy,disabled}:{endpoint:string;mapping:Record<string,string>;source:string;onGoto?:((source:string,feature:string)=>void)|undefined;onBusy:(busy:boolean)=>void;disabled:boolean}){
 const [report,setReport]=useState<Report|null>(null),[search,setSearch]=useState(""),[offset,setOffset]=useState(0),[busy,setBusy]=useState(false),[error,setError]=useState(""),[filter,setFilter]=useState("all");
 async function load(page=0){
  setBusy(true);onBusy(true);setError("");setReport(null);
  try{const r=await withCsrf(()=>hnClient.request<Report>(endpoint+"/prefix-recovery/preview",{method:"POST",body:JSON.stringify({source_id:source||null,mapping,search,offset:page,limit:100})}));setReport(r);setOffset(page);setFilter("all");}
  catch(e){setError(await uiError(e));}finally{setBusy(false);onBusy(false)}
 }
 function link(ref:Ref,label="Xem"){return onGoto&&<button type="button" className="hn-btn secondary" onClick={()=>onGoto(ref.source_id,ref.feature_id)}>{label}</button>}
 return <section className="hn-card" style={{marginTop:12}}>
  <h3>Phục hồi cây tuyến theo tiền tố nhà</h3>
  <p className="hn-muted">Tách từng đường dẫn từ địa chỉ khảo sát, tìm đúng cấp cha và tuyến GIS nguồn. Chỉ ưu tiên nguồn đơn hoặc nhóm đoạn ghép thành một đường, có đúng một cha nối đầu/cuối trong 8 m và nhà nằm trong 35 m. Không tự vẽ hoặc sửa mã.</p>
  <div className="hn-actions"><button type="button" className="hn-btn" disabled={busy||disabled} onClick={()=>void load()}>Rà soát cây tuyến thiếu</button><label>Tìm đường, hẻm, mã<input aria-label="Tìm cây tuyến thiếu" value={search} disabled={busy||disabled} onChange={e=>setSearch(e.target.value)}/></label></div>
  {error&&<p role="alert" className="hn-error">{error}</p>}
  {report&&<>
   <p><b>{report.counts.total?.toLocaleString("vi-VN")??0} nhà</b> · {report.total_groups} đường dẫn thiếu hoặc sai cấp</p>
   <p className="hn-muted">Chỉ đọc. Chọn các nguồn đã xác minh trong mục Import cây tuyến, nhập đủ cha trước con; sau đó chạy đối soát/gán lại mã nhà.</p>
   <div className="hn-actions">{Object.entries(report.counts).filter(([k])=>k!=="total").map(([k,n])=><span key={k} style={{padding:6,border:"1px solid var(--ovg-border,#dbe3ec)",borderRadius:4}}>{states[k]??k}: <b>{n.toLocaleString("vi-VN")}</b></span>)}</div>
   <label>Lọc trong lô hiện tại<select value={filter} onChange={e=>setFilter(e.target.value)}><option value="all">Tất cả</option>{Object.entries(states).map(([k,v])=><option key={k} value={k}>{v}</option>)}</select></label>
   <div style={{maxHeight:520,overflow:"auto"}}>
    {report.items.filter(g=>filter==="all"||g.status===filter).map(g=><details key={g.id} style={{borderBottom:"1px solid var(--ovg-border,#dbe3ec)",padding:8}}>
     <summary><b>{g.road}{g.alley?" / "+g.alley:""}</b> · {g.count} nhà · {states[g.status]??g.status}{g.near>0?" · "+g.near+" nhà ≤35m":""}</summary>
     <div style={{padding:8}}>
     <b>Đường dẫn cần phục hồi (cha → con)</b>
     {g.chain.map((c,i)=><div key={c.prefix} style={{padding:6,marginLeft:Math.min(i,5)*12,borderLeft:"2px solid var(--ovg-border,#dbe3ec)"}}>
      <b>{c.prefix||g.road}</b> · {c.existing_count?c.existing_count+" tuyến đã nhập":c.source_count?c.source_count+" nguồn chờ nhập":"Thiếu hình học"}
      {c.existing.map(r=><div key={r.route_id}>{r.name} {link(r,"Xem tuyến đã nhập")}</div>)}
      {c.sources.map(r=><div key={r.source_id+":"+r.feature_id}>{r.name} · {r.code||r.feature_id} {link(r,"Xem nguồn GIS")}</div>)}
     </div>)}
     {g.sources.length>0&&<p>Bằng chứng: {g.near}/{g.count} nhà ≤35 m; {g.geometry_connected?"hình học liền mạch":"đoạn rời hoặc phân nhánh"}; {g.junction_m===null?"chưa có cha xác định":"đầu/cuối cách cha "+g.junction_m+" m"}</p>}
     {g.parent_candidates.length>0&&<><b>Đoạn cha theo điểm nối</b>{g.parent_candidates.map(r=><div key={r.route_id}>{r.name} · {r.distance_m??"—"} m · {r.connected?"Nối đúng ngưỡng":"Không nối"} {link(r,"Xem cha")}</div>)}</>}
     <b>Mã tuyến khảo sát đang trỏ sai</b>{g.codes.map(c=><small key={c.code} style={{display:"block"}}>{c.code} · {c.count} nhà</small>)}
     {g.old_routes.map(r=><div key={r.route_id}>{r.name}{r.prefix?" / "+r.prefix:""} {link(r,"Xem tuyến mã cũ")}</div>)}
     </div>
    </details>)}
   </div>
   <div className="hn-actions"><button type="button" className="hn-btn secondary" disabled={busy||disabled||offset===0} onClick={()=>void load(Math.max(0,offset-100))}>Lô trước</button><button type="button" className="hn-btn secondary" disabled={busy||disabled||report.next_offset===null} onClick={()=>void load(report.next_offset??0)}>Lô sau</button><small>{report.filtered_groups} nhóm phù hợp tìm kiếm</small></div>
  </>}
 </section>
}
