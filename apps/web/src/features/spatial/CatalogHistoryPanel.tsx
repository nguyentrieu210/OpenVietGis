import {
  listSpatialCatalogChanges,
  type SpatialCatalogChange
} from "@openvietgis/client-sdk";
import { useQuery } from "@tanstack/react-query";
import { useState } from "react";

import { apiClient } from "../../app/api";
import { currentProtectedData, protectedQueryOptions } from "../../app/protected-query";
import {
  createHistoryTraversal, formatHistoryValue, HISTORY_PAGE_SIZE, historyKey,
  historyResponseMatchesScope, nextHistoryPage, previousHistoryPage, revisionEntries,
  type HistoryScope
} from "./catalog-history";

function catalogOperationLabel(operation: string): string {
  const labels: Record<string, string> = {
    register_source: "Đăng ký nguồn",
    import_source: "Nhập nguồn dữ liệu",
    update_source: "Cập nhật nguồn",
    correct_source_crs: "Sửa CRS nguồn",
    retire_source: "Ngừng sử dụng nguồn",
    create_layer: "Tạo lớp",
    rename_layer: "Đổi tên lớp",
    retire_layer: "Ngừng sử dụng lớp",
    refresh_reference_fixture: "Làm mới nguồn tham chiếu"
  };
  return labels[operation] ?? operation;
}

function resourceTypeLabel(type: string): string {
  const labels: Record<string, string> = {
    source: "Nguồn dữ liệu",
    layer: "Lớp dữ liệu"
  };
  return labels[type] ?? type;
}

function actorTypeLabel(type: string): string {
  const labels: Record<string, string> = {
    user: "Người dùng",
    service: "Dịch vụ",
    system: "Hệ thống"
  };
  return labels[type] ?? type;
}

function RevisionDescription({ change, prior }: { change: SpatialCatalogChange; prior: boolean }) {
  const revision = prior ? change.prior_revision : change.new_revision;
  if (prior && revision === null) return <p>Không có phiên bản trước (bản ghi được tạo mới).</p>;
  const entries = revisionEntries(change.resource_type, revision);
  if (!entries.length) return <p>Không có dữ liệu phiên bản.</p>;
  return (
    <dl style={{ overflowWrap: "anywhere" }}>
      {entries.map(([label, value]) => (
        <div key={label}><dt>{label}</dt><dd><code>{value}</code></dd></div>
      ))}
    </dl>
  );
}

function ChangeEvidence({ change }: { change: SpatialCatalogChange }) {
  return (
    <article aria-label={catalogOperationLabel(change.operation)} style={{ borderTop: "1px solid #ccc", paddingTop: 12 }}>
      <h3>{catalogOperationLabel(change.operation)}</h3>
      <dl style={{ overflowWrap: "anywhere" }}>
        <dt>Tài nguyên</dt><dd>{resourceTypeLabel(change.resource_type)} · <code>{change.resource_id}</code></dd>
        <dt>Người thực hiện</dt><dd>{change.actor_display_snapshot} ({actorTypeLabel(change.actor_type)}) · <code>{change.actor_id}</code></dd>
        <dt>Thời điểm ghi nhận (UTC)</dt><dd><time dateTime={change.created_at}>{change.created_at}</time></dd>
        <dt>Mã thay đổi</dt><dd><code>{change.id}</code></dd>
        <dt>Mã sự kiện kiểm toán</dt><dd><code>{change.audit_event_id}</code></dd>
      </dl>
      <h4>Phiên bản trước</h4><RevisionDescription change={change} prior />
      <h4>Phiên bản mới</h4><RevisionDescription change={change} prior={false} />
      <h4>Các trường đã thay đổi</h4>
      <div style={{ overflowX: "auto" }}>
        <table>
          <thead><tr><th>Trường</th><th>Giá trị cũ</th><th>Giá trị mới</th></tr></thead>
          <tbody>{change.changes.map((value, index) => (
            <tr key={value.field + ":" + index}>
              <th scope="row">{value.field}</th>
              <td><code style={{ overflowWrap: "anywhere" }}>{formatHistoryValue(value.old_value)}</code></td>
              <td><code style={{ overflowWrap: "anywhere" }}>{formatHistoryValue(value.new_value)}</code></td>
            </tr>
          ))}</tbody>
        </table>
      </div>
    </article>
  );
}

/** Mounted only behind the current account and exact active/archived Project gate. */
export function CatalogHistoryPanel({ scope }: { scope: HistoryScope }) {
  const [traversal, setTraversal] = useState(createHistoryTraversal);
  const query = useQuery({
    queryKey: historyKey(scope, traversal.cursor),
    queryFn: () => listSpatialCatalogChanges(apiClient, scope.organizationId, scope.projectId, {
      limit: HISTORY_PAGE_SIZE,
      ...(traversal.cursor === null ? {} : { after_id: traversal.cursor })
    }),
    ...protectedQueryOptions
  });
  const current = currentProtectedData(query);
  const mismatch = current !== undefined && !historyResponseMatchesScope(scope, current, traversal.cursor);
  const page = mismatch ? undefined : current;
  return (
    <section aria-label="Lịch sử thay đổi dữ liệu không gian" style={{ borderTop: "1px solid #aaa", marginTop: 32, paddingTop: 16 }}>
      <h2>Lịch sử thay đổi</h2>
      <p>Các thay đổi nguồn và lớp đã được ghi nhận, bao gồm bằng chứng ngừng sử dụng. Thứ tự theo mã thay đổi, không theo thời gian. Giá trị giữ nguyên kiểu JSON; null khác chuỗi rỗng. Mỗi trang được đọc theo quyền hiện tại.</p>
      {!page ? <>
        <p role={query.error || mismatch ? "alert" : "status"}>
          {mismatch ? "Lịch sử thay đổi không khớp với phạm vi dự án hoặc trang tiếp theo hiện tại."
            : query.error ? "Không thể tải quyền truy cập lịch sử thay đổi hiện tại." : "Đang kiểm tra quyền truy cập lịch sử thay đổi…"}
        </p>
        {query.error || mismatch ? <>
          <button type="button" onClick={() => { void query.refetch(); }}>Tải lại lịch sử</button>{" "}
          <button type="button" disabled={!traversal.history.length}
            onClick={() => setTraversal(previousHistoryPage(traversal))}>Lịch sử trước</button>{" "}
          <button type="button" disabled={traversal.cursor === null}
            onClick={() => setTraversal(createHistoryTraversal())}>Tải lại từ trang đầu</button>
        </> : null}
      </> : <>
        {page.items.length === 0 ? <p>Không có thay đổi nào được ghi nhận trên trang này.</p> :
          page.items.map(change => <ChangeEvidence key={change.id} change={change} />)}
        <p role="status">Trang lịch sử {traversal.history.length + 1} — {page.items.length} thay đổi.</p>
        <button type="button" disabled={!traversal.history.length}
          onClick={() => setTraversal(previousHistoryPage(traversal))}>Lịch sử trước</button>{" "}
        <button type="button" disabled={page.next_after_id === null}
          onClick={() => setTraversal(nextHistoryPage(traversal, page.next_after_id))}>Lịch sử sau</button>{" "}
        <button type="button" onClick={() => {
          if (traversal.cursor === null) void query.refetch();
          else setTraversal(createHistoryTraversal());
        }}>Tải lại từ trang đầu</button>
      </>}
    </section>
  );
}
