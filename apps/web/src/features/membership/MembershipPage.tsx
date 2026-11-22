import {
  ApiError, createResourceGrant, getEffectivePermissions,
  listOrganizationMembers, listProjects, listResourceGrants, listWorkspaces,
  loadCsrfToken, revokeResourceGrant, updateOrganizationMembership,
  type MembershipUpdate
} from "@openvietgis/client-sdk";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import type { FormEvent } from "react";
import { useState } from "react";
import { Link, useParams } from "react-router-dom";

import { apiClient } from "../../app/api";
import { currentProtectedData, isAuthorizationLossStatus, protectedQueryOptions } from "../../app/protected-query";
import { FreshAccount } from "../auth/FreshAccount";
import { InvitationIssuePanel } from "../invitations/InvitationIssuePanel";
import { initialScope, ResourceScopeFields, selectedGrant } from "./ResourceScopeFields";

function membershipRoleLabel(role: string): string {
  const labels: Record<string, string> = {
    owner: "Chủ sở hữu", admin: "Quản trị viên", member: "Thành viên", guest: "Khách",
    manager: "Quản lý", editor: "Biên tập viên", contributor: "Cộng tác viên", viewer: "Người xem"
  };
  return labels[role] ?? role;
}
function membershipStatusLabel(status: string): string {
  const labels: Record<string, string> = {
    active: "Đang hoạt động", suspended: "Tạm khóa", revoked: "Đã thu hồi"
  };
  return labels[status] ?? status;
}
function grantScopeLabel(scope: string): string {
  const labels: Record<string, string> = {
    organization: "Tổ chức", workspace: "Không gian làm việc", project: "Dự án"
  };
  return labels[scope] ?? scope;
}

