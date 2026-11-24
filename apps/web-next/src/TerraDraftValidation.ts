/** Optional local preflight. The server remains the authoritative geometry/ACL gate. */
import {ValidateNotSelfIntersecting} from "terra-draw";
import type {Geometry} from "geojson";

export function validateTerraDraft(geometry:Geometry):void {
  if(geometry.type!=="LineString"&&geometry.type!=="Polygon")return;
  if(geometry.type==="Polygon"&&geometry.coordinates.length!==1)
    throw Error("Engine thử nghiệm chỉ hỗ trợ vùng một vòng. Dùng EDIT cho vùng có lỗ.");
  // Do not modify coordinates, attributes, CRS, feature ID or the operation.
  // Terra Draw validates the preview only. PostGIS must still validate on commit.
  const result=ValidateNotSelfIntersecting({type:"Feature",geometry,properties:{}});
  if(!result.valid)throw Error("Hình học tự giao cắt · hãy hủy và vẽ lại.");
}
