import { useState, type ChangeEvent } from "react";

type LocalPreview = {
  drawing: string;
  model: string;
  sourceCrs: string;
  elements: number;
  unsupported: number;
};

/** A browser-only preview. No API request or project data persistence. */
export function MicroStationBridgePanel() {
  const [preview, setPreview] = useState<LocalPreview | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function inspect(event: ChangeEvent<HTMLInputElement>) {
    const file = event.currentTarget.files?.[0];
    setPreview(null);
    setError(null);
    event.currentTarget.value = "";
    if (!file) return;
    if (file.size === 0 || file.size > 4 * 1024 * 1024) {
      setError("Tệp snapshot phải là JSON và không vượt quá 4 MiB.");
      return;
    }
    try {
      const raw: unknown = JSON.parse(await file.text());
      if (typeof raw !== "object" || raw === null || Array.isArray(raw)) {
        throw new Error("invalid envelope");
      }
      const snap = raw as Record<string, unknown>;
      if (
        snap.schema_version !== "ovg.v8i.snapshot.v1" ||
        typeof snap.drawing_id !== "string" ||
        typeof snap.model_id !== "string" ||
        typeof snap.source_crs !== "string" ||
        !/^EPSG:[1-9][0-9]{2,5}$/.test(snap.source_crs) ||
        !Array.isArray(snap.elements) ||
        snap.elements.length > 5000 ||
        !Number.isInteger(snap.unsupported_count) ||
        Number(snap.unsupported_count) < 0
      ) {
        throw new Error("invalid contract");
      }
      const ids = new Set<string>();
      for (const element of snap.elements) {
        if (typeof element !== "object" || element === null) throw new Error("element");
        const entry = element as Record<string, unknown>;
        if (
          typeof entry.element_id !== "string" ||
          !/^[A-Za-z0-9_.:-]{1,128}$/.test(entry.element_id) ||
          ids.has(entry.element_id) ||
          typeof entry.geometry !== "object" ||
          entry.geometry === null
        ) throw new Error("element");
        ids.add(entry.element_id);
      }
      setPreview({
        drawing: snap.drawing_id.slice(0, 128),
        model: snap.model_id.slice(0, 128),
        sourceCrs: snap.source_crs,
        elements: snap.elements.length,
        unsupported: Number(snap.unsupported_count)
      });
    } catch {
      // Deliberately avoid echoing snapshot contents and parser messages.
      setError("Không nhận diện được bản snapshot V8i. Hãy kiểm tra schema và chạy lệnh bridge.py validate.");
    }
  }

  return (
    <section className="ovg-card" data-testid="v8i-bridge-panel" aria-label="MicroStation V8i Bridge">
      <h2>MicroStation V8i · Windows Bridge (R025)</h2>
      <p>
        Trạng thái: <strong>Thử nghiệm cục bộ, chưa kết nối máy chủ</strong>.
        Bridge đọc phiên V8i đang mở trên Windows thông qua COM. Chưa có API đồng bộ,
        tự động biên tập hoặc ghi ngược vào DGN.
      </p>
      <p>
        Chạy <code>py bridge.py probe</code> trên máy đã cài V8i và pywin32.
        Lệnh <code>py bridge.py scan --drawing-id du-an --model-id model1 --source-crs EPSG:XXXX --output snapshot.json</code>
        tạo bản xem trước cục bộ. Cần xác minh đơn vị tọa độ và đúng mã VN-2000
        trước khi sử dụng dữ liệu cho bản đồ.
      </p>
      <label>
        Kiểm tra cấu trúc snapshot tại trình duyệt (không tải lên máy chủ)
        <input type="file" accept=".json,application/json" onChange={(event) => void inspect(event)} />
      </label>
      {error && <p role="alert">{error}</p>}
      {preview && (
        <div role="status">
          <p>
            Đã đọc cấu trúc JSON: bản vẽ <strong>{preview.drawing}</strong>,
            model <strong>{preview.model}</strong>, CRS <strong>{preview.sourceCrs}</strong>.
          </p>
          <p>
            {preview.elements} phần tử · {preview.unsupported} phần tử CAD không hỗ trợ.
            Đây chỉ là kiểm tra cấu trúc, chưa xác nhận topology, đơn vị tọa độ, thuộc tính địa chính hoặc kết nối V8i.
          </p>
        </div>
      )}
      <p>
        <a href="https://github.com/nguyentrieu210/OpenVietGis/tree/feature/025-microstation-v8i-bridge/integrations/microstation-v8i-bridge"
          target="_blank" rel="noopener noreferrer">
          Mã nguồn Bridge và hướng dẫn thử nghiệm
        </a>
      </p>
    </section>
  );
}
