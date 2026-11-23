import React, {useEffect, useMemo, useState} from "react";
import {type ApiClient} from "@openvietgis/client-sdk";
import "./integration-catalog.css";

type ConnectorLifecycle = "existing_read_only" | "candidate" | "planned";
type CatalogAction = {
  action: string;
  required_permission: string;
  eligible_by_role: boolean;
  executable_in_hub: boolean;
  allowed: boolean;
  reason: string;
};
type Adapter = {
  key: string;
  name: string;
  category: string;
  mode: string;
  lifecycle: ConnectorLifecycle;
  protocols: string[];
  note: string;
  existing_api: string | null;
  actions: CatalogAction[];
};
type CatalogResponse = {
  schema_version: number;
  mode: string;
  default_write_policy: string;
  adapters: Adapter[];
};

const statuses: Record<ConnectorLifecycle,string> = {
  existing_read_only: "Có dịch vụ chỉ đọc",
  candidate: "Đang thử nghiệm",
  planned: "Chưa triển khai",
};
const categories: Record<string,string> = {
  gis:"GIS", cad:"CAD", cadastral:"Địa chính", database:"Cơ sở dữ liệu",
  tabular:"Bảng tính", survey_3d:"Trắc địa / 3D",
};

export function IntegrationCatalog({client,org,projectId}:{
  client:ApiClient;
  org:string;
  projectId:string;
}){
  const [catalog,setCatalog]=useState<CatalogResponse|null>(null);
  const [error,setError]=useState("");
  const [loading,setLoading]=useState(false);
  const [query,setQuery]=useState("");
  useEffect(()=>{
    let alive=true;
    setCatalog(null);setError("");setLoading(true);
    const path="/organizations/"+encodeURIComponent(org)+"/projects/"+
      encodeURIComponent(projectId)+"/integration-hub/catalog";
    void client.request<CatalogResponse>(path).then(value=>{
      if(alive)setCatalog(value);
    }).catch(()=>{
      if(alive)setError("Không thể đọc danh mục. Hãy kiểm tra quyền xem dự án.");
    }).finally(()=>{if(alive)setLoading(false)});
    return ()=>{alive=false};
  },[client,org,projectId]);
  const items=useMemo(()=>catalog?.adapters.filter(a=>
    (a.name+" "+a.category+" "+a.protocols.join(" ")).toLocaleLowerCase("vi")
      .includes(query.trim().toLocaleLowerCase("vi")))??[],[catalog,query]);
  return <section className="integration-catalog" aria-label="Tích hợp phần mềm">
    <details>
      <summary><strong>Tích hợp phần mềm</strong><span>{catalog?catalog.adapters.length+" nhóm kết nối · chính sách mặc định từ chối ghi":"GIS · CAD · Địa chính · 3D"}</span></summary>
      <p className="live-note">Danh mục năng lực kỹ thuật của OpenVietGIS. Quyền dự án không tự động cấp quyền truy cập tệp hoặc cơ sở dữ liệu ở phần mềm bên ngoài. Chỉ bật thao tác sau khi adapter được kiểm thử.</p>
      {loading&&<p role="status">Đang kiểm tra danh mục…</p>}
      {error&&<p className="error" role="alert">{error}</p>}
      {catalog&&<>
        <label className="integration-search">Tìm phần mềm hoặc định dạng
          <input value={query} onChange={e=>setQuery(e.target.value)} placeholder="QGIS, AutoCAD, DGN, WFS…"/>
        </label>
        <div className="integration-list">{items.map(a=>{
          const read=a.actions.find(x=>x.action==="catalog.discover");
          const write=a.actions.find(x=>x.action==="sync.push");
          return <article key={a.key} className="integration-item">
            <div className="integration-item-header"><b>{a.name}</b><small>{statuses[a.lifecycle]}</small></div>
            <div className="integration-meta"><span>{categories[a.category]??a.category}</span><span>{a.protocols.join(" · ")}</span></div>
            <p>{a.note}</p>
            <small>Quyền xem danh mục: {read?.allowed?"Được phép":"Không được cấp"}. Đồng bộ ghi: {write?.executable_in_hub?"Có API":"Chưa hỗ trợ"}.</small>
            {a.existing_api&&<small>Dịch vụ hiện hữu: {a.existing_api} (vẫn áp dụng phân quyền riêng).</small>}
          </article>;
        })}</div>
        {!items.length&&<p className="live-note">Không có kết nối khớp từ khóa.</p>}
      </>}
    </details>
  </section>;
}
