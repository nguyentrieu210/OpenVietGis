import { listSpatialCrsProfiles } from "@openvietgis/client-sdk";
import { useQuery } from "@tanstack/react-query";
import { apiClient } from "../../app/api";
import { currentProtectedData, protectedQueryOptions } from "../../app/protected-query";
import { crsRegistryKey, crsRegistryMatchesScope, type CrsScope } from "./crs-profiles";

export function CrsRegistryPanel({ scope }: { scope: CrsScope }) {
  const query = useQuery({ queryKey: crsRegistryKey(scope),
    queryFn: () => listSpatialCrsProfiles(apiClient, scope.organizationId, scope.projectId),
    ...protectedQueryOptions });
  const registry = currentProtectedData(query);
  if (!registry) return <p role={query.error ? "alert" : "status"}>{query.error
    ? "Không thể xác minh quyền truy cập cấu hình CRS hiện tại." : "Đang kiểm tra cấu hình CRS hiện tại…"}</p>;
  if (!crsRegistryMatchesScope(scope, registry)) return <p role="alert">Các cấu hình CRS không khớp với dự án hiện tại.</p>;
  return <section aria-label="Hỗ trợ hệ tọa độ (CRS)">
    <h2>Hỗ trợ hệ tọa độ (CRS)</h2>
    <p>Tọa độ trong ứng dụng sử dụng thứ tự X/Y. Trục và đơn vị gốc theo EPSG được hiển thị riêng.</p>
    <details><summary>Định nghĩa CRS và phép chuyển đổi được phê duyệt</summary>
      <div style={{ overflowX: "auto" }}><table aria-label="Định nghĩa CRS"><thead><tr>
        <th>CRS</th><th>Trục gốc</th><th>X/Y trong ứng dụng</th><th>Đơn vị</th><th>Mức hỗ trợ</th><th>Khu vực áp dụng</th>
      </tr></thead><tbody>{registry.profiles.map(profile => <tr key={profile.code}>
        <th>EPSG:{profile.code} — {profile.name}</th><td>{profile.native_axes.join(", ")}</td>
        <td>{profile.x_axis}, {profile.y_axis}</td><td>{profile.unit}</td>
        <td>{profile.disposition === "definition_only" ? "Chỉ có định nghĩa" : "Có thể đọc"}</td><td>{profile.area_of_use}</td>
      </tr>)}</tbody></table></div>
      <ul aria-label="Các phép chuyển đổi trực tiếp đã phê duyệt">{registry.transformations.map(edge => <li key={edge.operation_id}>
        EPSG:{edge.source_code} → EPSG:{edge.target_code}: {edge.method}; dung sai {edge.tolerance} {edge.tolerance_unit}.
        {" "}{edge.validated_area} Bằng chứng: {edge.evidence_id}.
      </li>)}</ul>
    </details>
  </section>;
}
