import {useEffect,useMemo,useState} from "react";
import {hnClient,uiError,withCsrf} from "./client";
import type {HouseNumberingRoute} from "./api";
import {TopologyRecoveryPanel,type MergeOption,type MissingParent,type ParentDraft} from "./TopologyRecoveryPanel";
import {SurveyConflictPanel,type SurveyReview} from "./SurveyConflictPanel";
import {GeometryReviewPanel} from "./GeometryReviewPanel";

type Field={field_id:string;name:string};
type Source={id:string;name:string;validation_state:string;schema:{geometry_family:string;fields:Field[]}};
type MapKey="name"|"code"|"parent_code"|"prefix"|"route_type"|"logical_code";
type Layer={source_id:string;default_type:"street"|"alley";direction_state:"forward"|"reverse";mapping:Partial<Record<MapKey,string>>};
type Override={parent_code:string;prefix:string;name?:string;route_type?:"street"|"alley";reviewed?:boolean};
type Suggestion={code:string;name:string;root:string;prefix:string;state:string;distance_m:number|null;address_match:boolean;connected:boolean;current:boolean};
type Row={key:string;code:string;name:string;route_type:"street"|"alley";parent_code:string;parent_key?:string|null;prefix:string;resolved_prefix:string|null;source_id:string;feature_id:string;state:"ready"|"existing"|"error";existing_id?:string;logical_code?:string;recovery_group?:boolean;manual_parent?:boolean;manual_parent_child_count?:number;legacy_parent_codes?:string[];errors:string[];review_reason?:string;reviewed?:boolean;survey_evidence?:{referenced:number;unassigned:number;address_conflicts:number};survey_review?:SurveyReview;address_review?:{total:number;verified:number;pending:number;blocked:number;geometry_current:boolean};geometry_stage_eligible?:boolean;expected_parent_prefix?:string;expected_root?:string;suggested_type?:"alley";suggested_prefix?:string;parent_evidence?:Suggestion|null;parent_search_truncated?:boolean;segments?:{source_id:string;feature_id:string;code:string}[];parent_suggestions?:Suggestion[]};
type Preview={preview_token:string;items:Row[];ready:number;existing:number;errors:number;merge_options?:MergeOption[];missing_parents?:MissingParent[]};
type Draft={layers:Layer[];overrides:Record<string,Override>;groups:string[];parents:ParentDraft[]};
// Keep unsaved working state while the operator goes to the map and reopens the panel. Never persist a preview token.
const workDrafts=new Map<string,Draft>();
const fields:Array<[MapKey,string]>=[["name","Tên tuyến"],["code","Mã đoạn GIS"],["logical_code","Mã tuyến logic (nhóm đoạn)"],["parent_code","Mã tuyến cha"],["prefix","Prefix / số nhánh"],["route_type","Loại tuyến"]];
const labels:Record<string,string>={invalid_name:"Thiếu tên tuyến",duplicate_or_empty_code:"Mã trống hoặc trùng",parent_not_found:"Không tìm thấy tuyến cha",parent_required:"Chưa chọn tuyến cha",parent_has_errors:"Tuyến cha có lỗi",cycle:"Quan hệ vòng lặp",invalid_prefix:"Prefix không hợp lệ",prefix_must_be_one_level:"Prefix cấp 1 phải là một thành phần (VD 30A hoặc C19)",prefix_does_not_follow_parent:"Prefix không khớp tuyến cha",parent_prefix_missing:"Tuyến cha chưa có prefix",duplicate_sibling_prefix:"Prefix trùng trong cùng tuyến cha",street_cannot_have_parent_or_prefix:"Đường chính không có cha/prefix",existing_route_conflict:"Khác dữ liệu đã liên kết",invalid_line_geometry:"Hình học không hợp lệ",invalid_route_type:"Loại tuyến không hợp lệ",multiple_existing_routes:"Trùng liên kết tuyến",prefix_too_long:"Prefix quá dài",invalid_route_segments:"Đoạn tuyến không liên tục hoặc không cùng CRS",inconsistent_logical_route:"Các đoạn thuộc tuyến logic có thuộc tính mâu thuẫn",too_many_segments:"Quá 200 đoạn trong một tuyến",review_required:"Chưa xác nhận đối soát",review_address_mismatch:"Cây cha–con không khớp tên đường và prefix đầy đủ",review_parent_not_connected:"Tuyến cha chưa nối điểm đầu/cuối (≤ 8 m)",review_duplicate_address_path:"Địa chỉ đã có hoặc trùng đoạn; cần ghép/đối soát",review_alley_type_required:"Tên chỉ ra đây là hẻm, cần sửa loại",review_survey_address_conflict:"Địa chỉ khảo sát mâu thuẫn, cần xác minh từng nhà",review_address_pending:"Còn hồ sơ địa chỉ chưa xác minh",review_stage_stale:"Hình học kho chờ đã đổi, cần xem lại",review_building_already_linked:"Có nhà đã liên kết tuyến khác, cần đối soát",review_parent_has_no_connected_child:"Đường GIS bổ sung chưa nối với hẻm con trực tiếp",manual_parent_feature:"Tuyến cha bổ sung từ đối tượng GIS"};
const norm=(s:string)=>s.toLowerCase().normalize("NFD").replace(/[\u0300-\u036f]/g,"").replace(/đ/g,"d").replace(/[^a-z0-9]/g,"");
const aliases:Record<MapKey,string[]>={name:["ten","name","tentuyen","tenduong","tenhem"],code:["matuyen","tuyenid","maduong","mahem","code","routecode","id"],parent_code:["macha","matuyencha","tuyenchaid","tuyenidcha","parenttuyenid","parentcode","parentid","parent"],prefix:["prefix","sohem","hem","sonhanh","sonha"],route_type:["loaituyen","routetype","type","loai"],logical_code:["matuyenlogic","logicalcode","logicalrouteid"]};
const blank=():Layer=>({source_id:"",mapping:{},default_type:"street",direction_state:"forward"});
export function RouteImportPanel({organizationId,projectId,routes,onDone,onGoto,handoffSource="",handoffFeature=""}:{organizationId:string;projectId:string;routes:HouseNumberingRoute[];onDone:()=>void;onGoto?:(source:string,feature:string)=>void;handoffSource?:string;handoffFeature?:string}){
 const draftKey=organizationId+":"+projectId;
 const [sources,setSources]=useState<Source[]>([]),[layers,setLayers]=useState<Layer[]>(()=>workDrafts.get(draftKey)?.layers??[blank()]),[overrides,setOverrides]=useState<Record<string,Override>>(()=>workDrafts.get(draftKey)?.overrides??{}),[preview,setPreview]=useState<Preview|null>(null),[stale,setStale]=useState(false),[busy,setBusy]=useState(false),[error,setError]=useState(""),[notice,setNotice]=useState(""),[filter,setFilter]=useState("all"),[query,setQuery]=useState(""),[page,setPage]=useState(0),[selected,setSelected]=useState<Set<string>>(new Set()),[reason,setReason]=useState("");
 const [mergeGroups,setMergeGroups]=useState<string[]>(()=>workDrafts.get(draftKey)?.groups??[]),[parentFeatures,setParentFeatures]=useState<ParentDraft[]>(()=>workDrafts.get(draftKey)?.parents??[]);
 useEffect(()=>{workDrafts.set(draftKey,{layers,overrides,groups:mergeGroups,parents:parentFeatures})},[draftKey,layers,overrides,mergeGroups,parentFeatures]);
 const path=`/organizations/${encodeURIComponent(organizationId)}/projects/${encodeURIComponent(projectId)}`;
 const endpoint=path+"/modules/house-numbering/route-import";
 useEffect(()=>{let live=true;void(async()=>{try{let after:string|null=null;const all:Source[]=[];do{const res:{items:Source[];next_after_id:string|null}=await hnClient.request(path+"/sources"+(after?"?after_id="+encodeURIComponent(after):""));all.push(...res.items);after=res.next_after_id;}while(after);if(live)setSources(all.filter(s=>s.validation_state==="ready"&&["LineString","MultiLineString"].includes(s.schema.geometry_family)));}catch(e){if(live)setError(await uiError(e));}})();return()=>{live=false}},[path]);
 const reset=()=>{setPreview(null);setOverrides({});setMergeGroups([]);setParentFeatures([]);setSelected(new Set());setReason("");setNotice("");setPage(0);setStale(false)};
 const change=(i:number,patch:Partial<Layer>)=>{setLayers(old=>old.map((l,j)=>i===j?{...l,...patch}:l));reset()};
 const choose=(i:number,id:string)=>{const src=sources.find(s=>s.id===id);const mapping:Layer["mapping"]={};if(src)for(const [key] of fields){const f=src.schema.fields.find(f=>aliases[key].includes(norm(f.name)));if(f)mapping[key]=f.field_id;}change(i,{source_id:id,mapping})};
 const body=()=>({layers:layers.map(l=>({...l,mapping:Object.fromEntries(Object.entries(l.mapping).filter(([,v])=>v))})),overrides,merge_groups:mergeGroups,parent_features:parentFeatures.map(({task_id,source_id,feature_id})=>({task_id,source_id,feature_id}))});
 async function run(apply=false){
  if(apply&&(!preview||stale||selected.size===0||!window.confirm(`Tạo ${selected.size} tuyến dự thảo đã chọn? Không sửa tuyến hiện có, hình học GIS hoặc liên kết nhà.`)))return;
  setBusy(true);setError("");setNotice("");
  try{
   if(apply&&preview){
    const r=await withCsrf(()=>hnClient.request<{created:number;existing:number;remaining:number}>(endpoint+"/apply",{method:"POST",body:JSON.stringify({...body(),preview_token:preview.preview_token,selected_keys:[...selected]})}));
    setNotice(`Đã tạo ${r.created} tuyến dự thảo · còn ${r.remaining} tuyến chưa tạo. Không đổi liên kết nhà.`);
    setPreview(null);setSelected(new Set());onDone();
   }
   const p=await withCsrf(()=>hnClient.request<Preview>(endpoint+"/preview",{method:"POST",body:JSON.stringify(body())}));
   setPreview(p);setStale(false);setSelected(new Set());setPage(0);
  }catch(e){setError(await uiError(e));}finally{setBusy(false)}
 }
 function amend(r:Row,patch:Partial<Override>){setOverrides(old=>({...old,[r.key]:{parent_code:r.parent_code,prefix:r.prefix,name:r.name,route_type:r.route_type,...old[r.key],...patch,reviewed:patch.reviewed??false}}));setStale(true);setSelected(new Set())}
 function toggleMerge(g:MergeOption,checked:boolean){
  setMergeGroups(old=>checked?[...new Set([...old,g.id])]:old.filter(id=>id!==g.id));
  setOverrides(old=>{const next={...old};for(const m of g.members)delete next[m.key];return next});setStale(true);setSelected(new Set());
 }
 function stageParent(p:ParentDraft){setParentFeatures(old=>[...old.filter(x=>x.task_id!==p.task_id),p]);setStale(true);setSelected(new Set())}
 function removeParent(p:ParentDraft){setParentFeatures(old=>old.filter(x=>x.task_id!==p.task_id));setOverrides(old=>{const next={...old};delete next[p.source_id+":"+p.feature_id];return next});setStale(true);setSelected(new Set())}
 function toggle(r:Row,checked:boolean){
  if(!preview||stale||r.state!=="ready")return;
  const byKey=new Map(preview.items.map(x=>[x.key,x]));
  setSelected(old=>{const next=new Set(old);
   if(checked){let item:Row|undefined=r;while(item?.state==="ready"&&!next.has(item.key)){next.add(item.key);item=item.parent_key?byKey.get(item.parent_key):undefined}}
   else{next.delete(r.key);let changed=true;while(changed){changed=false;for(const k of next){const pk=byKey.get(k)?.parent_key;if(pk&&byKey.get(pk)?.state==="ready"&&!next.has(pk)){next.delete(k);changed=true}}}}
   return next;
  })
 }
 const visible=useMemo(()=>preview?.items.filter(r=>(filter==="all"||filter===r.state)&&(!reason||r.errors.includes(reason)||r.review_reason===reason)&&norm(r.code+" "+r.name+" "+r.parent_code+" "+r.prefix).includes(norm(query)))??[],[preview,filter,query,reason]);
 const reasons=useMemo(()=>[...new Set(preview?.items.flatMap(r=>[...r.errors,...(r.review_reason?[r.review_reason]:[])])??[])].sort(),[preview]);
 const uniqueSuggestions=useMemo(()=>preview?.items.flatMap(r=>{
  if(!r.review_reason||r.existing_id||r.reviewed||r.errors.some(e=>["review_duplicate_address_path","review_survey_address_conflict","review_address_pending","review_stage_stale","review_building_already_linked"].includes(e))||!r.suggested_prefix)return [];
  const options=r.parent_suggestions?.filter(s=>s.address_match&&s.connected)??[];
  return options.length===1&&options[0]?.state==="existing"?[{row:r,parent:options[0]}]:[];
 })??[],[preview]);
 function fillSuggestions(){
  setOverrides(old=>{const next={...old};for(const {row,parent} of uniqueSuggestions){if(next[row.key])continue;next[row.key]={parent_code:parent.code,prefix:row.suggested_prefix??row.prefix,name:row.name,route_type:"alley",reviewed:false}}return next});
  setSelected(new Set());setStale(true);
 }
 const valid=layers.every(l=>l.source_id&&l.mapping.name)&&new Set(layers.map(l=>l.source_id)).size===layers.length;
 return <section className="hn-card"><h2>Import / phục hồi cây tuyến địa chỉ</h2><p className="hn-muted">Chọn lớp đường/hẻm đã nhập vào GIS Core. Gợi ý tuyến cha dựa trên tên đường, prefix và khoảng cách điểm nối. Có thể ghép nhiều đoạn bằng cùng mã tuyến logic. Tuyến cần đối soát phải được xác nhận, xem trước lại và chọn để tạo dự thảo; không thay đổi hình học hoặc liên kết nhà hiện tại.</p>
 {sources.length===0&&<p className="hn-muted">Chưa có lớp đường dạng LineString sẵn sàng. Hãy import SHP / GeoJSON / KML trong GIS Core trước.</p>}
 {layers.map((l,i)=>{const src=sources.find(s=>s.id===l.source_id);return <div key={i} style={{border:"1px solid var(--ovg-border,#dbe3ec)",borderRadius:8,padding:12,marginBottom:10}}>
 <div className="hn-form"><label>Lớp tuyến {i+1}<select value={l.source_id} disabled={busy} onChange={e=>choose(i,e.target.value)}><option value="">Chọn lớp GIS</option>{sources.map(s=><option key={s.id} value={s.id} disabled={layers.some((x,j)=>j!==i&&x.source_id===s.id)}>{s.name}</option>)}</select></label>
 <label>Loại mặc định<select value={l.default_type} disabled={busy} onChange={e=>change(i,{default_type:e.target.value as Layer["default_type"]})}><option value="street">Đường chính</option><option value="alley">Hẻm / nhánh</option></select></label>
 <label>Chiều số nhà<select value={l.direction_state} disabled={busy} onChange={e=>change(i,{direction_state:e.target.value as Layer["direction_state"]})}><option value="forward">Theo chiều hình học</option><option value="reverse">Ngược chiều hình học</option></select></label>
 {fields.map(([key,label])=><label key={key}>{label}{key==="name"?" *":""}<select value={l.mapping[key]??""} disabled={busy||!src} onChange={e=>change(i,{mapping:{...l.mapping,[key]:e.target.value}})}><option value="">{key==="code"?"Dùng Feature ID":key==="name"?"Chọn cột":"Không chọn"}</option>{src?.schema.fields.map(f=><option key={f.field_id} value={f.field_id}>{f.name}</option>)}</select></label>)}
 {layers.length>1&&<button type="button" className="hn-btn secondary" disabled={busy} onClick={()=>{setLayers(old=>old.filter((_,j)=>j!==i));reset()}}>Bỏ lớp</button>}</div></div>})}
 <div className="hn-actions"><button className="hn-btn secondary" disabled={busy||layers.length>=20} onClick={()=>{setLayers(old=>[...old,blank()]);reset()}}>+ Thêm lớp</button><button className="hn-btn" disabled={!valid||busy} onClick={()=>void run()}>{busy?"Đang xử lý…":"Xem trước cây tuyến"}</button></div>
 {error&&<p role="alert" style={{color:"#dc2626"}}>{error}</p>}{notice&&<p role="status" style={{color:"#15803d"}}>{notice}</p>}
 <GeometryReviewPanel endpoint={path+"/modules/house-numbering"} previewToken={preview&&!stale?preview.preview_token:null} candidates={preview?.items??[]} request={body} disabled={busy||stale} onBusy={setBusy} onGoto={onGoto}/>
 {preview&&<><TopologyRecoveryPanel options={preview.merge_options??[]} missing={preview.missing_parents??[]} mergeGroups={mergeGroups} parents={parentFeatures} sources={sources} handoffSource={handoffSource} handoffFeature={handoffFeature} busy={busy} stale={stale} onMerge={toggleMerge} onParent={stageParent} onRemoveParent={removeParent} onGoto={onGoto}/><div className="hn-actions" style={{marginTop:16}}><b>{preview.ready} tuyến mới</b><span>{preview.existing} đã có</span><span style={{color:preview.errors?"#dc2626":"inherit"}}>{preview.errors} lỗi</span>{stale&&<strong>Đã sửa — cần xem trước lại</strong>}{uniqueSuggestions.length>0&&<button className="hn-btn secondary" disabled={busy||stale} onClick={fillSuggestions}>Điền {uniqueSuggestions.length} gợi ý cha duy nhất</button>}</div>
 <div className="hn-form"><label>Lọc<select value={filter} onChange={e=>{setFilter(e.target.value);setPage(0)}}><option value="all">Tất cả</option><option value="error">Có lỗi</option><option value="ready">Có thể tạo</option><option value="existing">Đã tồn tại</option></select></label><label>Nguyên nhân<select value={reason} onChange={e=>{setReason(e.target.value);setPage(0)}}><option value="">Tất cả</option>{reasons.map(r=><option key={r} value={r}>{labels[r]??r}</option>)}</select></label><label>Tìm tuyến<input value={query} onChange={e=>{setQuery(e.target.value);setPage(0)}}/></label></div>
 <datalist id="hn-parent-route-options">{routes.map(s=><option key={s.id} value={s.id}>{s.name} {s.prefix??""}</option>)}</datalist>
 <div style={{overflow:"auto",maxHeight:500}}><table style={{width:"100%",minWidth:820,borderCollapse:"collapse",fontSize:12}}><thead style={{position:"sticky",top:0,background:"var(--ovg-surface,#fff)",zIndex:1}}><tr>{["Chọn","Mã / tên tuyến","Loại","Tuyến cha / bằng chứng","Prefix","Kiểm tra","Bản đồ"].map(h=><th key={h} style={{padding:8,textAlign:"left"}}>{h}</th>)}</tr></thead>
 <tbody>{visible.slice(page*50,page*50+50).map(r=>{
  const o=overrides[r.key],kind=o?.route_type??r.route_type,locked=busy||!!r.existing_id||r.state==="existing",identityLocked=!!r.manual_parent||!!r.recovery_group;
  return <tr key={r.key} style={{borderTop:"1px solid var(--ovg-border,#e2e8f0)",verticalAlign:"top"}}>
   <td style={{padding:8}}><input type="checkbox" aria-label={"Chọn "+r.code} disabled={busy||stale||r.state!=="ready"} checked={selected.has(r.key)} onChange={e=>toggle(r,e.target.checked)}/></td>
   <td style={{padding:8,minWidth:160}}><b style={{overflowWrap:"anywhere"}}>{r.code}</b>{r.review_reason&&!locked&&!identityLocked?<input aria-label={"Tên tuyến "+r.code} value={o?.name??r.name} onChange={e=>amend(r,{name:e.target.value})} style={{width:"100%"}}/>:<div>{r.name}</div>}{r.segments&&<small style={{display:"block"}}>{r.segments.length} đoạn GIS</small>}{r.legacy_parent_codes&&<small style={{display:"block"}}>Mã cha gốc: {r.legacy_parent_codes.join(", ")||"—"}</small>}{r.manual_parent&&<small style={{display:"block"}}>Cha bổ sung · {r.manual_parent_child_count??0} con nối liền</small>}</td>
   <td style={{padding:8}}>{locked||identityLocked?(kind==="street"?"Đường":"Hẻm"):<select aria-label={"Loại tuyến "+r.code} value={kind} onChange={e=>amend(r,{route_type:e.target.value as Row["route_type"]})}><option value="street">Đường</option><option value="alley">Hẻm</option></select>}{!locked&&!identityLocked&&kind!=="alley"&&r.suggested_type==="alley"&&<button className="hn-btn secondary" onClick={()=>amend(r,{route_type:"alley",prefix:r.suggested_prefix??r.prefix})}>Đổi thành hẻm</button>}</td>
   <td style={{padding:8,minWidth:215}}>
    {kind==="alley"?<>
     <input aria-label={"Mã cha "+r.code} list="hn-parent-route-options" placeholder="Mã hoặc ID tuyến cha" style={{width:"100%"}} value={o?.parent_code??r.parent_code} onChange={e=>amend(r,{parent_code:e.target.value})} disabled={locked}/>
     <select aria-label={"Gợi ý cha "+r.code} value="" disabled={locked} style={{width:"100%"}} onChange={e=>e.target.value&&amend(r,{parent_code:e.target.value,...(r.suggested_prefix?{prefix:r.suggested_prefix}:{})})}><option value="">Gợi ý cha ({r.parent_suggestions?.length??0})</option>{r.parent_suggestions?.map(s=><option key={s.code} value={s.code}>{s.name} · {s.prefix||"đường chính"} · {s.distance_m===null?"thiếu hình học":s.distance_m+" m"} · {s.address_match&&s.connected?"✓ khớp":"cần kiểm tra"}{s.state==="error"?" · cha lỗi":""}</option>)}</select>
    </>:"—"}
    {r.expected_root!==undefined&&<small style={{display:"block"}}>Địa chỉ cha dự kiến: {r.expected_root||"chưa rõ đường"} · {r.expected_parent_prefix||"đường chính"}</small>}
    {r.parent_evidence&&<small style={{display:"block",color:r.parent_evidence.connected&&r.parent_evidence.address_match?"#15803d":"#b91c1c"}}>Cha đang chọn: {r.parent_evidence.name} · {r.parent_evidence.distance_m===null?"không có khoảng cách":r.parent_evidence.distance_m+" m"} · {r.parent_evidence.address_match?"khớp địa chỉ":"khác địa chỉ"}</small>}
    {r.parent_search_truncated&&<small>Danh sách rộng, cần xác định tên đường.</small>}
   </td>
   <td style={{padding:8}}>{kind==="alley"?<><input aria-label={"Prefix "+r.code} style={{width:100}} disabled={locked||identityLocked} value={o?.prefix??r.prefix} onChange={e=>amend(r,{prefix:e.target.value})}/><small style={{display:"block"}}>Kết quả: {r.resolved_prefix??"—"}</small>{r.suggested_prefix&&r.suggested_prefix!==(o?.prefix??r.prefix)&&!locked&&!identityLocked&&<button className="hn-btn secondary" onClick={()=>amend(r,{prefix:r.suggested_prefix??r.prefix})}>{r.suggested_prefix}</button>}</>:"—"}</td>
   <td style={{padding:8,minWidth:155}}>
    <span style={{color:r.state==="error"?"#b91c1c":"#15803d"}}>{r.errors.length?r.errors.map(e=>labels[e]??e).join("; "):r.state==="existing"?"Đã có":"Hợp lệ"}</span>
    {r.review_reason&&<small style={{display:"block"}}>Nguồn: {labels[r.review_reason]??r.review_reason}</small>}
    {r.survey_evidence&&<small style={{display:"block"}}>Nhà theo mã gốc: {r.survey_evidence.referenced} · chưa liên kết {r.survey_evidence.unassigned} · mâu thuẫn {r.survey_evidence.address_conflicts}</small>}
    {r.address_review&&<small style={{display:"block"}}>Xác minh địa chỉ: {r.address_review.verified}/{r.address_review.total} · còn {r.address_review.pending} · khóa {r.address_review.blocked}</small>}
    {r.survey_review&&<SurveyConflictPanel review={r.survey_review} onGoto={onGoto}/>}
    {r.review_reason&&!locked&&<label style={{display:"flex",gap:4,alignItems:"center",marginTop:6}}><input type="checkbox" checked={o?.reviewed??r.reviewed??false} onChange={e=>amend(r,{reviewed:e.target.checked})}/>Đã đối soát địa chỉ khảo sát và vị trí</label>}
   </td>
   <td style={{padding:8}}>{onGoto&&<button className="hn-btn secondary" onClick={()=>onGoto(r.source_id,r.feature_id)}>Xem</button>}</td>
  </tr>})}</tbody></table></div>
 <div className="hn-actions"><button disabled={page===0} onClick={()=>setPage(p=>p-1)}>‹</button><span>{visible.length?Math.min(page*50+1,visible.length):0}–{Math.min((page+1)*50,visible.length)} / {visible.length}</span><button disabled={(page+1)*50>=visible.length} onClick={()=>setPage(p=>p+1)}>›</button><button className="hn-btn secondary" disabled={busy||stale||preview.ready===0} onClick={()=>setSelected(new Set(preview.items.filter(r=>r.state==="ready").map(r=>r.key)))}>Chọn toàn bộ {preview.ready} hợp lệ</button><button className="hn-btn secondary" disabled={busy||selected.size===0} onClick={()=>setSelected(new Set())}>Bỏ chọn</button><button className="hn-btn" disabled={busy||stale||selected.size===0} onClick={()=>void run(true)}>Tạo {selected.size} tuyến đã chọn</button></div>
 <p className="hn-muted">Khi chọn tuyến con, tuyến cha mới cần thiết cũng được chọn. Các dòng lỗi không được tạo; liên kết nhà vẫn giữ nguyên. Sau khi chỉnh sửa, bấm Xem trước cây tuyến lại.</p></>}
 </section>
}
