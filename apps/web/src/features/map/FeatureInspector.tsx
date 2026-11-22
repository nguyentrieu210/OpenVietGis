import type { SpatialFeatureDetail,SpatialLayer } from "@openvietgis/client-sdk";
import { formatFeatureValue } from "../spatial/feature-inspector";
export function FeatureInspector({layer,detail,focus,count,error}:{layer:SpatialLayer|undefined;detail:SpatialFeatureDetail|undefined;focus:string|null;count:number;error:string|null}) {
 const activeDetail=focus&&!error&&detail?detail:undefined;
 return <aside className="map-inspector" aria-label="Chi tiết đối tượng bản đồ" data-testid="map-inspector" data-feature-id={activeDetail?.feature.feature_id}><h2>Chi tiết đối tượng</h2>
 {!focus?<p>Bấm chọn một đối tượng trên bản đồ hoặc trong bảng thuộc tính để xem chi tiết.</p>:error?<p role="alert">{error}</p>:!activeDetail?<p role="status">Đang tải chi tiết đối tượng…</p>:<div>
 <h3>Đối tượng {activeDetail.feature.feature_id}</h3><p>{count} đang chọn{count>1?" · Đang xem đối tượng nổi bật":""}</p>
 <dl><dt>Lớp dữ liệu</dt><dd>{layer?.name}</dd><dt>CRS gốc</dt><dd>EPSG:{layer?.source.source_crs?.code??"unknown"}</dd><dt>CRS lưu trữ</dt><dd>EPSG:{layer?.source.stored_crs?.code??"unknown"}</dd><dt>CRS hiển thị</dt><dd>{activeDetail.context.response_crs?"EPSG:"+activeDetail.context.response_crs.code+" · X/Y":"Attributes only"}</dd><dt>Trạng thái hình học</dt><dd>{activeDetail.feature.geometry_state}</dd><dt>Phiên bản nội dung</dt><dd><code>{activeDetail.context.content_revision}</code></dd><dt>Phiên bản lược đồ</dt><dd><code>{activeDetail.context.schema_revision}</code></dd></dl>
 <table><thead><tr><th>Trường / Kiểu</th><th>Giá trị</th></tr></thead><tbody>{layer?.source.schema.fields.map(f=><tr key={f.field_id}><th>{f.name}<small>{f.field_id} · {f.type}</small></th><td><code>{formatFeatureValue(activeDetail.feature.attributes[f.field_id])}</code></td></tr>)}</tbody></table>
 {activeDetail.context.transformation?<p className="map-muted">Phép chuyển hệ tọa độ hiển thị {activeDetail.context.transformation.operation_id}. Hình học gốc không thay đổi.</p>:null}
 </div>}</aside>;
}
