import {
  houseNumberingDocumentDownloadUrl,
  issueHouseNumberingDocument,
  listHouseNumberingBuildings,
  listHouseNumberingDocuments,
  type HouseNumberingDocument
} from "./api";
import {IssuancePanel} from "./IssuancePanel";
import { useState } from "react";
import { hnClient, uiError, withCsrf } from "./client";
import { useReloadable } from "./hooks";

type Props = { organizationId: string; projectId: string };

export function DocumentsPanel({ organizationId, projectId }: Props) {
  const [showRegistry,setShowRegistry]=useState(true);
  const buildings = useReloadable(() => listHouseNumberingBuildings(hnClient, organizationId, projectId), [organizationId, projectId]);
  const [buildingId, setBuildingId] = useState("");
  const [docType, setDocType] = useState<"inspection"|"certificate"|"house_form"|"plate">("certificate");
  const [format, setFormat] = useState<"docx"|"pdf">("pdf");
  const [templateRevision, setTemplateRevision] = useState("tpl-2026.1");
  const [verified, setVerified] = useState(false);
  const [docs, setDocs] = useState<HouseNumberingDocument[]>([]);
  const [notice, setNotice] = useState<string|null>(null);
  const [error, setError] = useState<string|null>(null);

  async function loadHistory(id: string) {
    if (!id) { setDocs([]); return; }
    try { const response = await listHouseNumberingDocuments(hnClient, organizationId, projectId, id); setDocs(response.items); } catch (err) { setError(await uiError(err)); }
  }

  return <section className="hn-card">
    <h2>Hồ sơ, biểu mẫu và biển số</h2>
    <button type="button" onClick={()=>setShowRegistry(v=>!v)}>{showRegistry?"Ẩn":"Mở"} sổ chứng nhận</button>
    {showRegistry&&<IssuancePanel key={organizationId+":"+projectId} organizationId={organizationId} projectId={projectId}/>}
    <h3>Hồ sơ biểu mẫu cũ</h3>
    <div className="hn-form">
      <label>Building<select value={buildingId} onChange={(e) => { setBuildingId(e.target.value); void loadHistory(e.target.value); }}><option value="">Chọn nhà</option>{(buildings.data?.items ?? []).map((b)=><option key={b.id} value={b.id}>{b.parcel_ref ?? b.id.slice(0,8)}</option>)}</select></label>
      <label>Loại<select value={docType} onChange={(e)=>setDocType(e.target.value as typeof docType)}><option value="inspection">Phiếu kiểm tra</option><option value="certificate">Hồ sơ xác nhận</option><option value="house_form">Phiếu số nhà</option><option value="plate">Biển số nhà</option></select></label>
      <label>Định dạng<select value={format} onChange={(e)=>setFormat(e.target.value as typeof format)}><option value="pdf">PDF</option><option value="docx">DOCX</option></select></label>
      <label>Template revision<input value={templateRevision} onChange={(e)=>setTemplateRevision(e.target.value)} /></label>
      <label><input type="checkbox" checked={verified} onChange={(e)=>setVerified(e.target.checked)} /> Template đã xác minh</label>
      <button className="hn-btn" type="button" disabled={!buildingId} onClick={() => void (async()=>{setError(null);try{await withCsrf(async()=>{const result=await issueHouseNumberingDocument(hnClient,organizationId,projectId,{building_id:buildingId,doc_type:docType,output_format:format,template_revision:templateRevision,template_verified:verified});setNotice(`Đã tạo ${result.status}. Đây là kết quả tạo biểu mẫu; không thay thế chứng nhận do cơ quan có thẩm quyền cấp.`);await loadHistory(buildingId);});}catch(err){setError(await uiError(err));}})()}>Tạo hồ sơ</button>
    </div>
    <div className="hn-warning">Biểu mẫu cũ không phải sổ cấp chứng nhận. Việc ghi nhận chứng nhận đã cấp và bàn giao biển phải thực hiện trong sổ riêng phía trên.</div>
    <table className="hn-table"><thead><tr><th>Loại</th><th>Template</th><th>Trạng thái</th><th>Tải</th></tr></thead><tbody>{docs.map((doc)=><tr key={doc.id}><td>{doc.doc_type}</td><td>{doc.template_revision}</td><td>{doc.status}</td><td>{doc.artifact_id && <a className="hn-btn secondary" href={houseNumberingDocumentDownloadUrl(organizationId,projectId,doc.id)}>Tải</a>}</td></tr>)}</tbody></table>
    {notice&&<div className="hn-ok">{notice}</div>}{error&&<div className="hn-error">{error}</div>}
  </section>;
}
