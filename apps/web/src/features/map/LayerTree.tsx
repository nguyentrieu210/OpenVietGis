import { useState } from "react";
import { Link } from "react-router-dom";
import type { MapLayerReference,SpatialLayer } from "@openvietgis/client-sdk";
import { capability,defaultStyle,previewMove } from "./workspace";
export function LayerTree({layers,metadata,active,catalog,nextCatalog,previousCatalog,change,choose,locked,states,importPath,catalogPath}:{layers:MapLayerReference[];metadata:(SpatialLayer|undefined)[];active:string|null;catalog:SpatialLayer[]|undefined;nextCatalog:(()=>void)|null;previousCatalog:(()=>void)|null;change:(layers:MapLayerReference[])=>void;choose:(id:string)=>void;locked:boolean;states:string[];importPath:string;catalogPath:string}) {
 const [add,setAdd]=useState("");
 const available=catalog?.filter(l=>!layers.some(r=>r.layer_id===l.id))??[];
 return <aside className="map-layers" aria-label="Danh sách lớp bản đồ">
  <h2>Danh sách lớp <span className="map-badge">{layers.length}/32</span></h2>
  <label>Thêm lớp từ danh mục <select data-testid="add-map-layer" aria-label="Thêm lớp từ danh mục" value={add} onChange={e=>setAdd(e.target.value)} disabled={locked||!catalog||available.length===0}><option value="">{available.length?"Chọn lớp dữ liệu…":"Không còn lớp chưa thêm"}</option>{available.map(l=><option key={l.id} value={l.id}>{l.name}{capability(l,"render")?"":" (không hỗ trợ vẽ)"}</option>)}</select></label>
  <button type="button" disabled={locked||!add||layers.length>=32} onClick={()=>{if(add){change([...layers,{layer_id:add,visible:true,style:{...defaultStyle}}]);choose(add);setAdd("");}}}>+ Thêm lớp có sẵn</button>
  <div className="map-layer-create-actions">
   <Link to={importPath}>+ Nhập dữ liệu / tạo lớp mới</Link>
   <Link to={catalogPath}>Tạo lớp từ nguồn đã có</Link>
  </div>
  {catalog&&available.length===0?<p className="map-muted">Tất cả lớp hiện có trong danh mục đã được đưa lên bản đồ. Muốn có lớp mới, hãy nhập thêm dữ liệu hoặc tạo lớp mới từ một nguồn dữ liệu.</p>:null}
  <div className="map-catalog-pages"><button disabled={!previousCatalog} onClick={()=>previousCatalog?.()}>Trang trước</button><button disabled={!nextCatalog} onClick={()=>nextCatalog?.()}>Trang sau</button></div>
  {!catalog?<p role="status">Đang tải danh mục lớp…</p>:null}
  {!layers.length?<p className="map-empty">Chọn một lớp bên trên để hiển thị trên bản đồ, hoặc nhập thêm dữ liệu tại mục Nhập / Xuất.</p>:null}
  <ol>{layers.map((ref,index)=>{const layer=metadata[index],name=layer?.name??"Lớp không khả dụng";return <li key={ref.layer_id} data-testid={"map-layer-"+ref.layer_id} className={active===ref.layer_id?"is-active":""}>
   <div className="map-layer-title"><input type="checkbox" aria-label={"Show "+name} checked={ref.visible} disabled={locked} onChange={()=>change(layers.map(l=>l.layer_id===ref.layer_id?{...l,visible:!l.visible}:l))}/><button type="button" aria-label={"Make "+name+" active"} aria-pressed={active===ref.layer_id} onClick={()=>choose(ref.layer_id)}>{name}</button></div>
   <p data-testid={"viewport-state-"+ref.layer_id}>{states[index]??"loading"}</p>
   {layer?<p className="map-muted">{layer.source.schema.geometry_family} · Hệ tọa độ gốc EPSG:{layer.source.stored_crs?.code??"unknown"}<br/><code>{layer.id}</code></p>:null}
   <div className="map-layer-tools"><button type="button" disabled={locked||index===0} aria-label={"Đưa "+name+" lên trên"} onClick={()=>change(previewMove(layers,ref.layer_id,-1))}>↑</button><button type="button" disabled={locked||index===layers.length-1} aria-label={"Đưa "+name+" xuống dưới"} onClick={()=>change(previewMove(layers,ref.layer_id,1))}>↓</button><button type="button" disabled={locked} aria-label={"Gỡ "+name+" khỏi bản đồ"} onClick={()=>change(layers.filter(l=>l.layer_id!==ref.layer_id))}>Gỡ lớp</button></div>
   <details><summary>Tùy chỉnh hiển thị</summary><fieldset disabled={locked||!capability(layer,"style")}><label>Màu sắc <input type="color" aria-label={"Color "+name} data-testid={"color-"+ref.layer_id} value={ref.style.color} onChange={e=>change(layers.map(l=>l.layer_id===ref.layer_id?{...l,style:{...l.style,color:e.target.value}}:l))}/></label>{(["opacity","width","radius"] as const).map(key=><label key={key}>{key}<input type="range" aria-label={key+" "+name} min={key==="opacity"?0:key==="width"?1:2} max={key==="opacity"?1:key==="width"?12:20} step={key==="opacity"?.1:1} value={ref.style[key]} onChange={e=>change(layers.map(l=>l.layer_id===ref.layer_id?{...l,style:{...l.style,[key]:Number(e.target.value)}}:l))}/><output>{ref.style[key]}</output></label>)}</fieldset></details>
  </li>;})}</ol>
  <p className="map-muted">Xem trước hiển thị cục bộ. Dữ liệu gốc không bị thay đổi.</p>
 </aside>;
}
