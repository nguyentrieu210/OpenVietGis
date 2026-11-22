import { Link } from "react-router-dom";
import { AccountMenu } from "../auth/AccountMenu";
import "./platform.css";

interface ProjectToolbarProps {
  organizationId: string;
  projectId: string;
  active: "editing" | "queries" | "processing" | "cartography" | "hub";
  title: string;
}

export function ProjectToolbar({
  organizationId,
  projectId,
  active,
  title
}: ProjectToolbarProps) {
  const base = `/organizations/${organizationId}/projects/${projectId}`;
  return (
    <header className="ovg-toolbar">
      <div>
        <strong>OpenVietGIS</strong> · {title}
      </div>
      <nav className="ovg-toolbar-links" aria-label="GIS Workbenches">
        <Link to={`${base}/map`}>Bản đồ</Link>
        <Link to={`${base}/spatial`}>Danh mục</Link>
        <Link to={`${base}/transfers`}>Nhập/Xuất</Link>
        <Link to={`${base}/editing`} className={active === "editing" ? "active" : ""}>
          Biên tập (R006)
        </Link>
        <Link to={`${base}/queries`} className={active === "queries" ? "active" : ""}>
          Truy vấn (R007)
        </Link>
        <Link to={`${base}/processing`} className={active === "processing" ? "active" : ""}>
          Xử lý (R008)
        </Link>
        <Link to={`${base}/cartography`} className={active === "cartography" ? "active" : ""}>
          In ấn (R009)
        </Link>
        <Link to={`${base}/platform-hub`} className={active === "hub" ? "active" : ""}>
          Công cụ nâng cao
        </Link>
        <AccountMenu />
      </nav>
    </header>
  );
}
