import {
  ApiError,
  type CadDxfInput,
  type CadDxfInspection,
  importInspectedDxf,
  inspectDxf,
  loadCsrfToken
} from "@openvietgis/client-sdk";
import { useState } from "react";
import { apiClient } from "../../app/api";

interface Props {
  organizationId: string;
  projectId: string;
  onImported: () => void;
}

const errors: Record<string, string> = {
  CAD_UNIT_MISMATCH: "Đơn vị trong DXF không khớp đơn vị đã chọn.",
  CAD_CURVE_TOLERANCE_REQUIRED: "Đường cong cần dung sai xấp xỉ rõ ràng.",
  CAD_BULGE_UNSUPPORTED: "Polyline cong cần nhập dung sai xấp xỉ.",
  CAD_CURVE_LIMIT: "Không đạt dung sai trong giới hạn số điểm hoặc độ chính xác số.",
  CAD_3D_UNSUPPORTED: "DXF có cao độ hoặc chiều dày; chỉ hỗ trợ 2D.",
  CAD_WIDTH_UNSUPPORTED: "Polyline có bề rộng, chưa hỗ trợ.",
  CAD_OCS_UNSUPPORTED: "Hệ tọa độ vật thể không được hỗ trợ.",
  CAD_INSPECTION_STALE: "Tệp hoặc thông số đã thay đổi. Hãy kiểm tra lại.",
  CAD_LOSS_ACK_REQUIRED: "Cần xác nhận báo cáo mất mát hoặc xấp xỉ trước khi nhập.",
  INVALID_DXF: "DXF không hợp lệ hoặc có hình học không hỗ trợ.",
  INVALID_GEOMETRY: "Hình học không hợp lệ; không thể xuất bản.",
  CAD_VERSION_UNSUPPORTED: "Chỉ hỗ trợ DXF R2010, R2013 và R2018.",
  CAD_FORMAT_UNSUPPORTED: "Chỉ hỗ trợ DXF 2D, không hỗ trợ DWG."
};

async function cadFailure(error: unknown): Promise<string> {
  if (error instanceof ApiError) {
    if (error.status === 403 || error.status === 404) return "Bạn không có quyền nhập CAD trong dự án này.";
    try {
      const body = (await error.response.clone().json()) as { code?: string };
      const message = body.code ? errors[body.code] : undefined;
      if (message) return message;
    } catch {
      // Fall back to generic error; do not display arbitrary server content.
    }
  }
  return "Không hoàn thành thao tác CAD. Kiểm tra tệp, thông số và quyền truy cập.";
}

function Counts({ label, values }: { label: string; values: Record<string, number> }) {
  return <p>{label}: {Object.entries(values).map(([kind, count]) => `${kind}: ${count}`).join(", ") || "Không có"}</p>;
}

