import type { InvitationResourceGrant, Project, Workspace } from "@openvietgis/client-sdk";

export type ScopeSelection = {
  scope: "organization" | "workspace" | "project";
  resourceId: string;
  role: "manager" | "editor" | "contributor" | "viewer";
};

export const initialScope: ScopeSelection = { scope: "organization", resourceId: "", role: "viewer" };

export function selectedGrant(selection: ScopeSelection, workspaces: Workspace[], projects: Project[]): InvitationResourceGrant | undefined {
  if (selection.scope !== "organization" && !(selection.scope === "workspace" ? workspaces : projects)
    .some((item) => item.id === selection.resourceId && item.status === "active")) return undefined;
  return {
    scope_kind: selection.scope,
    role: selection.role,
    ...(selection.scope === "workspace" ? { workspace_id: selection.resourceId } : {}),
    ...(selection.scope === "project" ? { project_id: selection.resourceId } : {})
  };
}

export function ResourceScopeFields({ label, value, onChange, workspaces, projects }: {
  label: string;
  value: ScopeSelection;
  onChange: (selection: ScopeSelection) => void;
  workspaces: Workspace[];
  projects: Project[];
}) {
  return (
    <fieldset>
      <legend>{label}</legend>
      <label>Phạm vi
        <select value={value.scope} onChange={(event) => onChange({
          ...value, scope: event.target.value as ScopeSelection["scope"], resourceId: ""
        })}>
          <option value="organization">Tổ chức</option>
          <option value="workspace">Không gian làm việc</option>
          <option value="project">Dự án</option>
        </select>
      </label>
      {value.scope !== "organization" ? (
        <label>Tài nguyên
          <select required value={(value.scope === "workspace" ? workspaces : projects).some((item) => item.id === value.resourceId && item.status === "active") ? value.resourceId : ""} onChange={(event) => onChange({ ...value, resourceId: event.target.value })}>
            <option value="">Chọn tài nguyên</option>
            {(value.scope === "workspace" ? workspaces : projects).filter((item) => item.status === "active").map((item) => (
              <option key={item.id} value={item.id}>{item.name}</option>
            ))}
          </select>
        </label>
      ) : null}
      <label>Vai trò trên tài nguyên
        <select value={value.role} onChange={(event) => onChange({ ...value, role: event.target.value as ScopeSelection["role"] })}>
          <option value="viewer">Người xem</option>
          <option value="contributor">Cộng tác viên</option>
          <option value="editor">Biên tập viên</option>
          <option value="manager">Quản lý</option>
        </select>
      </label>
    </fieldset>
  );
}
