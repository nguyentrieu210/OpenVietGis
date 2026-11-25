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
export function matchesSurvey(row:SurveyRow,filter:SurveyFilter){
 return (!filter.area||surveyText(row,"ap")===filter.area)&&(!filter.road||surveyText(row,"ten_duong")===filter.road)
 &&(!filter.alley||(filter.alley===NO_ALLEY?!surveyText(row,"hem"):surveyText(row,"hem")===filter.alley))
 &&(!filter.status||surveyText(row,"trang_thai")===filter.status)&&(!filter.condition||surveyText(row,"hien_trang")===filter.condition);
}
function choices(rows:SurveyRow[],field:string,blank=false){const totals=new Map<string,number>();for(const r of rows){const value=surveyText(r,field)||(blank?NO_ALLEY:"");if(value)totals.set(value,(totals.get(value)??0)+1)}
 return [...totals].sort(([a],[b])=>a===NO_ALLEY?-1:b===NO_ALLEY?1:a.localeCompare(b,"vi",{numeric:true}));}
export function SurveyNavigator({rows,filter,change,total,buildingCount,loading,disabled,statusLabels,statusColors,areaNames=[]}:{rows:SurveyRow[];filter:SurveyFilter;change:(f:SurveyFilter)=>void;total:number;buildingCount:number;loading:boolean;disabled:boolean;statusLabels:Record<string,string>;statusColors:Record<string,string>;areaNames?:string[]}){
 const stats=useMemo(()=>{const areaRows=rows.filter(r=>!filter.area||surveyText(r,"ap")===filter.area),roadRows=areaRows.filter(r=>!filter.road||surveyText(r,"ten_duong")===filter.road),matching=rows.filter(r=>matchesSurvey(r,filter));return {areas:choices(rows,"ap"),roads:choices(areaRows,"ten_duong"),alleys:choices(roadRows,"hem",true),conditions:choices(areaRows,"hien_trang"),statuses:choices(roadRows,"trang_thai"),count:matching.length}},[rows,filter]);
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
 <div className="survey-nav-heading"><b>Trạng thái khảo sát</b></div>
 <div className="survey-status-options"><button disabled={disabled} className={!filter.status?"active":""} onClick={()=>update({status:""})}>Tất cả</button>{Object.entries(statusLabels).map(([k,v])=><button disabled={disabled} key={k} className={filter.status===k?"active":""} onClick={()=>update({status:filter.status===k?"":k})}><i style={{background:statusColors[k]}}/>{v}<small>{(stats.statuses.find(([s])=>s===k)?.[1]??0).toLocaleString("vi-VN")}</small></button>)}</div>
 <div className="survey-filter-result" role="status"><b>{stats.count.toLocaleString("vi-VN")}</b> điểm phù hợp{loading?" · đang tải thêm":""}</div>
 <p className="survey-hint">{areaNames.length?`Đã có ranh giới ${areaNames.length} ấp từ dữ liệu gốc. Chọn ấp để định vị trên bản đồ; số điểm lọc theo thuộc tính khảo sát.`:"Phân nhóm theo thuộc tính gốc. Chưa có ranh giới ấp."}</p></div>
}
