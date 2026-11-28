import {useEffect,useRef,useState} from "react";
import {executeLayerQuery,getSpatialLayer,loadCsrfToken,type ApiClient,type SpatialLayer} from "@openvietgis/client-sdk";
import {relationRules,relationLayerPin,relationOptions,type ProjectRelation} from "./relation-values";
import {queryableRelationLayer} from "./relation-inspector";
import {tableQueryBody,verifyTableQuery} from "./table-query";

export function useRelationRules(client:ApiClient,org:string,pid:string,source:string|undefined,enabled:boolean){
 const key=JSON.stringify([org,pid,source,enabled]);
 const [state,setState]=useState<{key:string;rules:ProjectRelation[];error:string}|null>(null);
 useEffect(()=>{
  if(!enabled||!source)return;
  let live=true;
  const path="/organizations/"+encodeURIComponent(org)+"/projects/"+encodeURIComponent(pid)+"/editing/relations";
  client.request<unknown>(path).then(r=>{if(live)setState({key,rules:relationRules(r,org,pid,source),error:""})}).catch(e=>{if(live)setState({key,rules:[],error:e instanceof Error?e.message:String(e)})});
  return()=>{live=false};
 },[client,key,org,pid,source,enabled]);
 return enabled&&source&&state?.key===key?state:{key,rules:[],error:""};
}

type Props={client:ApiClient;org:string;pid:string;source:SpatialLayer;layers:SpatialLayer[];field:string;rules:ProjectRelation[];disabled:boolean;choose:(value:string)=>void};
export function RelationValuePicker(props:Props){
 return <>{props.rules.filter(r=>r.source_id===props.source.source_id&&r.local_field===props.field).map(rule=><RulePicker key={rule.id} {...props} rule={rule}/>)}</>;
}
function RulePicker({client,org,pid,source,layers,rule,disabled,choose}:Props&{rule:ProjectRelation}){
 const target=layers.find(l=>l.source_id===rule.target_source_id&&queryableRelationLayer(l));
 const field=source.source.schema.fields.find(f=>f.field_id===rule.local_field);
 const compatible=!!target&&field?.type===rule.field_type&&target.source.schema.fields.some(f=>f.field_id===rule.target_field&&f.type===field.type);
 const [open,setOpen]=useState(false),[exact,setExact]=useState(false),[search,setSearch]=useState(""),[busy,setBusy]=useState(false),[error,setError]=useState("");
 const [result,setResult]=useState<{pin:string;options:ReturnType<typeof relationOptions>;total:number;loaded:number}|null>(null);
 const pin=JSON.stringify([org,pid,rule,relationLayerPin(source),target&&relationLayerPin(target),open,disabled,exact,search]);
 const latest=useRef(pin),generation=useRef(0);latest.current=pin;
 useEffect(()=>{generation.current++;setResult(null);setError("");setBusy(false);return()=>{generation.current++}},[pin]);
 async function load(){
  if(!target||!compatible||!open||disabled||busy)return;
  const ticket=++generation.current,current=()=>latest.current===pin&&generation.current===ticket;
  setBusy(true);setError("");setResult(null);
  try{
   const fresh=await getSpatialLayer(client,org,pid,target.id);
   if(!current())return;
   if(relationLayerPin(fresh)!==relationLayerPin(target)||!queryableRelationLayer(fresh))throw Error("Lớp đích hoặc quyền đã thay đổi; tải lại dự án.");
   const csrf=await loadCsrfToken(client);if(!current())return;
   const body=tableQueryBody(fresh,{field:exact?rule.target_field:"",op:"eq",value:search,negate:false},"","asc",0);
   const page=verifyTableQuery(fresh,await executeLayerQuery(client,org,pid,fresh.id,body,csrf),0);
   if(!current())return;
   setResult({pin,options:relationOptions(rule,fresh,page.items),total:page.total_count,loaded:page.items.length});
  }catch(e){if(current())setError(e instanceof Error?e.message:String(e))}
  finally{if(current())setBusy(false)}
 }
 const visible=result?.pin===pin?result:null;
 return <details open={open} onToggle={e=>setOpen(e.currentTarget.open)} className="relation-value-picker">
  <summary>Chọn khóa từ {target?.name??"lớp liên kết"}</summary>
  {!compatible?<p role="status">Không có lớp đích có quyền truy vấn hoặc schema đã đổi. Kiểm tra lại quan hệ.</p>:<>
  <small>Ràng buộc ghi: {rule.local_field} → {target!.name}.{rule.target_field}. Chọn khóa không tự lưu changeset.</small>
  <label><input type="checkbox" checked={exact} disabled={disabled||busy} onChange={e=>setExact(e.target.checked)}/> Tìm chính xác khóa</label>
  {exact&&<label>Giá trị khóa<input value={search} disabled={disabled||busy} onChange={e=>setSearch(e.target.value)}/></label>}
  <button type="button" disabled={disabled||busy} onClick={()=>void load()}>{busy?"Đang đọc…":"Tải khóa liên kết"}</button>
  {error&&<p role="alert">{error}</p>}
  {visible&&<><p>{visible.loaded}/{visible.total} bản ghi; {visible.options.length} khóa khác NULL trong kết quả.</p>
   {visible.total>visible.loaded&&<small>Chỉ hiển thị trang đầu. Dùng tìm chính xác để tra khóa ngoài trang này.</small>}
   <label>Khóa đích<select aria-label={"Khóa đích "+rule.local_field+" "+target!.name} disabled={disabled||busy||!visible.options.length} value="" onChange={e=>{const i=Number(e.target.value);if(e.target.value!==""&&latest.current===pin&&!disabled&&Number.isInteger(i)&&visible.options[i])choose(visible.options[i]!.value)}}>
    <option value="">Chọn khóa…</option>{visible.options.map((o,i)=><option key={o.value} value={i}>{o.value===""?"(chuỗi rỗng)":o.value} · {o.first}{o.count>1?" · "+o.count+" bản ghi":""}</option>)}
   </select></label></>}
  </>}
 </details>
}
