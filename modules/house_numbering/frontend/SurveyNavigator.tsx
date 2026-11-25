import React,{useMemo} from "react";
export type SurveyFilter={area:string;road:string;alley:string;status:string;condition:string};
export const emptySurveyFilter=():SurveyFilter=>({area:"",road:"",alley:"",status:"",condition:""});
export type SurveyRow={key:string;feature:{attributes:Record<string,unknown>}};
export function surveyText(row:SurveyRow,field:string){return String(row.feature.attributes[field]??"").trim()}
const fold=(s:string)=>s.normalize("NFD").replace(/\p{M}/gu,"").replace(/[đĐ]/g,"d").toLowerCase().replace(/[^a-z0-9]+/g," ").trim();
const NUMBER_FIELDS=["so_nha","so_sua","so_de_xuat","so_luoi"],SEARCH_FIELDS=[...NUMBER_FIELDS,"ten_duong","hem","ap","ma_phieu","ma_goc"];
const searchIndex=new WeakMap<object,{numbers:string[];tokens:string[]}>();
function indexed(row:SurveyRow){
 let entry=searchIndex.get(row.feature.attributes);
 if(!entry){entry={numbers:NUMBER_FIELDS.map(k=>fold(surveyText(row,k))).filter(Boolean),tokens:SEARCH_FIELDS.flatMap(k=>fold(surveyText(row,k)).split(" ")).filter(Boolean)};searchIndex.set(row.feature.attributes,entry)}
 return entry;
}
// Ignores accents, case and "/"/"-"; every typed word must start a word of the point. Exact house number first, then fewer prefix-only words.
export function searchSurvey<T extends SurveyRow>(rows:readonly T[],query:string,limit=100):T[]{
 const q=fold(query),words=q.split(" ").filter(Boolean);
 if(!words.length)return [];
 const hits:[number,T][]=[];
 for(const row of rows){
  const {numbers,tokens}=indexed(row);let score=numbers.includes(q)?0:1;
  for(const w of words){if(tokens.includes(w))continue;if(!tokens.some(t=>t.startsWith(w))){score=-1;break}score++}
  if(score>=0)hits.push([score,row]);
 }
 return hits.sort((a,b)=>a[0]-b[0]).slice(0,limit).map(h=>h[1]);
}
const NO_ALLEY="__no_alley__";
// Bình Hưng rule: a point still needs work while it is not "khop" and nobody has handled it (xu_ly).
export const XU_LY_LABELS:Record<string,string>={chua_xu_ly:"Chưa đối chiếu",giu:"Đã đối chiếu · giữ số",da_sua:"Đã đối chiếu · đã sửa",bo:"Đã đối chiếu · loại"};
export const needsWork=(row:SurveyRow)=>surveyText(row,"trang_thai")!=="khop"&&(surveyText(row,"xu_ly")||"chua_xu_ly")==="chua_xu_ly";
export function surveyProgress(rows:readonly SurveyRow[]){let pending=0;for(const r of rows)if(needsWork(r))pending++;const total=rows.length;return {total,pending,done:total-pending,percent:total?Math.floor((total-pending)*100/total):0}}
export type ScopeSignoff={id:string;ap:string;ten_duong:string|null;signed_at:string;point_count:number};
export const signoffFor=(signoffs:readonly ScopeSignoff[],ap:string,road:string)=>signoffs.find(s=>s.ap===ap&&(!s.ten_duong||s.ten_duong===road));
export function matchesSurvey(row:SurveyRow,filter:SurveyFilter){
 return (!filter.area||surveyText(row,"ap")===filter.area)&&(!filter.road||surveyText(row,"ten_duong")===filter.road)
 &&(!filter.alley||(filter.alley===NO_ALLEY?!surveyText(row,"hem"):surveyText(row,"hem")===filter.alley))
 &&(!filter.status||surveyText(row,"trang_thai")===filter.status)&&(!filter.condition||surveyText(row,"hien_trang")===filter.condition);
}
function choices(rows:SurveyRow[],field:string,blank=false){const totals=new Map<string,number>();for(const r of rows){const value=surveyText(r,field)||(blank?NO_ALLEY:"");if(value)totals.set(value,(totals.get(value)??0)+1)}
 return [...totals].sort(([a],[b])=>a===NO_ALLEY?-1:b===NO_ALLEY?1:a.localeCompare(b,"vi",{numeric:true}));}
