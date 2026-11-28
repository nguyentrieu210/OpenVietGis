import {
  getHouseNumberingProgress,
  houseNumberingProgressReportUrl,
  listHouseNumberingBuildings,
  listHouseNumberingSurveys,
  reviewHouseNumberingSurvey,
  syncHouseNumberingSurveys,
  uploadHouseNumberingSurveyPhoto
} from "./api";
import { useEffect, useRef, useState } from "react";
import {getCurrentUser} from "@openvietgis/client-sdk";
import {acknowledgedIds,addDraft,saveDraft,deleteDraft,listDrafts,scopeKey,draftKey,type SurveyDraft,type SurveyOperation} from "./survey-outbox";
import { hnClient, uiError, withCsrf } from "./client";
import { useReloadable } from "./hooks";

type Props = { organizationId: string; projectId: string };

type GpsObservation = {
  x: number;
  y: number;
  crs_code: 4326;
  gps_accuracy_m: number;
};

export function SurveyPanel({ organizationId, projectId }: Props) {
  const [actor,setActor]=useState<string|null>(null);
  const scope=actor?scopeKey(actor,organizationId,projectId):null;
  const [busy,setBusy]=useState(false);
  const running=useRef(false);
  const activeScope=useRef(scope);activeScope.current=scope;
  const legacyKey=`hn-survey-queue:${organizationId}:${projectId}`;
  const [legacyPresent,setLegacyPresent]=useState(false);
  const buildings = useReloadable(
    () => listHouseNumberingBuildings(hnClient, organizationId, projectId),
    [organizationId, projectId]
  );
  const surveys = useReloadable(
    () => listHouseNumberingSurveys(hnClient, organizationId, projectId),
    [organizationId, projectId]
  );
  const progress = useReloadable(
    () => getHouseNumberingProgress(hnClient, organizationId, projectId),
    [organizationId, projectId]
  );

  const [queue,setQueue]=useState<SurveyDraft[]>([]);
  const [buildingId, setBuildingId] = useState("");
  const [condition, setCondition] = useState("house");
  const [photo,setPhoto]=useState<File|null>(null);
  const photoInput=useRef<HTMLInputElement>(null);
  const [gps, setGps] = useState<GpsObservation | null>(null);
  const [recordedBy, setRecordedBy] = useState("");
  const [online, setOnline] = useState(navigator.onLine);
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(()=>{
    let alive=true;
    const verify=()=>{if(!navigator.onLine)return;void getCurrentUser(hnClient).then(u=>{if(alive)setActor(u.status==="active"?u.id:null)}).catch(()=>{if(alive)setActor(null)})};
    verify();window.addEventListener("online",verify);window.addEventListener("focus",verify);
    try{setLegacyPresent(JSON.parse(localStorage.getItem(legacyKey)??"[]").length>0)}catch{setLegacyPresent(true)}
    return()=>{alive=false;window.removeEventListener("online",verify);window.removeEventListener("focus",verify)};
  },[legacyKey]);
  useEffect(()=>{
    let alive=true;setQueue([]);setBuildingId("");setGps(null);setPhoto(null);setRecordedBy("");
    if(scope)void listDrafts(scope).then(rows=>{if(alive)setQueue(rows)}).catch(async e=>{if(alive)setError(await uiError(e))});
    return()=>{alive=false};
  },[scope]);
  const refreshQueue=async()=>{if(!scope)return;const rows=await listDrafts(scope);if(activeScope.current===scope)setQueue(rows)};
  const verifyActor=async()=>{const u=await getCurrentUser(hnClient);if(!actor||u.status!=="active"||u.id!==actor||activeScope.current!==scope)throw Error("Phiên đăng nhập đã thay đổi; phiếu vẫn được giữ cục bộ.")};

  useEffect(() => {
    const update = () => setOnline(navigator.onLine);
    window.addEventListener("online", update);
    window.addEventListener("offline", update);
    return () => {
      window.removeEventListener("online", update);
      window.removeEventListener("offline", update);
    };
  }, []);

  function captureGps() {
    setError(null);
    if (!navigator.geolocation) {
      setError("Thiết bị/trình duyệt không hỗ trợ GPS.");
      return;
    }
    navigator.geolocation.getCurrentPosition(
      (position) => {
        const accuracy = position.coords.accuracy;
        if (!Number.isFinite(accuracy) || accuracy <= 0) {
          setError("Thiết bị không trả về độ chính xác GPS hợp lệ.");
          return;
        }
        setGps({
          x: position.coords.longitude,
          y: position.coords.latitude,
          crs_code: 4326,
          gps_accuracy_m: accuracy
        });
        setNotice(`Đã lấy GPS · độ chính xác thiết bị ±${accuracy.toFixed(1)} m.`);
      },
      (positionError) => {
        setError(`Không lấy được GPS: ${positionError.message}`);
      },
      {
        enableHighAccuracy: true,
        timeout: 15_000,
        maximumAge: 15_000
      }
    );
  }

  async function syncQueue(){
    if(!scope||!actor||running.current||!navigator.onLine)return;
    running.current=true;setBusy(true);setError(null);
    try{
      await verifyActor();
      // Cross-tab exclusive lock where supported. Each acknowledgement only
      // removes its own row, even when other tabs enqueue concurrently.
      const sync=async()=>{
        let sent=0,conflicts=0;
        const rows=(await listDrafts(scope)).filter(r=>!r.conflict);
        for(let i=0;i<rows.length;i+=50){
          await verifyActor();
          const batch=rows.slice(i,i+50);
          for(const row of batch){
            if(row.photo&&!row.operation.photo_artifact_ids.length){
              await verifyActor();
              const uploaded=await withCsrf(()=>uploadHouseNumberingSurveyPhoto(hnClient,organizationId,projectId,row.photo!));
              const updated:SurveyDraft={...row,operation:{...row.operation,photo_artifact_ids:[uploaded.id]}};
              await saveDraft(updated);row.operation=updated.operation;
            }
          }
          await verifyActor();
          const operations=batch.map(r=>r.operation);
          const result=await withCsrf(()=>syncHouseNumberingSurveys(hnClient,organizationId,projectId,operations,2));
          if(!Array.isArray(result.receipts))throw Error("Máy chủ chưa hỗ trợ biên nhận offline v2; không xóa phiếu.");
          const ack=acknowledgedIds(operations,result.receipts,result.conflicts);
          for(const row of batch){
            if(ack.has(row.operation.client_uuid)){await deleteDraft(row.key);sent++;continue;}
            const conflict=result.conflicts.find(c=>c.client_uuid===row.operation.client_uuid);
            if(conflict){await saveDraft({...row,conflict:String(conflict.code??"CONFLICT")});conflicts++;}
          }
          await refreshQueue();
        }
        setNotice(`Đã nhận biên nhận ${sent} phiếu; ${conflicts} xung đột cần xem lại.`);
      };
      if(!navigator.locks)throw Error("Trình duyệt chưa hỗ trợ khóa đồng bộ đa tab; phiếu vẫn được lưu cục bộ.");
      await navigator.locks.request("ovg-hn-outbox:"+scope,sync);
      await Promise.all([surveys.reload(),progress.reload()]);
    }catch(err){setError(await uiError(err))}
    finally{running.current=false;setBusy(false);await refreshQueue().catch(()=>{})}
  }

  async function enqueue(){
    if(!scope||!actor||!buildingId||busy)return;
    const building=buildings.data?.items.find(b=>b.id===buildingId&&b.state==="active");
    if(!building){setError("Chưa có phiên bản công trình đã tải. Kết nối mạng để tải lại.");return;}
    setBusy(true);setError(null);
    try{
      const operation:SurveyOperation={client_uuid:crypto.randomUUID(),building_id:buildingId,condition,queue_actor_id:actor,expected_building_revision:building.revision,...(gps??{}),...(recordedBy.trim()?{recorded_by_actor_id:recordedBy.trim()}:{}),photo_artifact_ids:[]};
      await addDraft({key:draftKey(scope,operation.client_uuid),scope,operation,...(photo?{photo}:{})});
      setPhoto(null);if(photoInput.current)photoInput.current.value="";setGps(null);
      await refreshQueue();setNotice("Đã lưu phiếu và ảnh vào kho cục bộ.");
    }catch(err){setError(await uiError(err))}
    finally{setBusy(false)}
  }

  return (
    <>
      <section className="hn-card">
        <div className={online ? "hn-ok" : "hn-warning"}>
          {online
            ? "Đang online"
            : "Đang offline — phiếu sẽ lưu cục bộ và sync sau."}
        </div>
        <h2>Khảo sát hiện trường offline</h2>
        <div className="hn-form">
          <label>
            Building
            <select
              value={buildingId}
              onChange={(e) => {setBuildingId(e.target.value);setGps(null);setPhoto(null);if(photoInput.current)photoInput.current.value="";}}
            >
              <option value="">Chọn nhà</option>
              {(buildings.data?.items ?? []).map((building) => (
                <option key={building.id} value={building.id}>
                  {building.parcel_ref ?? building.id.slice(0, 8)}
                </option>
              ))}
            </select>
          </label>
          <label>
            Tình trạng
            <select
              value={condition}
              onChange={(e) => setCondition(e.target.value)}
            >
              <option value="house">Có nhà</option>
              <option value="vacant">Đất trống</option>
              <option value="removed">Không còn</option>
              <option value="other">Khác</option>
            </select>
          </label>
          <label>
            Ảnh
            <input ref={photoInput} type="file" accept="image/png,image/jpeg" disabled={busy||!actor}
              onChange={e=>{const file=e.target.files?.[0];if(!file){setPhoto(null);return;}if(!["image/png","image/jpeg"].includes(file.type)||file.size<=0||file.size>15*1024*1024){setError("Ảnh phải là JPEG/PNG, tối đa 15 MB");e.target.value="";setPhoto(null);return;}setPhoto(file);}}/>
            {photo&&<small>{photo.name} · lưu cùng phiếu khi bấm Lưu</small>}
          </label>
          <label>
            Gửi hộ cho actor UUID (tùy chọn)
            <input
              value={recordedBy}
              onChange={(e) => setRecordedBy(e.target.value)}
              placeholder="Chỉ người có quyền quản lý được gửi hộ"
            />
          </label>
          <div className="hn-actions">
            <button className="hn-btn secondary" type="button" onClick={captureGps}>
              Lấy GPS
            </button>
            {gps ? (
              <span className="hn-badge">
                EPSG:4326 · ±{gps.gps_accuracy_m.toFixed(1)} m
              </span>
            ) : (
              <span className="hn-muted">Chưa lấy GPS</span>
            )}
          </div>
          <button
            className="hn-btn"
            type="button"
            disabled={!buildingId||!actor||busy}
            onClick={()=>void enqueue()}
          >
            Lưu phiếu
          </button>
          <button
            className="hn-btn"
            type="button"
            disabled={!online || !actor || busy || !queue.some(r=>!r.conflict)}
            onClick={() => void syncQueue()}
          >
            Sync {queue.length} phiếu
          </button>
        </div>
        {!actor&&<p role="alert">Cần xác thực tài khoản khi có mạng trước khi sử dụng hàng đợi. Phiếu chưa gửi vẫn được giữ.</p>}
        {legacyPresent&&<p role="alert">Có hàng đợi phiên bản cũ chưa gắn tài khoản. Không tự nhập hoặc xóa; cần đối soát người lập trước khi chuyển.</p>}
        <button type="button" disabled={!scope||busy} onClick={()=>void refreshQueue().catch(async e=>setError(await uiError(e)))}>Đọc lại hàng đợi</button>
        <ul>{queue.map(row=><li key={row.key}>{row.operation.building_id.slice(0,8)} · {row.operation.condition} · {row.operation.photo_artifact_ids.length?"Đã tải ảnh, còn bản cục bộ":row.photo?"Có ảnh cục bộ":"Không ảnh"} · {row.conflict??"Chờ gửi"}</li>)}</ul>
        <p className="hn-muted">Phiếu xung đột không bị xóa hoặc tự gửi lại. Đối soát dữ liệu trước khi xử lý. Ảnh lưu cục bộ đến khi máy chủ xác nhận phiếu; khi đóng trình duyệt cần đăng nhập lại để mở hàng đợi.</p>
        <p className="hn-muted">
          GPS lưu kèm độ chính xác do thiết bị báo. Nếu sai lệch tới geometry nhà lớn
          hơn độ chính xác đó, API trả cảnh báo và ghi audit; không dùng ngưỡng mét
          hardcode. Client UUID đảm bảo retry không tạo phiếu trùng.
        </p>
      </section>

      <section className="hn-card">
        <h2>Tiến độ</h2>
        <div className="hn-grid">
          <div>
            <div className="hn-kpi">{progress.data?.positions ?? 0}</div>
            vị trí đã chấp nhận
          </div>
          <div>
            <div className="hn-kpi">{progress.data?.completion ?? "—"}%</div>
            hoàn thành theo target
          </div>
          <div>
            <div className="hn-kpi">{progress.data?.submitted_pending ?? 0}</div>
            đang chờ duyệt
          </div>
        </div>
        <a
          className="hn-btn secondary"
          href={houseNumberingProgressReportUrl(organizationId, projectId)}
        >
          Tải báo cáo XLSX
        </a>
      </section>

      <section className="hn-card">
        <h2>Phiếu khảo sát</h2>
        <table className="hn-table">
          <thead>
            <tr>
              <th>Building</th>
              <th>Tình trạng</th>
              <th>GPS</th>
              <th>State</th>
              <th>Review</th>
            </tr>
          </thead>
          <tbody>
            {(surveys.data?.items ?? []).map((survey) => (
              <tr key={survey.id}>
                <td>{survey.building_id?.slice(0, 8) ?? "—"}</td>
                <td>{survey.condition}</td>
                <td>
                  {survey.gps_deviation_m == null
                    ? "—"
                    : `${survey.gps_deviation_m.toFixed(1)} m`}
                  {(survey.warnings ?? []).length ? (
                    <div className="hn-warning">
                      {(survey.warnings ?? [])
                        .map((item) => String(item.code ?? "GPS_WARNING"))
                        .join(", ")}
                    </div>
                  ) : null}
                </td>
                <td>{survey.state}</td>
                <td>
                  {survey.state === "submitted" ? (
                    <div className="hn-actions">
                      <button
                        className="hn-btn"
                        onClick={() =>
                          void withCsrf(async () => {
                            await reviewHouseNumberingSurvey(
                              hnClient,
                              organizationId,
                              projectId,
                              survey.id,
                              {
                                expected_revision: survey.revision,
                                decision: "accepted"
                              }
                            );
                            await surveys.reload();
                            await progress.reload();
                          })
                        }
                      >
                        Nhận
                      </button>
                      <button
                        className="hn-btn danger"
                        onClick={() =>
                          void withCsrf(async () => {
                            await reviewHouseNumberingSurvey(
                              hnClient,
                              organizationId,
                              projectId,
                              survey.id,
                              {
                                expected_revision: survey.revision,
                                decision: "rejected"
                              }
                            );
                            await surveys.reload();
                          })
                        }
                      >
                        Từ chối
                      </button>
                    </div>
                  ) : (
                    "—"
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        {notice ? <div className="hn-ok">{notice}</div> : null}
        {error ? <div className="hn-error">{error}</div> : null}
      </section>
    </>
  );
}
