import {useMemo,useState} from "react";
import type {StudioRow} from "./studio-contract";
import {derive,fmtArea,fmtLength} from "./geom-measure";
import "./feature-info.css";
const names:Record<string,string>={Point:"Điểm",MultiPoint:"Tập điểm",LineString:"Đường",MultiLineString:"Tập đường",Polygon:"Vùng",MultiPolygon:"Tập vùng",GeometryCollection:"Tập hình học"};
const text=(v:unknown)=>v==null||v===""?"—":typeof v==="object"?JSON.stringify(v):String(v);
const fold=(v:string)=>v.toLocaleLowerCase("vi").normalize("NFD").replace(/\p{Diacritic}/gu,"").replace(/đ/g,"d");
export function FeatureInfo({row}:{row:StudioRow}){
 const [query,setQuery]=useState(""),[hideEmpty,setHideEmpty]=useState(false);
 const metrics=useMemo(()=>row.geometry?derive(row.geometry):null,[row.geometry]);
 const fields=Object.entries(row.feature.attributes).map(([key,value])=>({key,name:row.layer.source.schema?.fields.find(x=>(x.field_id||x.name)===key)?.name??key,value}));
 const visible=fields.filter(f=>(!hideEmpty||f.value!==null&&f.value!==undefined&&f.value!=="")&&fold(f.name+" "+text(f.value)).includes(fold(query)));
 return <section className="feature-info" aria-label="Thông tin đối tượng">
  <div className="fi-heading"><b>Hình học</b><small>WGS 84 · EPSG:4326</small></div>
  <dl className="fi-metrics"><div><dt>Loại</dt><dd>{row.geometry?names[row.geometry.type]??row.geometry.type:"Không có hình học"}</dd></div>{metrics&&<><div><dt>Số đỉnh</dt><dd>{metrics.vertices.toLocaleString("vi-VN")}</dd></div>{metrics.area!==undefined&&<div><dt>Diện tích ước tính</dt><dd>{fmtArea(metrics.area)}</dd></div>}{metrics.perimeter!==undefined&&<div><dt>Chu vi ước tính</dt><dd>{fmtLength(metrics.perimeter)}</dd></div>}{metrics.length!==undefined&&<div><dt>Chiều dài ước tính</dt><dd>{fmtLength(metrics.length)}</dd></div>}</>}</dl>
  <div className="fi-heading"><b>Thuộc tính</b><small>{visible.length}/{fields.length} trường</small></div>
  <div className="fi-filter"><input value={query} onChange={e=>setQuery(e.target.value)} placeholder="Tìm trường hoặc giá trị…" aria-label="Tìm thuộc tính"/><label><input type="checkbox" checked={hideEmpty} onChange={e=>setHideEmpty(e.target.checked)}/> Ẩn trống</label></div>
  <dl className="fi-fields">{visible.map(f=><div key={f.key}><dt title={f.key}>{f.name}</dt><dd title={text(f.value)}>{text(f.value)}</dd></div>)}</dl>
  {!visible.length&&<p>Không có thuộc tính phù hợp.</p>}
 </section>
}
