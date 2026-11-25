import {
  addReviewComment, createChangeReview, decideChangeReview, getChangeset, getEffectivePermissions,
  getProject, listChangeReviews, listChangesets, listProjectActivities, listReviewComments,
  listSpatialLayers, loadCsrfToken
} from "@openvietgis/client-sdk";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useRef, useState } from "react";
import { apiClient } from "../../app/api";
import { currentProtectedData, protectedQueryOptions } from "../../app/protected-query";
import { FreshAccount } from "../auth/FreshAccount";

const statusLabels = { pending: "Đang chờ", approved: "Đã phê duyệt", changes_requested: "Yêu cầu chỉnh sửa" };

function CollaborationWorkbench({ organizationId, projectId, accountId }: {
  organizationId: string; projectId: string; accountId: string;
}) {
  const qc = useQueryClient();
  const prefix = ["r011", accountId, organizationId, projectId];
  const [sourceId, setSourceId] = useState("");
  const [changesetId, setChangesetId] = useState("");
  const [assignee, setAssignee] = useState("");
  const [reviewId, setReviewId] = useState("");
  const [note, setNote] = useState("");
  const [comment, setComment] = useState("");
  const [anchor, setAnchor] = useState("");
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  const [activityCursor, setActivityCursor] = useState<string | undefined>();
  const pendingComment = useRef<{ intent: string; operationId: string } | null>(null);
  const inFlight = useRef(false);
  const projectQ = useQuery({
    queryKey: [...prefix, "project"], queryFn: () => getProject(apiClient, organizationId, projectId),
    ...protectedQueryOptions
  });
  const permissionsQ = useQuery({
    queryKey: [...prefix, "permissions"],
    queryFn: () => getEffectivePermissions(apiClient, organizationId, { project_id: projectId }),
    ...protectedQueryOptions
  });
  const project = currentProtectedData(projectQ);
  const permissions = currentProtectedData(permissionsQ);
  const readable = project?.id === projectId && project.organization_id === organizationId &&
    permissions?.permissions.includes("project.read") === true;
  const writable = readable && project.status === "active" &&
    permissions?.permissions.includes("project.edit") === true;
  const reviewsQ = useQuery({
    queryKey: [...prefix, "reviews"], queryFn: () => listChangeReviews(apiClient, organizationId, projectId),
    enabled: readable, ...protectedQueryOptions
  });
  const actsQ = useQuery({
    queryKey: [...prefix, "activity", activityCursor],
    queryFn: () => listProjectActivities(apiClient, organizationId, projectId,
      { limit: 50, ...(activityCursor ? { cursor: activityCursor } : {}) }),
    enabled: readable, ...protectedQueryOptions
  });
  const layersQ = useQuery({
    queryKey: [...prefix, "layers"],
    queryFn: () => listSpatialLayers(apiClient, organizationId, projectId, { limit: 100 }),
    enabled: writable, ...protectedQueryOptions
  });
  const layers = writable ? (currentProtectedData(layersQ)?.items ?? []).filter((layer, i, all) =>
    layer.organization_id === organizationId && layer.project_id === projectId &&
    all.findIndex(other => other.source_id === layer.source_id) === i) : [];
  const selectedSource = layers.find(layer => layer.source_id === sourceId);
  const historyQ = useQuery({
    queryKey: [...prefix, "history", sourceId],
    queryFn: () => listChangesets(apiClient, organizationId, projectId, sourceId),
    enabled: Boolean(writable && selectedSource), ...protectedQueryOptions
  });
  const history = selectedSource ? currentProtectedData(historyQ)?.items ?? [] : [];
  const reviews = readable ? currentProtectedData(reviewsQ)?.items ?? [] : [];
  const review = reviews.find(item => item.id === reviewId);
  const detailQ = useQuery({
    queryKey: [...prefix, "review-detail", review?.changeset_id],
    queryFn: () => getChangeset(apiClient, organizationId, projectId, review!.changeset_id),
    enabled: Boolean(review), ...protectedQueryOptions
  });
  const commentsQ = useQuery({
    queryKey: [...prefix, "comments", review?.id],
    queryFn: () => listReviewComments(apiClient, organizationId, projectId, review!.id),
    enabled: Boolean(review), ...protectedQueryOptions
  });
  const detail = review ? currentProtectedData(detailQ) : undefined;
  const matchingDetail = detail && detail.source_id === review?.source_id &&
    detail.resulting_content_revision === review.content_revision ? detail : undefined;
  const comments = review ? currentProtectedData(commentsQ)?.items ?? [] : [];
  const uuid = /^[0-9a-f-]{36}$/i;
  async function act(fn: (csrf: string) => Promise<unknown>) {
    if (inFlight.current) return;
    inFlight.current = true;
    setBusy(true);
    setMessage("");
    try {
      await fn(await loadCsrfToken(apiClient));
      setActivityCursor(undefined);
      setMessage("Đã ghi nhận thành công.");
    } catch (error) {
      setMessage(error instanceof Error ? error.message : "Không thể ghi nhận yêu cầu.");
    } finally {
      await qc.invalidateQueries({ queryKey: prefix });
      setBusy(false);
      inFlight.current = false;
    }
  }

  if (!readable) return <section className="ovg-card"><h2>Cộng tác nhóm</h2>
    <p role="status">Đang xác minh quyền hoặc không có quyền truy cập dự án.</p></section>;
  return <section className="ovg-card" data-testid="collaboration-workbench">
    <h2>Cộng tác nhóm & Duyệt thay đổi sau ghi nhận</h2>
    <p>Phê duyệt một bộ thay đổi đã ghi nhận; không thay thế quyền chỉnh sửa hoặc sửa trực tiếp dữ liệu.</p>
    {writable && <fieldset disabled={busy}>
      <legend>Tạo yêu cầu duyệt</legend>
      <label>Nguồn dữ liệu <select value={sourceId} onChange={e => {
        setSourceId(e.target.value); setChangesetId("");
      }}><option value="">Chọn nguồn</option>{layers.map(layer =>
        <option key={layer.source_id} value={layer.source_id}>{layer.source.name}</option>
      )}</select></label>
      <label>Bộ thay đổi <select value={changesetId} onChange={e => setChangesetId(e.target.value)}>
        <option value="">Chọn bộ thay đổi</option>{history.map(item =>
          <option key={item.id} value={item.id}>{item.summary} — {item.resulting_content_revision.slice(0, 12)}</option>
        )}</select></label>
      <label>Mã người duyệt (UUID)<input value={assignee} onChange={e => setAssignee(e.target.value)} /></label>
      <button type="button" onClick={() => setAssignee(accountId)}>Chọn tài khoản của tôi</button>
      <button type="button" disabled={!history.some(item => item.id === changesetId) || !uuid.test(assignee)}
        onClick={() => void act(async csrf => {
          const created = await createChangeReview(apiClient, organizationId, projectId,
            { changeset_id: changesetId, assignee_user_id: assignee }, csrf);
          setReviewId(created.id);
        })}>Gửi yêu cầu duyệt</button>
    </fieldset>}
    <h3>Yêu cầu duyệt ({reviews.length})</h3>
    {reviewsQ.isError ? <p role="alert">Không thể tải danh sách yêu cầu.</p> :
      <ul>{reviews.map(item => <li key={item.id}>
        <button type="button" onClick={() => { setReviewId(item.id); setNote(""); setComment(""); setAnchor(""); }}>
          {item.changeset_id.slice(0, 8)} — {statusLabels[item.status]}
        </button></li>)}</ul>}
    {review && <section aria-label="Chi tiết yêu cầu duyệt">
      <p>Phiên bản: <code>{review.content_revision}</code></p>
      <p>Người duyệt: {review.assignee_user_id}; trạng thái: {statusLabels[review.status]}</p>
      {review.decision_note && <p>Ý kiến quyết định: {review.decision_note}</p>}
      {matchingDetail && <ul>{matchingDetail.operations.map(op =>
        <li key={op.id}>{op.op_type}: {op.feature_id}</li>
      )}</ul>}
      <fieldset disabled={busy || !matchingDetail || !writable}>
        <legend>Bình luận</legend>
        <ul>{comments.map(item => <li key={item.id}>{item.author_display}: {item.body}
          {item.feature_id && <> — {item.feature_id}</>}</li>)}</ul>
        {commentsQ.isError && <p role="alert">Không thể tải bình luận.</p>}
        <label>Đối tượng đã thay đổi <select value={anchor} onChange={e => setAnchor(e.target.value)}>
          <option value="">Toàn bộ bộ thay đổi</option>{matchingDetail?.operations.map(op =>
            <option key={op.id} value={op.feature_id}>{op.feature_id}</option>
          )}</select></label>
        <label>Nội dung bình luận<textarea maxLength={2000} value={comment} onChange={e => setComment(e.target.value)} /></label>
        <button type="button" disabled={!comment.trim() || Boolean(anchor && !matchingDetail?.operations.some(op => op.feature_id === anchor))}
          onClick={() => void act(async csrf => {
            const intent = JSON.stringify([review.id, comment.trim(), anchor || null]);
            if (pendingComment.current?.intent !== intent) {
              pendingComment.current = { intent, operationId: crypto.randomUUID() };
            }
            await addReviewComment(apiClient, organizationId, projectId, review.id,
              { body: comment, feature_id: anchor || null, operation_id: pendingComment.current.operationId }, csrf);
            pendingComment.current = null;
            setComment("");
          })}>Gửi bình luận</button>
      </fieldset>
      {writable && review.assignee_user_id === accountId && review.status === "pending" && matchingDetail &&
        <fieldset disabled={busy}><legend>Quyết định người được giao duyệt</legend>
          <label>Ý kiến<textarea maxLength={500} value={note} onChange={e => setNote(e.target.value)} /></label>
          <button type="button" onClick={() => void act(csrf => decideChangeReview(apiClient, organizationId, projectId,
            review.id, { status: "approved", decision_note: note }, csrf))}>Phê duyệt</button>
          <button type="button" onClick={() => void act(csrf => decideChangeReview(apiClient, organizationId, projectId,
            review.id, { status: "changes_requested", decision_note: note }, csrf))}>Yêu cầu chỉnh sửa</button>
        </fieldset>}
    </section>}
    {message && <p role="status">{message}</p>}
    <h3>Dòng hoạt động dự án</h3>
    {actsQ.isError && <p role="alert">Không thể tải hoạt động. Làm mới để mở danh sách hiện hành.</p>}
    {currentProtectedData(actsQ) && <>
      <p>Mốc ghi nhận: {currentProtectedData(actsQ)?.cutoff}</p>
      <ul>{(currentProtectedData(actsQ)?.items ?? []).map(item =>
        <li key={item.event_kind + ":" + item.id}>
          {item.actor_display}: {item.summary} — {item.event_kind} · {item.created_at}
          {item.revision_ref && <> · <code>{item.revision_ref}</code></>}
        </li>)}</ul>
    </>}
    <button type="button" disabled={!currentProtectedData(actsQ)?.next_cursor} onClick={() => {
      setActivityCursor(currentProtectedData(actsQ)?.next_cursor ?? undefined);
    }}>Trang hoạt động tiếp theo</button>
    <button type="button" onClick={() => {
      setActivityCursor(undefined);
      void qc.invalidateQueries({ queryKey: [...prefix, "activity"] });
    }}>Làm mới hoạt động</button>
  </section>;
}

export function CollaborationWorkbenchPanel({ organizationId, projectId }: {
  organizationId: string; projectId: string;
}) {
  return <FreshAccount key={organizationId + ":" + projectId}>{account => <CollaborationWorkbench
    key={account.id + ":" + organizationId + ":" + projectId}
    accountId={account.id} organizationId={organizationId} projectId={projectId}
  />}</FreshAccount>;
}