function MemberAdministration({ organizationId, userId, permissions }: {
  organizationId: string;
  userId: string;
  permissions: string[];
}) {
  const queryClient = useQueryClient();
  const [membershipId, setMembershipId] = useState("");
  const [scope, setScope] = useState(initialScope);
  const canManageMembers = permissions.includes("organization.manage_members");
  const canManageGrants = permissions.includes("authorization.manage_grants");
  const canManageInvitations = permissions.includes("invitation.manage");

  const members = useQuery({
    queryKey: ["members", organizationId, userId],
    queryFn: () => listOrganizationMembers(apiClient, organizationId),
    enabled: canManageMembers || canManageGrants,
    ...protectedQueryOptions
  });
  const grants = useQuery({
    queryKey: ["grants", organizationId, userId],
    queryFn: () => listResourceGrants(apiClient, organizationId),
    enabled: canManageGrants,
    ...protectedQueryOptions
  });
  const workspaces = useQuery({
    queryKey: ["workspaces", organizationId, userId],
    queryFn: () => listWorkspaces(apiClient, organizationId),
    enabled: canManageGrants || canManageInvitations,
    ...protectedQueryOptions
  });
  const projects = useQuery({
    queryKey: ["projects", organizationId, userId],
    queryFn: () => listProjects(apiClient, organizationId),
    enabled: canManageGrants || canManageInvitations,
    ...protectedQueryOptions
  });

  const currentMembers = currentProtectedData(members);
  const currentGrants = currentProtectedData(grants);
  const currentWorkspaces = currentProtectedData(workspaces);
  const currentProjects = currentProtectedData(projects);
  const availableMembers = currentMembers ?? [];
  const availableWorkspaces = currentWorkspaces ?? [];
  const availableProjects = currentProjects ?? [];
  const canSubmitGrant = currentGrants !== undefined && availableMembers.some((member) => member.id === membershipId && member.status === "active") &&
    !!selectedGrant(scope, availableWorkspaces, availableProjects);

  async function refreshChildren() {
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: ["members", organizationId, userId] }),
      queryClient.invalidateQueries({ queryKey: ["grants", organizationId, userId] }),
      queryClient.invalidateQueries({ queryKey: ["workspaces", organizationId, userId] }),
      queryClient.invalidateQueries({ queryKey: ["projects", organizationId, userId] })
    ]);
  }

  async function refreshAccess() {
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: ["permissions", organizationId, userId] }),
      queryClient.invalidateQueries({ queryKey: ["my-organizations", userId] }),
      refreshChildren()
    ]);
  }

  async function refreshAfterError(error: unknown) {
    if (error instanceof ApiError && isAuthorizationLossStatus(error.status)) {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ["me"] }),
        refreshAccess()
      ]);
    } else {
      // Validation/conflict failures should retain the form and safe error notice.
      // Refresh affected child choices without unmounting their authorization gate.
      await refreshChildren();
    }
  }

  const changeMember = useMutation({
    mutationFn: async ({ id, change }: { id: string; change: MembershipUpdate }) => {
      await loadCsrfToken(apiClient);
      await updateOrganizationMembership(apiClient, organizationId, id, change);
    },
    onSuccess: refreshAccess,
    onError: refreshAfterError
  });
  const grantAccess = useMutation({
    mutationFn: async () => {
      const grant = selectedGrant(scope, availableWorkspaces, availableProjects);
      if (!grant || !canSubmitGrant) throw new Error("Thiếu đối tượng cần cấp quyền");
      await loadCsrfToken(apiClient);
      await createResourceGrant(apiClient, organizationId, {
        ...grant, subject_kind: "membership", subject_id: membershipId
      });
    },
    onSuccess: async () => {
      setMembershipId("");
      await refreshAccess();
    },
    onError: refreshAfterError
  });
  const revokeAccess = useMutation({
    mutationFn: async (id: string) => {
      await loadCsrfToken(apiClient);
      await revokeResourceGrant(apiClient, organizationId, id);
    },
    onSuccess: refreshAccess,
    onError: refreshAfterError
  });

  function submitGrant(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!grantAccess.isPending && canSubmitGrant) grantAccess.mutate();
  }

  return (
    <>
      {canManageMembers ? (
        <section>
          <h2>Thành viên</h2>
          {members.fetchStatus !== "idle" || members.isPending ? <p>Đang tải thành viên…</p> : !currentMembers ? <p role="alert">Không thể tải danh sách thành viên.</p> : null}
          <ul data-testid="member-list">
            {availableMembers.map((member) => (
              <li key={member.id} data-testid={"member-" + member.id}>
                {member.user_id}{member.user_id === userId ? " (bạn)" : ""} — {membershipRoleLabel(member.role)} — {membershipStatusLabel(member.status)}{" "}
                {member.status === "active" ? (
                  <button type="button" disabled={changeMember.isPending} onClick={() => changeMember.mutate({ id: member.id, change: { status: "suspended" } })}>Tạm khóa</button>
                ) : member.status === "suspended" ? (
                  <button type="button" disabled={changeMember.isPending} onClick={() => changeMember.mutate({ id: member.id, change: { status: "active" } })}>Khôi phục</button>
                ) : null}
                {member.status !== "revoked" ? (
                  <button type="button" disabled={changeMember.isPending} onClick={() => {
                    if (window.confirm("Bạn có chắc muốn thu hồi thành viên này?")) changeMember.mutate({ id: member.id, change: { status: "revoked" } });
                  }}>Thu hồi</button>
                ) : null}
              </li>
            ))}
          </ul>
          {changeMember.error ? <p role="alert">Không thể thay đổi tư cách thành viên. Hệ thống đang áp dụng quy tắc Chủ sở hữu và vòng đời tài khoản.</p> : null}
        </section>
      ) : null}

      {canManageGrants ? (
        <section>
          <h2>Phân quyền tài nguyên</h2>
          <form onSubmit={submitGrant}>
            <label>Thành viên
              <select required value={availableMembers.some((member) => member.id === membershipId && member.status === "active") ? membershipId : ""} onChange={(event) => setMembershipId(event.target.value)}>
                <option value="">Chọn thành viên</option>
                {availableMembers.filter((member) => member.status === "active").map((member) => (
                  <option key={member.id} value={member.id}>{member.user_id} — {membershipRoleLabel(member.role)}</option>
                ))}
              </select>
            </label>
            <ResourceScopeFields label="Cấp quyền truy cập tài nguyên" value={scope} onChange={setScope} workspaces={availableWorkspaces} projects={availableProjects} />
            <button type="submit" disabled={grantAccess.isPending || !canSubmitGrant}>Cấp quyền</button>
          </form>
          {grants.fetchStatus !== "idle" || grants.isPending ? <p>Đang tải phân quyền…</p> : null}
          {grants.error || members.error || workspaces.error || projects.error ? <p role="alert">Không thể tải tài nguyên phân quyền.</p> : null}
          <ul data-testid="resource-grant-list">
            {currentGrants?.map((grant) => (
              <li key={grant.id}>
                {grant.subject_id} — {membershipRoleLabel(grant.role)} — {grantScopeLabel(grant.scope_kind)} {grant.workspace_id ?? grant.project_id ?? ""} — {grant.revoked_at ? "Đã thu hồi" : "Đang hoạt động"}{" "}
                {!grant.revoked_at ? (
                  <button type="button" disabled={revokeAccess.isPending} onClick={() => revokeAccess.mutate(grant.id)}>Thu hồi quyền</button>
                ) : null}
              </li>
            ))}
          </ul>
          {grantAccess.error || revokeAccess.error ? <p role="alert">Không thể cập nhật phân quyền.</p> : null}
        </section>
      ) : null}

      {canManageInvitations ? (
        <InvitationIssuePanel organizationId={organizationId} workspaces={availableWorkspaces} projects={availableProjects} onAccessError={refreshAfterError} />
      ) : null}
      {!canManageMembers && !canManageGrants && !canManageInvitations ? (
        <p>Bạn không có quyền quản trị tổ chức này.</p>
      ) : null}
    </>
  );
}

function OrganizationMembership({ organizationId, userId }: { organizationId: string; userId: string }) {
  const effective = useQuery({
    queryKey: ["permissions", organizationId, userId],
    queryFn: () => getEffectivePermissions(apiClient, organizationId),
    ...protectedQueryOptions
  });

  const current = currentProtectedData(effective);

  return (
    <main style={{ maxWidth: 960, margin: "40px auto", fontFamily: "system-ui, sans-serif" }}>
      <Link to={"/organizations/" + encodeURIComponent(organizationId)}>← Tổ chức</Link>
      <h1>Thành viên & Phân quyền</h1>
      {effective.fetchStatus !== "idle" || effective.isPending ? <p>Đang kiểm tra quyền của tổ chức…</p> : !current ? (
        <p role="alert">Không thể tải quyền truy cập tổ chức.</p>
      ) : (
        <MemberAdministration organizationId={organizationId} userId={userId} permissions={current.permissions} />
      )}
    </main>
  );
}

export function MembershipPage() {
  const { organizationId } = useParams();
  if (!organizationId) return <p role="alert">Thiếu ngữ cảnh tổ chức.</p>;
  return (
    <FreshAccount key={organizationId}>
      {(account) => <OrganizationMembership key={organizationId + ":" + account.id} organizationId={organizationId} userId={account.id} />}
    </FreshAccount>
  );
}