export function CadImportPanel({ organizationId, projectId, onImported }: Props) {
  const [file, setFile] = useState<File | null>(null);
  const [units, setUnits] = useState<CadDxfInput["units"] | "">("");
  const [crsCode, setCrsCode] = useState<CadDxfInput["crsCode"] | "">("");
  const [tolerance, setTolerance] = useState("");
  const [name, setName] = useState("DXF");
  const [confirmCrs, setConfirmCrs] = useState(false);
  const [ack, setAck] = useState(false);
  const [inspection, setInspection] = useState<CadDxfInspection | null>(null);
  const [pending, setPending] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  const [success, setSuccess] = useState<string | null>(null);
  const tol = tolerance.trim() === "" ? undefined : Number(tolerance);
  const input: CadDxfInput | null =
    file && file.size <= 25 * 1024 * 1024 && file.name.toLowerCase().endsWith(".dxf") &&
    units !== "" && crsCode !== "" && confirmCrs &&
    (tol === undefined || (Number.isFinite(tol) && tol >= 0.000001 && tol <= 1000))
      ? { file, units, crsCode, ...(tol === undefined ? {} : { curveToleranceM: tol }) }
      : null;

  function resetInspection() {
    setInspection(null);
    setAck(false);
    setSuccess(null);
    setFailure(null);
  }

  async function check() {
    if (!input) return;
    resetInspection();
    setPending(true);
    try {
      const csrf = await loadCsrfToken(apiClient);
      const result = await inspectDxf(apiClient, organizationId, projectId, input, csrf);
      if (result.published !== false || !result.geometry_valid || result.loss_report.survey_grade !== false) {
        setFailure("Máy chủ chưa xác nhận được báo cáo kiểm tra.");
        return;
      }
      setInspection(result);
    } catch (error) {
      setFailure(await cadFailure(error));
    } finally {
      setPending(false);
    }
  }

  async function publish() {
    if (!input || !inspection || (inspection.loss_report.requires_acknowledgement && !ack)) return;
    setPending(true);
    setFailure(null);
    try {
      const csrf = await loadCsrfToken(apiClient);
      const result = await importInspectedDxf(
        apiClient, organizationId, projectId, input, inspection, name, ack, csrf
      );
      if (result.status !== "succeeded" || !result.layer_id) {
        setFailure("Máy chủ chưa xác nhận xuất bản lớp CAD.");
        return;
      }
      setInspection(null);
      setAck(false);
      setSuccess(`Đã tạo lớp dữ liệu mới: ${result.filename} (EPSG:${result.crs_code}).`);
      onImported();
    } catch (error) {
      setInspection(null);
      setAck(false);
      setFailure(await cadFailure(error));
    } finally {
      setPending(false);
    }
  }

  return (
    <section aria-label="Kiểm tra và nhập DXF">
      <h3>Nhập DXF 2D · kiểm tra trước khi xuất bản</h3>
      <fieldset disabled={pending}>
        <legend>Thông số bản vẽ</legend>
        <div className="ovg-form-row">
          <label>Tệp DXF (tối đa 25 MiB)
            <input type="file" accept=".dxf" onChange={(e) => {
              setFile(e.target.files?.[0] ?? null); resetInspection();
            }} />
          </label>
          <label>Đơn vị bản vẽ
            <select value={units} onChange={(e) => {
              setUnits(e.target.value as typeof units); setConfirmCrs(false); resetInspection();
            }}>
              <option value="">Chọn đơn vị đã xác minh</option>
              <option value="meter">Mét</option>
              <option value="centimeter">Centimét</option>
              <option value="millimeter">Milimét</option>
            </select>
          </label>
          <label>CRS mặt phẳng đã xác minh
            <select value={crsCode} onChange={(e) => {
              setCrsCode(e.target.value === "" ? "" : Number(e.target.value) as 3857 | 5899);
              setConfirmCrs(false); resetInspection();
            }}>
              <option value="">Chọn CRS đã biết</option>
              <option value="3857">EPSG:3857</option>
              <option value="5899">EPSG:5899</option>
            </select>
          </label>
          <label>Dung sai đường cong (mét lưới, tùy chọn)
            <input type="number" step="any" min="0.000001" max="1000" value={tolerance} onChange={(e) => {
              setTolerance(e.target.value); resetInspection();
            }} />
          </label>
          <label>Tên lớp dữ liệu mới
            <input value={name} maxLength={200} onChange={(e) => setName(e.target.value)} />
          </label>
        </div>
        <label>
          <input type="checkbox" checked={confirmCrs} onChange={(e) => {
            setConfirmCrs(e.target.checked); resetInspection();
          }} />
          Tôi đã xác minh tọa độ XY của bản vẽ dùng đúng CRS đã chọn sau quy đổi đơn vị; không cần dịch chuyển hay xoay.
        </label>
        <p><button type="button" className="ovg-btn" disabled={!input || pending} onClick={check}>Kiểm tra bản vẽ</button></p>
      </fieldset>
      {pending && <p role="status">Đang xử lý DXF...</p>}
      {failure && <p role="alert">{failure}</p>}
      {success && <p role="status">{success}</p>}
      {inspection && <section aria-label="Báo cáo kiểm tra DXF">
        <h4>Báo cáo kiểm tra · chưa xuất bản</h4>
        <p>EPSG:{inspection.entity_summary.crs_code} · {inspection.entity_summary.published_feature_count} đối tượng thuộc họ {inspection.entity_summary.published_family}. Đây là phép xấp xỉ theo mét lưới, không bảo đảm độ chính xác khảo sát.</p>
        <Counts label="Loại đối tượng" values={inspection.entity_summary.entity_counts} />
        <Counts label="Lớp CAD" values={inspection.entity_summary.layer_counts} />
        <Counts label="Không hỗ trợ" values={inspection.loss_report.unsupported_entities} />
        <Counts label="Họ hình học bị loại trừ" values={inspection.loss_report.dropped_secondary_families} />
        <p>Đường cong xấp xỉ: {inspection.loss_report.curve_details.length}; sai lệch dây cung tối đa {Math.max(0, ...inspection.loss_report.curve_details.map((curve) => curve.max_chord_deviation_grid_m)).toPrecision(5)} m lưới.</p>
        {inspection.loss_report.requires_acknowledgement && <label>
          <input type="checkbox" checked={ack} disabled={pending} onChange={(e) => setAck(e.target.checked)} /> Tôi chấp nhận các phần bị loại trừ và sai lệch xấp xỉ trong báo cáo.
        </label>}
        <p><button type="button" className="ovg-btn" disabled={pending || !input || (inspection.loss_report.requires_acknowledgement && !ack)} onClick={publish}>Xác nhận nhập và tạo lớp mới</button></p>
      </section>}
    </section>
  );
}
