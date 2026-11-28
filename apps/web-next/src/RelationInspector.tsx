import {useState} from "react";
import type {SpatialLayer} from "@openvietgis/client-sdk";
import type {StudioRow} from "./studio-contract";
import {readRelationSpec,readableRelationLayer,relationMatches,type RelationSpec} from "./relation-inspector";
import {readPreference,writePreference} from "./spatial-resume";
const empty:RelationSpec={target:"",localField:"",targetField:"",cardinality:"many"};
export function RelationInspector({row,layers,rows,storageKey,restricted,navigate,disabled}:{row:StudioRow;layers:SpatialLayer[];rows:StudioRow[];storageKey:string;restricted:boolean;navigate:(key:string)=>void;disabled:boolean}){
 const [spec,setSpec]=useState<RelationSpec>(()=>{try{return readRelationSpec(JSON.parse(readPreference(storageKey)??"null"))??empty}catch{return empty}});
 const [opened,setOpened]=useState(false);
 const targets=layers.filter(readableRelationLayer),target=targets.find(l=>l.id===spec.target),localFields=row.layer.source.schema.fields;
 const local=localFields.find(f=>f.field_id===spec.localField),targetFields=target?.source.schema.fields.filter(f=>f.type===local?.type)??[];
 const patch=(p:Partial<RelationSpec>)=>{setSpec(v=>({...v,...p}))};
 let result:StudioRow[]|null=null,error="";
 if(opened&&spec.target&&spec.localField&&spec.targetField){try{result=relationMatches(row,layers,rows,spec,restricted)}catch(e){error=e instanceof Error?e.message:String(e)}}
 return <details open={opened} onToggle={e=>setOpened(e.currentTarget.open)}><summary>Liên kết thuộc tính</summary>
 <p>Tra cứu khóa chính xác trên lớp đã tải đủ. Cấu hình cá nhân theo dự án và lớp nguồn; chỉ đọc, không tạo ràng buộc cơ sở dữ liệu.</p>
 <fieldset disabled={disabled||restricted}><label>Trường nguồn<select value={spec.localField} onChange={e=>patch({localField:e.target.value,targetField:""})}><option value="">Chọn trường</option>{localFields.map(f=><option key={f.field_id} value={f.field_id}>{f.name}</option>)}</select></label>
 <label>Lớp đích<select value={spec.target} onChange={e=>patch({target:e.target.value,targetField:""})}><option value="">Chọn lớp</option>{targets.map(l=><option key={l.id} value={l.id}>{l.name}</option>)}</select></label>
 <label>Trường đích<select value={spec.targetField} onChange={e=>patch({targetField:e.target.value})}><option value="">Chọn trường cùng kiểu</option>{targetFields.map(f=><option key={f.field_id} value={f.field_id}>{f.name}</option>)}</select></label>
 <label>Cardinality<select value={spec.cardinality} onChange={e=>patch({cardinality:e.target.value==="one"?"one":"many"})}><option value="many">Một–nhiều</option><option value="one">Nhiều–một (kiểm tra khóa đang chọn)</option></select></label>
 <button type="button" disabled={!result} onClick={()=>writePreference(storageKey,spec)}>Lưu cấu hình cá nhân</button><button type="button" onClick={()=>{setSpec(empty);writePreference(storageKey,null)}}>Xóa cấu hình</button></fieldset>
 {error&&<p role="alert">{error}</p>}{result&&<><p>{result.length} đối tượng khớp</p>{result.map(r=><button key={r.key} type="button" disabled={disabled} onClick={()=>navigate(r.key)}>{r.layer.name} · {r.feature.feature_id}</button>)}</>}
 </details>
}
