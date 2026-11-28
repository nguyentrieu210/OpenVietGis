import {
  importHouseNumberingEvidence,
  listHouseNumberingReconciliation,
  rebuildHouseNumberingReconciliation,
  resolveHouseNumberingCase,
  type HouseNumberingReconcileCase
} from "./api";
import { useState } from "react";
import { hnClient, uiError, withCsrf } from "./client";
import { useReloadable } from "./hooks";

const caseLabels:Record<HouseNumberingReconcileCase["case_class"],string>={matched:"Khớp",number_mismatch:"Sai khác số nhà",cad_certificate_mismatch:"Mâu thuẫn chứng nhận / bản vẽ",new_building:"Công trình mới",no_longer_exists:"Không còn tồn tại",not_surveyed:"Chưa khảo sát",certificate_unmatched:"Chứng nhận chưa ghép"};
const statusLabels:Record<HouseNumberingReconcileCase["status"],string>={open:"Chờ xử lý",resolved:"Đã xử lý",ignored:"Bỏ qua có lý do"};
type Props = { organizationId: string; projectId: string };

export function ReconcilePanel({ organizationId, projectId }: Props) {
  const cases = useReloadable(() => listHouseNumberingReconciliation(hnClient, organizationId, projectId), [organizationId, projectId]);
  const [file, setFile] = useState<File | null>(null);
  const [origin, setOrigin] = useState("certificate");
  const [mappingText, setMappingText] = useState('{"building_id":"building_id","route_id":"route_id","raw_number":"raw_number","source_reference":"source_reference"}');
  const [sourceRef, setSourceRef] = useState("Nguồn nhập liệu");
  const [notice, setNotice] = useState<string | null>(null);
  const [reason,setReason]=useState("");
  const [busy,setBusy]=useState(false);
  const [statusFilter,setStatusFilter]=useState<"all"|HouseNumberingReconcileCase["status"]>("open");
  const [error, setError] = useState<string | null>(null);

  async function mutation(action: () => Promise<void>) {
    setError(null); setNotice(null);setBusy(true);
    try { await withCsrf(action); } catch (err) { setError(await uiError(err)); } finally{setBusy(false)}
  }
  async function resolve(item: HouseNumberingReconcileCase, status: "resolved" | "ignored") {
    if(!reason.trim())return;
    await mutation(async () => {
      await resolveHouseNumberingCase(hnClient, organizationId, projectId, item.id, {
        expected_revision: item.revision, status, resolution: reason.trim()
      });
      setReason("");await cases.reload();
    });
  }

  return <>
    <section className="hn-card">
      <h2>Nhập nguồn chứng cứ</h2>
      <div className="hn-form">
        <label>File<input type="file" onChange={(e) => setFile(e.target.files?.[0] ?? null)} /></label>
        <label>Nguồn<select value={origin} onChange={(e) => setOrigin(e.target.value)}><option value="certificate">Giấy chứng nhận</option><option value="cad">CAD</option><option value="survey">Khảo sát</option><option value="legacy">Dữ liệu cũ</option></select></label>
        <label>Tham chiếu nguồn<input value={sourceRef} onChange={(e) => setSourceRef(e.target.value)} /></label>
        <label>Mapping JSON<textarea value={mappingText} onChange={(e) => setMappingText(e.target.value)} /></label>
        <button className="hn-btn" type="button" disabled={!file||busy} onClick={() => void mutation(async () => {
          if (!file) return;
          const ext = file.name.toLowerCase().split(".").pop();
          const format = ext === "xlsx" ? "xlsx" : ext === "geojson" || ext === "json" ? "geojson" : ext === "dxf" ? "dxf" : ext === "zip" ? "shapefile_zip" : "csv";
          const mapping = JSON.parse(mappingText) as Record<string, string>;
          const result = await importHouseNumberingEvidence(hnClient, organizationId, projectId, { file, source_format: format, origin, mapping, default_source_reference: sourceRef });
          const batch = result.batch as { read_count?: number; accepted_count?: number; rejected_count?: number };
          setNotice(`Đã đọc ${batch.read_count ?? 0}, nhận ${batch.accepted_count ?? 0}, loại ${batch.rejected_count ?? 0}${result.replayed ? " · replay không nhân đôi" : ""}.`);
        })}>Nhập & chuẩn hóa</button>
      </div>
    </section>
    <section className="hn-card">
      <div className="hn-actions"><h2 style={{marginRight:"auto"}}>Hàng đợi đối chiếu</h2><button className="hn-btn" type="button" disabled={busy} onClick={() => void mutation(async () => { await rebuildHouseNumberingReconciliation(hnClient, organizationId, projectId, {}); await cases.reload(); setNotice("Đã cập nhật hàng đợi; kết quả cũ được giữ."); })}>Cập nhật đối soát</button></div>
      <label>Trạng thái <select value={statusFilter} onChange={e=>setStatusFilter(e.target.value as typeof statusFilter)}><option value="open">Chờ xử lý</option><option value="resolved">Đã xử lý</option><option value="ignored">Bỏ qua</option><option value="all">Tất cả lịch sử</option></select></label>
      <label>Ghi rõ kết quả đối chiếu / căn cứ xử lý<input value={reason} disabled={busy} maxLength={1000} onChange={e=>setReason(e.target.value)}/></label>
      <p className="hn-muted">Không tự chọn số mới nhất nếu các nguồn có mâu thuẫn. Các nhóm bên dưới là hồ sơ đối soát, có thể gồm nhiều phiên lịch sử trên một công trình.</p>
      <table className="hn-table"><thead><tr><th>Nhóm</th><th>Công trình</th><th>Ước tính</th><th>Trạng thái</th><th>Xử lý</th></tr></thead><tbody>{(cases.data?.items ?? []).filter(i=>statusFilter==="all"||i.status===statusFilter).map((item) => <tr key={item.id}><td>{caseLabels[item.case_class]}</td><td>{item.building_id?.slice(0,8) ?? "Chưa ghép"}</td><td>{item.estimated ? "Có" : "Không"}</td><td>{statusLabels[item.status]}{item.resolution&&<p>{item.resolution}</p>}</td><td>{item.status === "open" && <div className="hn-actions"><button className="hn-btn" type="button" disabled={busy||!reason.trim()} onClick={() => void resolve(item,"resolved")}>Đã xử lý</button><button className="hn-btn secondary" type="button" disabled={busy||!reason.trim()} onClick={() => void resolve(item,"ignored")}>Bỏ qua</button></div>}</td></tr>)}</tbody></table>
      {notice && <div className="hn-ok">{notice}</div>}{error && <div className="hn-error">{error}</div>}
    </section>
  </>;
}
