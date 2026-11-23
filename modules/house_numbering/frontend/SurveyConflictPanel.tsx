type Group={kind:string;road:string;alley:string;code:string;count:number;unassigned:boolean;candidate_count:number;examples:{source_id:string;feature_id:string}[]};
export type SurveyReview={counts:Record<string,number>;groups:Group[];total_groups:number};
const kinds:Record<string,string>={descendant:"Nhà khai hẻm con sâu hơn",different_branch:"Sai nhánh cùng đường",different_road:"Tên đường mâu thuẫn",missing_prefix:"Thiếu tiền tố hẻm",missing_road:"Thiếu tên đường",exact:"Khớp địa chỉ"};
const actions:Record<string,string>={descendant:"Tìm hoặc bổ sung tuyến con, giữ đường dẫn đầy đủ.",different_branch:"So sánh nhánh đúng trên bản đồ và khảo sát cửa vào.",different_road:"Xác minh tên đường và mã gốc; không dựa vào khoảng cách riêng lẻ.",missing_prefix:"Xác nhận địa chỉ mặt tiền hay hẻm; không tự điền tiền tố.",missing_road:"Xác minh tên đường thực tế."};
export function SurveyConflictPanel({review,onGoto}:{review:SurveyReview;onGoto?:((source:string,feature:string)=>void)|undefined}){
 if(!review.total_groups)return null;
 return <details style={{marginTop:6,padding:6,border:"1px solid var(--ovg-border,#dbe3ec)",borderRadius:4}}>
  <summary><b>Chi tiết hồ sơ cần đối soát ({Object.entries(review.counts).filter(([k])=>k!=="exact").reduce((sum,[,n])=>sum+n,0)})</b></summary>
  <div style={{display:"flex",flexWrap:"wrap",gap:4,margin:"6px 0"}}>{Object.entries(review.counts).map(([k,n])=><small key={k} style={{padding:"3px 5px",border:"1px solid var(--ovg-border,#dbe3ec)",borderRadius:3}}>{kinds[k]??k}: <b>{n}</b></small>)}</div>
  <p className="hn-muted">Không thay đổi hồ sơ khảo sát. Tuyến có mâu thuẫn vẫn bị chặn import; thiếu trường địa chỉ phải xác minh trước khi gán nhà.</p>
  <div style={{maxHeight:340,minWidth:240,overflow:"auto"}}>{review.groups.map((g,i)=><details key={g.code+":"+g.kind+":"+i} style={{padding:5,borderBottom:"1px solid var(--ovg-border,#dbe3ec)"}}>
    <summary>{g.road||"Chưa có tên"}{g.alley?" / "+g.alley:""} · {g.count} nhà · {kinds[g.kind]??g.kind}</summary>
    <small style={{display:"block",overflowWrap:"anywhere"}}>Mã gốc: {g.code} · {g.unassigned?"Chưa liên kết":"Đã liên kết"}</small>
    <p>{actions[g.kind]??"Xác minh địa chỉ."}</p>
    <small>{g.candidate_count?g.candidate_count+" tuyến danh mục cùng địa chỉ; cần đo khoảng cách và xác minh.":"Chưa có tuyến danh mục khớp địa chỉ."}</small>
    {onGoto&&<div className="hn-actions">{g.examples.map((f,j)=><button key={f.source_id+":"+f.feature_id} type="button" title={f.feature_id} className="hn-btn secondary" onClick={()=>onGoto(f.source_id,f.feature_id)}>Xem nhà {j+1}</button>)}</div>}
    {g.count>g.examples.length&&<small>Hiển thị {g.examples.length}/{g.count} nhà mẫu.</small>}
  </details>)}</div>
  {review.total_groups>review.groups.length&&<small>Hiển thị {review.groups.length}/{review.total_groups} nhóm ưu tiên.</small>}
 </details>
}
