import {useEffect,useState} from "react";
import {getHouseNumberingProposal,listHouseNumberingProposals,recordHouseNumberingIssuance,listHouseNumberingIssuances,getHouseNumberingIssuance,transitionHouseNumberingIssuance,houseNumberingAddressLookupUrl,type IssuanceDetail,type IssuanceInput,type IssuanceState} from "./api";
import {hnClient,uiError,withCsrf} from "./client";

type Props={organizationId:string;projectId:string};
type ApprovedItem={building_id:string;source_number_id:string;proposed_number:string};
const labels:Record<IssuanceState,string>={recorded:"Đã ghi nhận chứng nhận",printed:"Đã in biển",delivered:"Đã bàn giao",installed:"Đã lắp đặt",void:"Đã vô hiệu hóa"};
const next:Partial<Record<IssuanceState,Exclude<IssuanceState,"recorded">>>={recorded:"printed",printed:"delivered",delivered:"installed"};

export function IssuancePanel({organizationId:org,projectId:project}:Props){
 const [proposals,setProposals]=useState<{id:string;rule_version:string;item_count:number}[]>([]),[offset,setOffset]=useState(0),[more,setMore]=useState(false);
 const [proposal,setProposal]=useState(""),[items,setItems]=useState<ApprovedItem[]>([]),[number,setNumber]=useState("");
 const [detail,setDetail]=useState<IssuanceDetail|null>(null),[busy,setBusy]=useState(false),[error,setError]=useState("");
 const [records,setRecords]=useState<Awaited<ReturnType<typeof listHouseNumberingIssuances>>["items"]>([]);
 const [form,setForm]=useState<Omit<IssuanceInput,"proposal_id"|"number_id">>({authority_name:"",decision_ref:"",certificate_ref:"",effective_date:"",template_revision:""});
 const [evidence,setEvidence]=useState("");
 const item=items.find(i=>i.source_number_id===number),building=item?.building_id;
 useEffect(()=>{let live=true;listHouseNumberingProposals(hnClient,org,project,{status:"approved",offset,limit:50}).then(r=>{if(live){setProposals(r.items);setMore(r.next_offset!==null)}}).catch(async e=>{if(live)setError(await uiError(e))});return()=>{live=false}},[org,project,offset]);
 useEffect(()=>{let live=true;setItems([]);setNumber("");setDetail(null);setRecords([]);if(proposal)getHouseNumberingProposal(hnClient,org,project,proposal).then(r=>{if(live&&r.proposal.status==="approved")setItems(r.items.flatMap(i=>typeof i.building_id==="string"&&typeof i.source_number_id==="string"&&typeof i.proposed_number==="string"&&i.number_kind==="approved"?[{building_id:i.building_id,source_number_id:i.source_number_id,proposed_number:i.proposed_number}]:[]))}).catch(async e=>{if(live)setError(await uiError(e))});return()=>{live=false}},[org,project,proposal]);
 useEffect(()=>{let live=true;setRecords([]);setDetail(null);if(building)listHouseNumberingIssuances(hnClient,org,project,building).then(r=>{if(live)setRecords(r.items)}).catch(async e=>{if(live)setError(await uiError(e))});return()=>{live=false}},[org,project,building]);
 const refresh=async()=>{if(building)setRecords((await listHouseNumberingIssuances(hnClient,org,project,building)).items)};
 const run=async(task:()=>Promise<void>)=>{setBusy(true);setError("");try{await task()}catch(e){setError(await uiError(e))}finally{setBusy(false)}};
 const latest=detail?.history.at(-1),following=latest&&next[latest.state];
 const fields:[keyof typeof form,string][]=[["authority_name","Cơ quan cấp"],["decision_ref","Số quyết định"],["certificate_ref","Số chứng nhận"],["effective_date","Ngày hiệu lực"],["template_revision","Phiên bản biểu mẫu"]];
 return <section className="hn-card"><h3>Sổ chứng nhận và biển số</h3><p>Chỉ ghi nhận chứng nhận đã được cơ quan có thẩm quyền cấp. Không tự phát hành từ phương án đã duyệt.</p>
 <div className="hn-form">
 <label>Phương án đã duyệt<select disabled={busy} value={proposal} onChange={e=>setProposal(e.target.value)}><option value="">Chọn phương án</option>{proposals.map(p=><option key={p.id} value={p.id}>{p.rule_version} · {p.item_count} hồ sơ · {p.id.slice(0,8)}</option>)}</select></label>
 <button type="button" disabled={busy||offset===0} onClick={()=>{setProposal("");setOffset(v=>Math.max(0,v-50))}}>Trang trước</button>
 <button type="button" disabled={busy||!more} onClick={()=>{setProposal("");setOffset(v=>v+50)}}>Trang sau</button>
 <label>Số đã duyệt<select disabled={busy||!proposal} value={number} onChange={e=>{setNumber(e.target.value);setDetail(null)}}><option value="">Chọn số</option>{items.map(i=><option key={i.source_number_id} value={i.source_number_id}>{i.proposed_number} · {i.building_id.slice(0,8)}</option>)}</select></label>
 {fields.map(([key,label])=><label key={key}>{label}<input disabled={busy} type={key==="effective_date"?"date":"text"} maxLength={key==="template_revision"?100:200} value={form[key]} onChange={e=>setForm(v=>({...v,[key]:e.target.value}))}/></label>)}
 <button className="hn-btn" type="button" disabled={busy||!number||Object.values(form).some(v=>!v.trim())} onClick={()=>void run(async()=>{const result=await withCsrf(()=>recordHouseNumberingIssuance(hnClient,org,project,{proposal_id:proposal,number_id:number,...form}));setDetail(result);await refresh()})}>Ghi nhận chứng nhận đã cấp</button>
 </div>
 {records.map(r=><div key={r.id}><button type="button" disabled={busy} onClick={()=>void run(async()=>{setDetail(await getHouseNumberingIssuance(hnClient,org,project,r.id))})}>{r.number_text} {r.route_name} · {labels[r.state]} · {r.certificate_ref}</button></div>)}
 {detail&&latest&&<div><strong>{detail.record.number_text} {detail.record.route_name}</strong><p>{detail.record.certificate_ref} · {labels[latest.state]}</p>
 <ol>{detail.history.map(e=><li key={e.revision}>{labels[e.state]} · {e.evidence_ref} · {new Date(e.occurred_at).toLocaleString("vi-VN")}</li>)}</ol>
 {latest.state!=="void"&&<><a href={houseNumberingAddressLookupUrl(org,project,detail.record.id)} target="_blank" rel="noreferrer">Đường dẫn tra cứu nội bộ (dùng làm nội dung QR)</a><label>Tham chiếu bằng chứng / lý do<input value={evidence} maxLength={500} disabled={busy} onChange={e=>setEvidence(e.target.value)}/></label>
 {[following,"void" as const].filter((s):s is Exclude<IssuanceState,"recorded">=>!!s).map(s=><button key={s} type="button" disabled={busy||!evidence.trim()} onClick={()=>void run(async()=>{const result=await withCsrf(()=>transitionHouseNumberingIssuance(hnClient,org,project,detail.record.id,{expected_revision:latest.revision,state:s,evidence_ref:evidence}));setDetail(result);setEvidence("");await refresh()})}>{labels[s]}</button>)}</>}
 </div>}
 {error&&<p role="alert" className="hn-error">{error}</p>}
 </section>
}
