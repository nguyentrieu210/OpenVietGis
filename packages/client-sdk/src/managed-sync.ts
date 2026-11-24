import { projectApiPath, type ApiClient } from "./http";

export interface ManagedSessionItem {
  id: string;
  organization_id: string;
  project_id: string;
  binding_id: string | null;
  actor_type: string;
  actor_id: string;
  actor_display: string;
  device_label: string;
  client_version: string;
  mode: "managed" | "legacy_snapshot";
  capabilities: {
    can_read?: boolean;
    can_edit?: boolean;
    can_delete?: boolean;
    realtime_delta?: boolean;
    ai_desktop_bridge?: boolean;
  };
  permission_epoch: number;
  last_ack_cursor: number;
  state: "active" | "paused" | "revoked" | "expired";
  expires_at: string;
  revoked_at: string | null;
  last_seen_at: string;
  created_at: string;
}

export interface ManagedSessionListResponse {
  current_cursor: number;
  permission_epoch: number;
  retention_floor: number;
  items: ManagedSessionItem[];
}

export interface DesktopCommandResponse {
  id: string;
  session_id: string;
  tool_name: string;
  arguments: Record<string, unknown>;
  requires_consent: boolean;
  status: string;
  result: Record<string, unknown> | null;
  expires_at: string;
  created_at: string;
  resolved_at: string | null;
}

function managedBase(orgId: string, projectId: string): string {
  return projectApiPath(orgId, projectId) + "/integration-hub/managed";
}

export function listManagedSessions(
  client: ApiClient,
  orgId: string,
  projectId: string,
): Promise<ManagedSessionListResponse> {
  return client.request<ManagedSessionListResponse>(`${managedBase(orgId, projectId)}/sessions`);
}

export function revokeManagedSession(
  client: ApiClient,
  orgId: string,
  projectId: string,
  sessionId: string,
  csrfToken: string,
  bumpEpoch = false,
): Promise<ManagedSessionItem> {
  const qs = bumpEpoch ? "?bump_epoch=true" : "";
  return client.request<ManagedSessionItem>(
    `${managedBase(orgId, projectId)}/sessions/${encodeURIComponent(sessionId)}${qs}`,
    {
      method: "DELETE",
      headers: { "X-OpenVietGIS-CSRF": csrfToken },
    },
  );
}

export function sendDesktopCommand(
  client: ApiClient,
  orgId: string,
  projectId: string,
  sessionId: string,
  payload: { tool_name: string; arguments?: Record<string, unknown>; ttl_seconds?: number },
  csrfToken: string,
): Promise<DesktopCommandResponse> {
  return client.request<DesktopCommandResponse>(
    `${managedBase(orgId, projectId)}/desktop-sessions/${encodeURIComponent(sessionId)}/commands`,
    {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "X-OpenVietGIS-CSRF": csrfToken,
      },
      body: JSON.stringify(payload),
    },
  );
}
