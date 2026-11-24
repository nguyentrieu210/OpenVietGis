import { useEffect, useRef, useState } from "react";
import { ApiClient, ApiError, loadCsrfToken, type Changeset } from "@openvietgis/client-sdk";
import type { CadFileVersion } from "./CadEditorPanel";
import type { CadLink } from "./TileFeaturePanel";

type Operation = {
  op_type: "update";
  feature_id: string;
  geometry: { type: "Point" | "LineString" | "Polygon"; coordinates: unknown[] };
};
type Preview = {
  conversion_id: string;
  source_id: string;
  candidate_sha256: string;
  candidate_version: number;
  base_content_revision: string;
  base_schema_revision: string;
  operations: Operation[];
  published: false;
};
const digestPattern = /^[a-f0-9]{64}$/;

export function CadDraftImport({ client, path, document, link, expire }: {
  client: ApiClient;
  path: string;
  document: CadFileVersion;
  link: CadLink;
  expire: () => void;
}) {
  const [draft, setDraft] = useState<{ preview: Preview; operationId: string } | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  const [applied, setApplied] = useState(false);
  const pending = useRef(false);
  const abort = useRef(new AbortController());
  useEffect(() => {
    const controller = new AbortController();
    abort.current = controller;
    return () => controller.abort();
  }, []);

  async function request(action: (signal: AbortSignal) => Promise<void>, applying = false) {
    if (pending.current || abort.current.signal.aborted) return;
    const { signal } = abort.current;
    pending.current = true;
    setBusy(true);
    setMessage("");
    try {
      await loadCsrfToken(client);
      if (!signal.aborted) await action(signal);
    } catch (error) {
      if (signal.aborted) return;
      if (error instanceof ApiError && error.status === 401) {
        setDraft(null);
        expire();
      } else if (error instanceof ApiError && [403, 404, 409, 422].includes(error.status)) {
        setDraft(null);
        setMessage(error.status === 409
          ? "Phiên bản CAD hoặc dữ liệu GIS đã thay đổi. Hãy mở lại bản vẽ và kiểm tra liên kết."
          : "Không thể xử lý đề xuất. Hãy kiểm tra quyền, phiên bản DXF và liên kết GIS.");
      } else {
        setMessage(applying
          ? "Chưa xác nhận được kết quả. Bấm áp dụng lại để kiểm tra cùng mã thao tác."
          : "Không tải được đề xuất CAD → GIS.");
      }
    } finally {
      pending.current = false;
      if (!signal.aborted) setBusy(false);
    }
  }

  function inspect() {
    setDraft(null);
    void request(async signal => {
      const endpoint = path + "/" + encodeURIComponent(document.id)
        + "/versions/" + link.version + "/preview-native-import";
      const preview = await client.request<Preview>(endpoint, {
        method: "POST",
        body: JSON.stringify({
          conversion_id: link.conversion_id,
          expected_candidate_sha256: document.sha256,
        }),
        cache: "no-store",
        signal,
      });
      if (preview.conversion_id !== link.conversion_id || preview.source_id !== link.source_id
        || preview.candidate_sha256 !== document.sha256 || preview.candidate_version !== document.version
        || preview.base_content_revision !== link.content_revision
        || preview.base_schema_revision !== link.schema_revision || preview.published !== false
        || !Array.isArray(preview.operations) || preview.operations.length > 1000
        || !preview.operations.every(op => op?.op_type === "update" && typeof op.feature_id === "string"
          && ["Point", "LineString", "Polygon"].includes(op.geometry?.type)
          && Array.isArray(op.geometry.coordinates))) {
        throw new Error("Invalid CAD preview");
      }
      if (signal.aborted) return;
      if (!preview.operations.length) {
        setMessage("Không có thay đổi hình học cần áp dụng.");
        return;
      }
      setDraft({ preview, operationId: crypto.randomUUID() });
    });
  }

  function apply() {
    if (!draft || !path.endsWith("/cad/documents")) return;
    const { preview, operationId } = draft;
    void request(async signal => {
      const projectPath = path.slice(0, -"/cad/documents".length);
      const receipt = await client.request<Changeset>(
        projectPath + "/sources/" + encodeURIComponent(link.source_id) + "/changesets",
        {
          method: "POST",
          signal,
          body: JSON.stringify({
            operation_id: operationId,
            summary: "CAD → GIS · DXF v" + document.version + " · " + document.sha256,
            base_content_revision: preview.base_content_revision,
            base_schema_revision: preview.base_schema_revision,
            operations: preview.operations.map(({ op_type, feature_id, geometry }) => ({
              op_type, feature_id, geometry,
            })),
          }),
        },
      );
      if (receipt.operation_id !== operationId || receipt.source_id !== link.source_id
        || receipt.base_content_revision !== preview.base_content_revision
        || receipt.operation_count !== preview.operations.length
        || !digestPattern.test(receipt.resulting_content_revision)) {
        throw new Error("Invalid ChangeSet receipt");
      }
      if (signal.aborted) return;
      setApplied(true);
      setDraft(null);
      setMessage("Đã áp dụng ChangeSet " + receipt.id + ".");
    }, true);
  }

  return (
    <section aria-label="CAD sang GIS">
      {!draft && !applied && <button disabled={busy} onClick={inspect}>Xem trước CAD → GIS</button>}
      {draft && (
        <fieldset disabled={busy}>
          <legend>Duyệt {draft.preview.operations.length} thay đổi hình học · {link.layer_name}</legend>
          <details>
            <summary>Xem đối tượng và tọa độ đề xuất</summary>
            <ol>
              {draft.preview.operations.map(op => (
                <li key={op.feature_id}>
                  <strong>{op.feature_id}</strong>
                  <pre>{JSON.stringify(op.geometry)}</pre>
                </li>
              ))}
            </ol>
          </details>
          <button onClick={apply}>Áp dụng vào GIS</button>
        </fieldset>
      )}
      {(busy || message) && <small role="status">{busy ? "Đang xử lý…" : message}</small>}
    </section>
  );
}
