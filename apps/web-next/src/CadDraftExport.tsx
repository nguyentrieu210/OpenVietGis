import {useEffect,useRef,useState} from "react";
import {ApiClient,ApiError,loadCsrfToken} from "@openvietgis/client-sdk";
import type {CadFileVersion} from "./CadEditorPanel";
import type {CadLink} from "./TileFeaturePanel";

const digestPattern=/^[a-f0-9]{64}$/;
const idPattern=/^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/i;
type DraftLink=CadLink&{conversion_id:string;content_revision:string;schema_revision:string};
export function supportsCadDraft(link:CadLink):link is DraftLink{
 return typeof link.conversion_id==="string"&&idPattern.test(link.conversion_id)&&
  typeof link.content_revision==="string"&&digestPattern.test(link.content_revision)&&
  typeof link.schema_revision==="string"&&digestPattern.test(link.schema_revision);
}
type Profile="derived-local-dxf-v1"|"native-xy-dxf-v1";
type Preview={profile?:Profile;changed_handles?:string[];document_id:string;version:number;document_sha256:string;conversion_id:string;source_id:string;layer_id:string;content_revision:string;schema_revision:string;format:string;units:string;feature_count:number;published:boolean;loss_report:Record<string,unknown>;preview_fingerprint:string};
export function CadDraftExport({client,path,document,link,expire}:{client:ApiClient;path:string;document:CadFileVersion;link:DraftLink;expire:()=>void}){
 const [profile,setProfile]=useState<Profile>("derived-local-dxf-v1");
 const [preview,setPreview]=useState<Preview|null>(null),[ack,setAck]=useState(false),[busy,setBusy]=useState(false),[message,setMessage]=useState("");
 const pending=useRef(false),active=useRef(true),abort=useRef(new AbortController());
 useEffect(()=>{active.current=true;abort.current=new AbortController();return()=>{active.current=false;abort.current.abort()}},[]);
 const endpoint=path+"/"+encodeURIComponent(document.id)+"/versions/"+document.version+"/export-georef-draft";
 const body={profile,conversion_id:link.conversion_id,expected_document_sha256:document.sha256,expected_content_revision:link.content_revision,expected_schema_revision:link.schema_revision};
 const valid=(p:Preview)=>p&&p.document_id===document.id&&p.version===document.version&&p.document_sha256===document.sha256&&p.conversion_id===link.conversion_id&&p.source_id===link.source_id&&p.layer_id===link.layer_id&&p.content_revision===link.content_revision&&p.schema_revision===link.schema_revision&&p.published===false&&p.format==="dxf"&&["millimeter","centimeter","meter"].includes(p.units)&&Number.isInteger(p.feature_count)&&p.feature_count>0&&p.feature_count<=10000&&digestPattern.test(p.preview_fingerprint)&&p.loss_report?.original_document_unchanged===true&&p.loss_report?.requires_acknowledgement===true&&p.loss_report?.attributes_omitted===true&&((profile==="derived-local-dxf-v1"&&(p.profile===undefined||p.profile===profile)&&p.loss_report?.native_handles_not_preserved===true&&p.loss_report?.native_styles_layers_curves_blocks_text_and_unlinked_entities_not_preserved===true)||(profile==="native-xy-dxf-v1"&&p.profile===profile&&p.loss_report?.native_handles_not_preserved===false&&p.loss_report?.qualified_entity_properties_preserved===true&&p.loss_report?.other_native_metadata_not_certified===true&&p.loss_report?.geometry_only===true&&Array.isArray(p.changed_handles)&&p.changed_handles.length<=1000&&p.changed_handles.every(h=>typeof h==="string"&&/^[0-9A-F]{1,32}$/.test(h))))&&p.loss_report?.all_current_source_features_included===true&&p.loss_report?.survey_grade===false;
 const task=async(fn:()=>Promise<void>)=>{
  if(pending.current||!active.current)return;pending.current=true;setBusy(true);setMessage("");
  try{await loadCsrfToken(client);if(active.current)await fn()}
  catch(e){if(active.current){setPreview(null);setAck(false);if(e instanceof ApiError&&e.status===401)expire();else setMessage(e instanceof ApiError&&e.status===409?"Bản vẽ hoặc lớp GIS đã thay đổi. Chọn lại đối tượng để lấy phiên bản mới.":"Không thể tạo bản DXF nháp hoặc không xác minh được dữ liệu.")}}
  finally{pending.current=false;if(active.current)setBusy(false)}
 };
 const inspect=()=>{setPreview(null);setAck(false);void task(async()=>{const r=await client.request<Preview>(endpoint,{method:"POST",body:JSON.stringify(body),cache:"no-store",signal:abort.current.signal});if(!valid(r))throw Error("Invalid derived DXF preview");if(active.current)setPreview(r)})};
 const download=()=>{if(!preview||!ack||!valid(preview))return;const approved=preview;void task(async()=>{
  const r=await client.requestRaw(endpoint,{method:"POST",body:JSON.stringify({...body,expected_preview_fingerprint:approved.preview_fingerprint,acknowledge_loss:true}),cache:"no-store",signal:abort.current.signal});
  const bytes=await r.arrayBuffer(),sha=Array.from(new Uint8Array(await crypto.subtle.digest("SHA-256",bytes)),b=>b.toString(16).padStart(2,"0")).join("");
  if(!bytes.byteLength||bytes.byteLength>25*1024*1024||r.headers.get("Content-Type")?.split(";")[0]!=="application/dxf"||sha!==r.headers.get("X-Content-SHA256")||r.headers.get("X-CAD-Preview-Fingerprint")!==approved.preview_fingerprint)throw Error("DXF integrity mismatch");
  if(!active.current)return;
  const url=URL.createObjectURL(new Blob([bytes],{type:"application/dxf"}));const a=window.document.createElement("a");a.href=url;a.download="GIS-nhap-"+document.id+"-v"+document.version+".dxf";a.click();setTimeout(()=>URL.revokeObjectURL(url),1000);setMessage("Đã tạo DXF nháp. Bản gốc không thay đổi.");setPreview(null);setAck(false)
 })};
 return <section aria-label="GIS sang DXF nháp"><label>Phương án DXF <select disabled={busy} value={profile} onChange={e=>{setProfile(e.target.value as Profile);setPreview(null);setAck(false);setMessage("")}}><option value="derived-local-dxf-v1">Tạo DXF mới</option><option value="native-xy-dxf-v1">Giữ Handle gốc (DXF XY đơn giản)</option></select></label><button disabled={busy} onClick={inspect}>GIS → DXF nháp</button>{preview&&<fieldset disabled={busy}><p>Xuất toàn bộ {preview.feature_count} đối tượng lớp {link.layer_name}, theo đơn vị {preview.units} của bản vẽ gốc.</p>{profile==="native-xy-dxf-v1"?<small>Cập nhật {preview.changed_handles?.length??0} Handle, kiểm tra giữ thuộc tính entity hiện có. Chỉ DXF XY đơn giản, đầy đủ liên kết một-một; không hỗ trợ thêm/xóa, đường cong, Block hoặc Text. Không đồng bộ thuộc tính GIS; chưa chứng nhận toàn bộ metadata CAD. Bản gốc không bị ghi đè.</small>:<small>Chỉ hình học XY. Không giữ thuộc tính, Handle, kiểu nét, lớp gốc, đường cong, Block, Text hoặc đối tượng CAD ngoài lớp GIS. Không phải dữ liệu trắc địa và không ghi đè bản gốc.</small>}<label><input type="checkbox" checked={ack} onChange={e=>setAck(e.target.checked)}/> Tôi hiểu giới hạn và đồng ý tải bản nháp riêng.</label><button disabled={!ack} onClick={download}>Tải DXF nháp</button></fieldset>}{(busy||message)&&<small role="status">{busy?"Đang kiểm tra…":message}</small>}</section>;
}
