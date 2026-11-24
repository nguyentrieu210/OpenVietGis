import { Link } from "react-router-dom";

import { ThemeToggle } from "../../app/shell/ThemeToggle";
import { AccountMenu } from "../auth/AccountMenu";
import { invokeCommand, type MapCommand } from "./commands";
import type { ActiveModuleSummary, MapDockToolId } from "./MapToolDock";

interface MapRibbonProps {
  organizationId: string;
  projectName: string;
  revision: number;
  writable: boolean;
  archived: boolean;
  dirty: boolean;
  saveState: string;
  saveLabel: string;
  needsRecovery: boolean;
  sharedControlsOpen: boolean;
  cmdLineOpen: boolean;
  commands: MapCommand[];
  activeTool: MapDockToolId | null;
  activeModules: ActiveModuleSummary[];
  leftOpen: boolean;
  rightOpen: boolean;
  tableOpen: boolean;
  onToggleLeft: () => void;
  onToggleRight: () => void;
  onToggleTable: () => void;
  onToggleCmdLine: () => void;
  onSelectTool: (tool: MapDockToolId | null) => void;
  onOpenTechnical: () => void;
  onToggleSharedControls: () => void;
}

export function MapRibbon({
  organizationId,
  projectName,
  revision,
  dirty,
  saveState,
  saveLabel,
  needsRecovery,
  sharedControlsOpen,
  cmdLineOpen,
  commands,
  rightOpen,
  tableOpen,
  onToggleRight,
  onToggleTable,
  onToggleCmdLine,
  onOpenTechnical,
  onToggleSharedControls
}: MapRibbonProps) {
  const addCmd = commands.find((c) => c.id === "map.add-data");
  const fitAllCmd = commands.find((c) => c.id === "map.fit-all");
  const saveCmd = commands.find((c) => c.id === "map.save");

  return (
    <header className="gis-top-menubar" data-testid="gis-ribbon">
      <div className="gis-header-left">
        <Link to="/app" className="gis-home-badge" title="Về Trang tổng quan">
          OVG
        </Link>
        <Link to={`/app/organizations/${organizationId}/projects`} className="gis-back-btn">
          ← Dự án
        </Link>
        <h1>
          {projectName}
          <small>v{revision}</small>
        </h1>
        <button
          type="button"
          className={dirty || needsRecovery
            ? "map-context-unsaved map-status-pill"
            : "map-context-saved map-status-pill"}
          aria-label={`Cấu hình bản đồ dùng chung · ${saveLabel}`}
          aria-controls="map-shared-controls"
          aria-expanded={sharedControlsOpen}
          data-save-state={saveState}
          onClick={onToggleSharedControls}
        >
          {saveLabel}
        </button>
      </div>

      <nav className="gis-header-meta" aria-label="Lệnh bản đồ">
        {addCmd ? (
          <button
            type="button"
            className="gis-top-primary"
            aria-label={addCmd.label}
            disabled={!addCmd.enabled}
            title={addCmd.reason ?? addCmd.label}
            onClick={() => invokeCommand(commands, addCmd.id, "toolbar")}
          >
            + Thêm dữ liệu
          </button>
        ) : null}
        {fitAllCmd ? (
          <button
            type="button"
            disabled={!fitAllCmd.enabled}
            aria-label={fitAllCmd.label}
            title={fitAllCmd.reason ?? fitAllCmd.label}
            onClick={() => invokeCommand(commands, fitAllCmd.id, "toolbar")}
          >
            Toàn bản đồ
          </button>
        ) : null}
        {saveCmd ? (
          <button
            type="button"
            disabled={!saveCmd.enabled}
            title={saveCmd.reason ?? saveCmd.label}
            onClick={() => invokeCommand(commands, saveCmd.id, "toolbar")}
          >
            Lưu
          </button>
        ) : null}

        <span className="gis-top-sep" aria-hidden="true" />

        <button
          type="button"
          className={tableOpen ? "is-on" : undefined}
          onClick={onToggleTable}
        >
          Bảng thuộc tính
        </button>
        <button
          type="button"
          className={rightOpen ? "is-on" : undefined}
          onClick={onToggleRight}
        >
          Chi tiết
        </button>
        <button
          type="button"
          className={cmdLineOpen ? "is-on" : undefined}
          title="Bật / Tắt dòng lệnh AutoCAD (phím /)"
          onClick={onToggleCmdLine}
        >
          &gt;_ Lệnh
        </button>
        <button
          type="button"
          aria-label="Thông tin kỹ thuật"
          title="Thông tin kỹ thuật & CRS"
          onClick={onOpenTechnical}
        >
          Kỹ thuật
        </button>

        <ThemeToggle />
        <AccountMenu />
      </nav>
    </header>
  );
}
