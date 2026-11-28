import {featureAction} from "./feature-diagram";
import {formGroups,formStorageKey,readFormLayout} from "./attribute-form-layout";
import {useMemo,useState} from "react";
import type {StudioRow} from "./studio-contract";
import {derive,fmtArea,fmtLength} from "./geom-measure";
import "./feature-info.css";
const names:Record<string,string>={Point:"Điểm",MultiPoint:"Tập điểm",LineString:"Đường",MultiLineString:"Tập đường",Polygon:"Vùng",MultiPolygon:"Tập vùng",GeometryCollection:"Tập hình học"};
const text=(v:unknown)=>v==null||v===""?"—":typeof v==="object"?JSON.stringify(v):String(v);
const fold=(v:string)=>v.toLocaleLowerCase("vi").normalize("NFD").replace(/\p{Diacritic}/gu,"").replace(/đ/g,"d");
export function FeatureInfo({row,formScope=""}:{row:StudioRow;formScope?:string}){
 const [query,setQuery]=useState(""),[hideEmpty,setHideEmpty]=useState(false),[actionStatus,setActionStatus]=useState("");
 const copy=async(kind:"id"|"attributes")=>{try{if(!navigator.clipboard)throw Error("Trình duyệt không cho phép clipboard.");await navigator.clipboard.writeText(featureAction(kind,row.feature.feature_id,row.feature.attributes));setActionStatus("Đã sao chép.")}catch(e){setActionStatus(e instanceof Error?e.message:String(e))}};
 const metrics=useMemo(()=>row.geometry?derive(row.geometry):null,[row.geometry]);
 const schema=row.layer.source.schema.fields,layout=readFormLayout(formStorageKey(formScope,row.layer),schema);
 const fields=formGroups(layout,schema).flatMap(g=>g.items.map(({field,layout})=>({key:field.field_id,name:layout.label.trim()||field.name,value:row.feature.attributes[field.field_id],group:g.name})));
 const visible=fields.filter(f=>(!hideEmpty||f.value!==null&&f.value!==undefined&&f.value!=="")&&fold(f.name+" "+text(f.value)).includes(fold(query)));
 return <section className="feature-info" aria-label="Thông tin đối tượng">
  <div className="fi-heading"><b>Hình học</b><small>WGS 84 · EPSG:4326</small></div>
  <dl className="fi-metrics"><div><dt>Loại</dt><dd>{row.geometry?names[row.geometry.type]??row.geometry.type:"Không có hình học"}</dd></div>{metrics&&<><div><dt>Số đỉnh</dt><dd>{metrics.vertices.toLocaleString("vi-VN")}</dd></div>{metrics.area!==undefined&&<div><dt>Diện tích ước tính</dt><dd>{fmtArea(metrics.area)}</dd></div>}{metrics.perimeter!==undefined&&<div><dt>Chu vi ước tính</dt><dd>{fmtLength(metrics.perimeter)}</dd></div>}{metrics.length!==undefined&&<div><dt>Chiều dài ước tính</dt><dd>{fmtLength(metrics.length)}</dd></div>}</>}</dl>
  <div className="fi-heading"><b>Thuộc tính</b><small>{visible.length}/{fields.length} trường</small></div>
  <div className="fi-filter"><input value={query} onChange={e=>setQuery(e.target.value)} placeholder="Tìm trường hoặc giá trị…" aria-label="Tìm thuộc tính"/><label><input type="checkbox" checked={hideEmpty} onChange={e=>setHideEmpty(e.target.checked)}/> Ẩn trống</label></div>
  {[...new Set(visible.map(f=>f.group))].map(group=><section key={group}>{group&&<h4>{group}</h4>}<dl className="fi-fields">{visible.filter(f=>f.group===group).map(f=><div key={f.key}><dt title={f.key}>{f.name}</dt><dd title={text(f.value)}>{text(f.value)}</dd></div>)}</dl></section>)}
  {!visible.length&&<p>Không có thuộc tính phù hợp.</p>}
  <details><summary>Thao tác đối tượng</summary><p>Chỉ thao tác trên dữ liệu đang được phép đọc; không chạy lệnh, script hoặc gửi dữ liệu ra ngoài.</p><button type="button" onClick={()=>void copy("id")}>Sao chép FID</button> <button type="button" onClick={()=>void copy("attributes")}>Sao chép thuộc tính JSON</button>{actionStatus&&<p role="status">{actionStatus}</p>}</details>
 </section>
}