export function SurveyNavigator({rows,filter,change,total,buildingCount,loading,disabled,statusLabels,statusColors,areaNames=[],signoffs=[],canSign=false,sign,revoke}:{rows:SurveyRow[];filter:SurveyFilter;change:(f:SurveyFilter)=>void;total:number;buildingCount:number;loading:boolean;disabled:boolean;statusLabels:Record<string,string>;statusColors:Record<string,string>;areaNames?:string[];signoffs?:readonly ScopeSignoff[];canSign?:boolean;sign?:(ap:string,road:string|null)=>void;revoke?:(signoff:ScopeSignoff)=>void}){
 const stats=useMemo(()=>{const areaRows=rows.filter(r=>!filter.area||surveyText(r,"ap")===filter.area),roadRows=areaRows.filter(r=>!filter.road||surveyText(r,"ten_duong")===filter.road),matching=rows.filter(r=>matchesSurvey(r,filter));return {areas:choices(rows,"ap"),roads:choices(areaRows,"ten_duong"),alleys:choices(roadRows,"hem",true),conditions:choices(areaRows,"hien_trang"),statuses:choices(roadRows,"trang_thai"),count:matching.length,progress:filter.area?surveyProgress(filter.road?roadRows:areaRows):null}},[rows,filter]);
 const areaOptions=useMemo(()=>{const m=new Map(stats.areas);for(const name of areaNames)if(!m.has(name))m.set(name,0);return [...m].sort(([a],[b])=>a.localeCompare(b,"vi",{numeric:true}))},[stats.areas,areaNames]);
 const update=(patch:Partial<SurveyFilter>)=>change({...filter,...patch});
 const options=(v:[string,number][])=>v.map(([s,n])=><option key={s} value={s}>{s===NO_ALLEY?"Mặt đường / không hẻm":s} · {n.toLocaleString("vi-VN")}</option>);
 const active=Object.values(filter).some(Boolean);
 return <div className="survey-nav"><div className="survey-summary"><span className="survey-summary-kicker">KHẢO SÁT SỐ NHÀ</span><strong>{total.toLocaleString("vi-VN")}</strong><small>Điểm khảo sát trên bản đồ</small><span>{buildingCount.toLocaleString("vi-VN")} hồ sơ công trình</span></div>
 <div className="survey-nav-heading"><b>Lọc theo địa bàn</b>{active&&<button disabled={disabled} onClick={()=>change(emptySurveyFilter())}>Xóa lọc</button>}</div>
 <label>Ấp<select aria-label="Lọc theo ấp" value={filter.area} disabled={disabled} onChange={e=>update({area:e.target.value,road:"",alley:""})}><option value="">Tất cả ấp</option>{options(areaOptions)}</select></label>
 <label>Đường<select aria-label="Lọc theo đường" value={filter.road} disabled={disabled} onChange={e=>update({road:e.target.value,alley:""})}><option value="">Tất cả đường</option>{options(stats.roads)}</select></label>
 <label>Hẻm<select aria-label="Lọc theo hẻm" value={filter.alley} disabled={disabled} onChange={e=>update({alley:e.target.value})}><option value="">Tất cả hẻm</option>{options(stats.alleys)}</select></label>
 <label>Hiện trạng<select aria-label="Lọc hiện trạng" value={filter.condition} disabled={disabled} onChange={e=>update({condition:e.target.value})}><option value="">Tất cả hiện trạng</option>{options(stats.conditions)}</select></label>
 {stats.progress&&<ScopeProgress ap={filter.area} road={filter.road} progress={stats.progress} signoff={signoffFor(signoffs,filter.area,filter.road)} canSign={canSign&&!disabled} sign={sign} revoke={revoke}/>}
 <div className="survey-nav-heading"><b>Trạng thái khảo sát</b></div>
 <div className="survey-status-options"><button disabled={disabled} className={!filter.status?"active":""} onClick={()=>update({status:""})}>Tất cả</button>{Object.entries(statusLabels).map(([k,v])=><button disabled={disabled} key={k} className={filter.status===k?"active":""} onClick={()=>update({status:filter.status===k?"":k})}><i style={{background:statusColors[k]}}/>{v}<small>{(stats.statuses.find(([s])=>s===k)?.[1]??0).toLocaleString("vi-VN")}</small></button>)}</div>
 <div className="survey-filter-result" role="status"><b>{stats.count.toLocaleString("vi-VN")}</b> điểm phù hợp{loading?" · đang tải thêm":""}</div>
 <p className="survey-hint">{areaNames.length?`Đã có ranh giới ${areaNames.length} ấp từ dữ liệu gốc. Chọn ấp để định vị trên bản đồ; số điểm lọc theo thuộc tính khảo sát.`:"Phân nhóm theo thuộc tính gốc. Chưa có ranh giới ấp."}</p></div>
}
function ScopeProgress({ap,road,progress,signoff,canSign,sign,revoke}:{ap:string;road:string;progress:ReturnType<typeof surveyProgress>;signoff:ScopeSignoff|undefined;canSign:boolean;sign?:(ap:string,road:string|null)=>void;revoke?:(signoff:ScopeSignoff)=>void}){
 const label=road?ap+" · "+road:ap,complete=progress.total>0&&progress.pending===0;
 return <section className="survey-progress" aria-label={"Tiến độ đối chiếu "+label}>
  <div className="survey-nav-heading"><b>Đối chiếu {road?"tuyến":"ấp"}</b><span>{progress.percent}%</span></div>
  <progress value={progress.done} max={Math.max(progress.total,1)} aria-label={"Đã đối chiếu "+progress.done+" trên "+progress.total+" điểm"}/>
  <small>{progress.done.toLocaleString("vi-VN")}/{progress.total.toLocaleString("vi-VN")} điểm{progress.pending?" · còn "+progress.pending.toLocaleString("vi-VN")+" điểm chưa đối chiếu":""}</small>
  {signoff?<><p className="survey-signed">✓ Đã ký duyệt {new Date(signoff.signed_at).toLocaleDateString("vi-VN")}{signoff.ten_duong&&!road?" (tuyến "+signoff.ten_duong+")":""} · đang khóa sửa</p>{canSign&&revoke&&<button type="button" onClick={()=>revoke(signoff)}>Mở khóa để sửa</button>}</>
  :canSign&&sign?<button type="button" className="primary" disabled={!complete} title={complete?"Ký xác nhận và khóa "+label:"Cần đối chiếu hết các điểm trước khi ký"} onClick={()=>sign(ap,road||null)}>Ký duyệt hoàn thành {road?"tuyến":"ấp"}</button>
  :complete?<p className="survey-hint">Đã đối chiếu xong, chờ người duyệt ký.</p>:null}
 </section>;
}
