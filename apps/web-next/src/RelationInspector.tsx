import {useEffect,useRef,useState} from "react";
import {loadCsrfToken,executeLayerQuery,type ApiClient,type SpatialLayer} from "@openvietgis/client-sdk";
import type {StudioRow} from "./studio-contract";
import {readRelationSpec,readableRelationLayer,queryableRelationLayer,relationMatches,queryRelation,type RelatedRecord,type RelationSpec} from "./relation-inspector";
import {readPreference,writePreference} from "./spatial-resume";
const empty:RelationSpec={target:"",localField:"",targetField:"",cardinality:"many"};
export function RelationInspector({row,layers,rows,storageKey,restricted,navigate,disabled,client,org,pid,navigateRemote}:{row:StudioRow;layers:SpatialLayer[];rows:StudioRow[];storageKey:string;restricted:boolean;navigate:(key:string)=>void;disabled:boolean;client:ApiClient;org:string;pid:string;navigateRemote:(layer:SpatialLayer,id:string,current:()=>boolean)=>Promise<void>}){
 const [spec,setSpec]=useState<RelationSpec>(()=>{try{return readRelationSpec(JSON.parse(readPreference(storageKey)??"null"))??empty}catch{return empty}});
 const [opened,setOpened]=useState(false);
 const targets=layers.filter(readableRelationLayer),target=targets.find(l=>l.id===spec.target),localFields=row.layer.source.schema.fields;
 const local=localFields.find(f=>f.field_id===spec.localField),targetFields=target?.source.schema.fields.filter(f=>f.type===local?.type)??[];
 const [remote,setRemote]=useState<{pin:string;items:RelatedRecord[]}|null>(null),[remoteError,setRemoteError]=useState(""),[busy,setBusy]=useState(false);
 const source=layers.find(l=>l.id===row.layer.id),canQuery=!!source&&readableRelationLayer(source)&&!!target&&queryableRelationLayer(target);
 const pin=JSON.stringify([org,pid,row.key,row.layer.id,row.layer.source.content_revision,row.layer.source.schema_revision,spec,target?.source_id,target?.source.content_revision,target?.source.schema_revision,canQuery,opened,restricted,disabled]);
 const latest=useRef(pin),generation=useRef(0);latest.current=pin;
 useEffect(()=>{generation.current++;setRemote(null);setRemoteError("");setBusy(false);return()=>{generation.current++}},[pin]);
 const patch=(p:Partial<RelationSpec>)=>{setSpec(v=>({...v,...p}))};
 const query=async()=>{if(!target||!canQuery||!opened||restricted||disabled||busy)return;const ticket=++generation.current,key=pin;setBusy(true);setRemoteError("");setRemote(null);const current=()=>latest.current===key&&generation.current===ticket;
  try{const csrf=await loadCsrfToken(client);if(!current())return;const items=await queryRelation(row,target,spec,body=>executeLayerQuery(client,org,pid,target.id,body,csrf),current);if(current())setRemote({pin:key,items})}catch(e){if(current())setRemoteError(e instanceof Error?e.message:String(e))}finally{if(current())setBusy(false)}
 };
 const openRemote=async(id:string)=>{if(!target||!canQuery||!opened||!remote||remote.pin!==pin||busy||restricted||disabled)return;const ticket=++generation.current,key=pin;setBusy(true);const current=()=>latest.current===key&&generation.current===ticket;try{await navigateRemote(target,id,current)}catch(e){if(current())setRemoteError(e instanceof Error?e.message:String(e))}finally{if(current())setBusy(false)}};
 let result:StudioRow[]|null=null,error="";
 if(opened&&!remote&&spec.target&&spec.localField&&spec.targetField){try{result=relationMatches(row,layers,rows,spec,restricted)}catch(e){error=e instanceof Error?e.message:String(e)}}
 return <details open={opened} onToggle={e=>setOpened(e.currentTarget.open)}><summary>Liên kết thuộc tính</summary>
 <p>Tra cứu khóa chính xác cục bộ hoặc bằng truy vấn máy chủ (kể cả đối tượng chưa tải). Cấu hình cá nhân theo dự án và lớp nguồn; chỉ đọc, không tạo ràng buộc cơ sở dữ liệu.</p>
 <fieldset disabled={disabled||restricted||busy}><label>Trường nguồn<select value={spec.localField} onChange={e=>patch({localField:e.target.value,targetField:""})}><option value="">Chọn trường</option>{localFields.map(f=><option key={f.field_id} value={f.field_id}>{f.name}</option>)}</select></label>
 <label>Lớp đích<select value={spec.target} onChange={e=>patch({target:e.target.value,targetField:""})}><option value="">Chọn lớp</option>{targets.map(l=><option key={l.id} value={l.id}>{l.name}</option>)}</select></label>
 <label>Trường đích<select value={spec.targetField} onChange={e=>patch({targetField:e.target.value})}><option value="">Chọn trường cùng kiểu</option>{targetFields.map(f=><option key={f.field_id} value={f.field_id}>{f.name}</option>)}</select></label>
 <label>Cardinality<select value={spec.cardinality} onChange={e=>patch({cardinality:e.target.value==="one"?"one":"many"})}><option value="many">Một–nhiều</option><option value="one">Nhiều–một (kiểm tra khóa đang chọn)</option></select></label>
 <button type="button" disabled={!readRelationSpec(spec)||!target||!local||!targetFields.some(f=>f.field_id===spec.targetField)} onClick={()=>writePreference(storageKey,spec)}>Lưu cấu hình cá nhân</button><button type="button" onClick={()=>{setSpec(empty);writePreference(storageKey,null)}}>Xóa cấu hình</button><button type="button" disabled={!canQuery||!readRelationSpec(spec)||!target||!local||!targetFields.some(f=>f.field_id===spec.targetField)} onClick={()=>void query()}>Tra cứu toàn nguồn qua máy chủ</button></fieldset>
 {busy&&<p role="status">Đang truy vấn…</p>}{remoteError&&<p role="alert">{remoteError}</p>}{opened&&canQuery&&!restricted&&remote?.pin===pin&&<><p>{remote.items.length} kết quả máy chủ</p>{remote.items.map(r=><button key={r.feature_id} disabled={busy||disabled||restricted} onClick={()=>void openRemote(r.feature_id)}>{target?.name} · {r.feature_id}</button>)}</>}{error&&!remote&&<p role="status">{error}</p>}{result&&!remote&&<><p>{result.length} đối tượng khớp</p>{result.map(r=><button key={r.key} type="button" disabled={disabled||restricted} onClick={()=>navigate(r.key)}>{r.layer.name} · {r.feature.feature_id}</button>)}</>}
 </details>
}
