import {useState} from "react";

export type LinkRef={source_id:string;feature_id:string};
export type MergeOption={id:string;name:string;root:string;prefix:string;status:string;members:(LinkRef&{key:string;code:string;name:string;parent_code:string;prefix:string})[]};
export type MissingParent={id:string;root:string;prefix:string;name:string;parent_prefix:string;children:number;buildings:number;examples:(LinkRef&{name:string})[]};
export type ParentDraft=LinkRef&{task_id:string;name:string};
type Source={id:string;name:string};
const titles:Record<string,string>={connected:"Nối liền, có thể ghép",disconnected:"Rời đoạn / phân nhánh",existing_conflict:"Địa chỉ đã có tuyến",attribute_conflict:"Thuộc tính mâu thuẫn",too_many_segments:"Vượt giới hạn 200 đoạn"};

export function TopologyRecoveryPanel({options,missing,mergeGroups,parents,sources,handoffSource,handoffFeature,busy,stale,onMerge,onParent,onRemoveParent,onGoto}:{options:MergeOption[];missing:MissingParent[];mergeGroups:string[];parents:ParentDraft[];sources:Source[];handoffSource:string;handoffFeature:string;busy:boolean;stale:boolean;onMerge:(g:MergeOption,checked:boolean)=>void;onParent:(p:ParentDraft)=>void;onRemoveParent:(p:ParentDraft)=>void;onGoto?:((source:string,feature:string)=>void)|undefined}){
 const [mode,setMode]=useState<"merge"|"parents">("merge");
 const [status,setStatus]=useState("all"),[search,setSearch]=useState("");
 const [taskId,setTaskId]=useState(""),[source,setSource]=useState(""),[feature,setFeature]=useState("");
 const selectedTask=missing.find(t=>t.id===taskId),connected=options.filter(g=>g.status==="connected");
 const matches=(s:string)=>s.toLocaleLowerCase("vi").includes(search.toLocaleLowerCase("vi"));
 return <section style={{border:"1px solid var(--ovg-border,#dbe3ec)",borderRadius:8,padding:12,marginTop:12}}>
  <h3 style={{marginTop:0}}>Phục hồi cấu trúc tuyến</h3>
  <div className="hn-actions"><button className={"hn-btn "+(mode==="merge"?"":"secondary")} onClick={()=>{setMode("merge");setSearch("")}}>Ghép đoạn ({connected.length}/{options.length})</button><button className={"hn-btn "+(mode==="parents"?"":"secondary")} onClick={()=>{setMode("parents");setSearch("")}}>Tuyến cha thiếu ({missing.length})</button></div>
  <p className="hn-muted">{mode==="merge"?"Ghép logic, giữ nguyên từng đường GIS và mã gốc. Chỉ nhóm cùng đường dẫn, nối thành một đường không phân nhánh mới được chọn; sau đó xác nhận cha và dữ liệu khảo sát.":"Danh sách cấp địa chỉ chưa có trong lớp đã chọn hoặc cây đã lưu. Hãy khảo sát, tìm hoặc vẽ tuyến thực trên bản đồ GIS rồi cung cấp ID đối tượng; không tự sinh đường."}</p>
  <div className="hn-form"><label>Tìm đường / hẻm<input value={search} onChange={e=>setSearch(e.target.value)}/></label>{mode==="merge"&&<label>Trạng thái hình học<select value={status} onChange={e=>setStatus(e.target.value)}><option value="all">Tất cả</option>{Object.entries(titles).map(([k,v])=><option value={k} key={k}>{v}</option>)}</select></label>}</div>
  {mode==="merge"?<>
    <div style={{maxHeight:320,overflow:"auto"}}>
     {options.filter(g=>(status==="all"||g.status===status)&&matches(g.name+" "+g.prefix)).map(g=><details key={g.id} style={{padding:"8px 0",borderBottom:"1px solid var(--ovg-border,#dbe3ec)"}}>
      <summary><b>{g.name}</b> · {g.members.length} đoạn · <span style={{color:g.status==="connected"?"#15803d":"#b91c1c"}}>{titles[g.status]??g.status}</span></summary>
      <div style={{padding:8}}>{g.members.map(m=><div key={m.key} style={{display:"flex",gap:8,alignItems:"center",marginBottom:5}}><span style={{overflowWrap:"anywhere"}}>{m.code} · cha gốc {m.parent_code||"—"}</span>{onGoto&&<button className="hn-btn secondary" onClick={()=>onGoto(m.source_id,m.feature_id)}>Xem đoạn</button>}</div>)}
      {g.status==="connected"&&<label style={{display:"flex",gap:6,alignItems:"center"}}><input type="checkbox" checked={mergeGroups.includes(g.id)} disabled={busy} onChange={e=>onMerge(g,e.target.checked)}/> Ghép {g.members.length} đoạn vào cùng tuyến (chờ xem trước)</label>}</div>
     </details>)}
     {!options.length&&<p className="hn-muted">Không có nhóm trùng đường dẫn trong lớp này.</p>}
    </div>
    <small>{mergeGroups.length} nhóm đã chọn. {stale?"Bấm Xem trước cây tuyến để kiểm tra lại.":"Mở nhóm để xem từng đoạn."}</small>
  </>:<>
    <div style={{maxHeight:320,overflow:"auto"}}>
     {missing.filter(t=>matches(t.name+" "+t.prefix)).map(t=><div key={t.id} style={{padding:8,borderBottom:"1px solid var(--ovg-border,#dbe3ec)"}}>
      <b>{t.name}</b> · {t.children} tuyến con liên quan · {t.buildings} nhà theo mã gốc
      <div className="hn-muted">Cấp cha: {t.parent_prefix||"đường chính"}{!t.prefix?" (tạo đường gốc)":""}</div>
      <div className="hn-actions">{onGoto&&t.examples[0]&&<button className="hn-btn secondary" onClick={()=>onGoto(t.examples[0]!.source_id,t.examples[0]!.feature_id)}>Xem tuyến con</button>}<button className="hn-btn secondary" disabled={busy||parents.some(p=>p.task_id===t.id)} onClick={()=>{setTaskId(t.id);setSource("");setFeature("")}}>Liên kết cha đã vẽ</button></div>
     </div>)}
     {!missing.length&&<p className="hn-muted">Không phát hiện cấp cha còn thiếu trong dữ liệu đang xem.</p>}
    </div>
    {selectedTask&&<div style={{border:"1px solid var(--ovg-border,#dbe3ec)",padding:10,marginTop:8,borderRadius:6}}>
     <b>Bổ sung {selectedTask.name}</b>
     <div className="hn-form"><label>Lớp đường GIS<select value={source} onChange={e=>{setSource(e.target.value);setFeature("")}}><option value="">Chọn lớp</option>{sources.map(s=><option key={s.id} value={s.id}>{s.name}</option>)}</select></label><label>ID đối tượng tuyến cha<input value={feature} onChange={e=>setFeature(e.target.value)} placeholder="Feature ID"/></label></div>
     <div className="hn-actions">{handoffSource&&handoffFeature&&sources.some(s=>s.id===handoffSource)&&<button className="hn-btn secondary" onClick={()=>{setSource(handoffSource);setFeature(handoffFeature)}}>Dùng đối tượng đang chọn</button>}<button className="hn-btn" disabled={busy||!source||!feature.trim()} onClick={()=>{onParent({task_id:selectedTask.id,name:selectedTask.name,source_id:source,feature_id:feature.trim()});setTaskId("")}}>Đưa vào xem trước</button><button className="hn-btn secondary" onClick={()=>setTaskId("")}>Hủy</button></div>
    </div>}
    {parents.length>0&&<div style={{marginTop:8}}><b>Tuyến cha đã chỉ định ({parents.length})</b>{parents.map(p=><div className="hn-actions" key={p.task_id}><span>{p.name}: {p.feature_id}</span>{onGoto&&<button className="hn-btn secondary" onClick={()=>onGoto(p.source_id,p.feature_id)}>Xem</button>}<button className="hn-btn secondary" disabled={busy} onClick={()=>onRemoveParent(p)}>Bỏ khỏi kế hoạch</button></div>)}</div>}
  </>}
 </section>
}
