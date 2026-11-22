import {
  ApiError,
  createOrganization,
  getBootstrapStatus,
  listMyOrganizations,
  loadCsrfToken,
  type CurrentUser
} from "@openvietgis/client-sdk";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import type { FormEvent } from "react";
import { useState } from "react";
import { Link, useParams } from "react-router-dom";

import { apiClient } from "../../app/api";
import { currentProtectedData, protectedQueryOptions } from "../../app/protected-query";
import { AccountMenu } from "../auth/AccountMenu";
import { FreshAccount } from "../auth/FreshAccount";
import { BootstrapPage } from "./BootstrapPage";
import "./organization-home.css";

function slugify(value: string): string {
  return value
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-|-$/g, "");
}

function organizationRoleLabel(role: string): string {
  const labels: Record<string, string> = {
    owner: "Chủ sở hữu",
    admin: "Quản trị viên",
    member: "Thành viên",
    guest: "Khách"
  };
  return labels[role] ?? role;
}

function OrganizationList({ organizationId, account }: { organizationId: string | undefined; account: CurrentUser }) {
  const queryClient = useQueryClient();
  const [name, setName] = useState("");
  const [slug, setSlug] = useState("");

  const organizations = useQuery({
    queryKey: ["my-organizations", account.id],
    queryFn: () => listMyOrganizations(apiClient),
    ...protectedQueryOptions
  });

  async function refreshOrganizations() {
    await queryClient.invalidateQueries({ queryKey: ["my-organizations", account.id] });
  }

  const create = useMutation({
    mutationFn: async (input: { name: string; slug: string }) => {
      await loadCsrfToken(apiClient);
      return createOrganization(apiClient, input);
    },
    onSuccess: async () => {
      setName("");
      setSlug("");
      await refreshOrganizations();
    },
    onError: refreshOrganizations
  });

  const visibleOrganizations = currentProtectedData(organizations);
  if (!visibleOrganizations) {
    const checking = !organizations.error &&
      (organizations.isPending || organizations.fetchStatus !== "idle");
    return (
      <main>
        <h1>Tổ chức</h1>
        {checking ? <p>Đang kiểm tra quyền truy cập tổ chức…</p> : <p role="alert">Không thể tải danh sách tổ chức.</p>}
      </main>
    );
  }

  const activeOrganization = visibleOrganizations.find((item) => item.id === organizationId);

  function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const normalizedSlug = slugify(slug || name);
    if (!normalizedSlug) return;
    create.mutate({ name: name.trim(), slug: normalizedSlug });
  }

  return (
    <div className="org-shell">
      <header className="org-topbar">
        <div className="org-topbar-brand">
          <span className="org-topbar-mark">OVG</span>
          <span>OpenVietGIS</span>
        </div>
        <div className="org-topbar-actions">
          <AccountMenu />
        </div>
      </header>

      <main className="org-page">
        <section className="org-hero">
          <div>
            <span className="org-eyebrow">Không gian làm việc</span>
            <h1>{activeOrganization ? activeOrganization.name : "Tổ chức của bạn"}</h1>
            <p>Chọn tổ chức để quản lý dự án, dữ liệu GIS và quyền truy cập.</p>
          </div>
        </section>

        <div className="org-grid">
          <div style={{ display: "grid", gap: 20 }}>
            <section className="org-card">
              <h2>Tổ chức của bạn</h2>
              <p className="org-card-sub">Các tổ chức mà tài khoản hiện tại có quyền truy cập.</p>
              {visibleOrganizations.length ? (
                <ul className="org-list" data-testid="organization-list">
                  {visibleOrganizations.map((organization) => (
                    <li key={organization.id} className={organization.id === organizationId ? "is-active" : ""}>
                      <Link
                        to={"/organizations/" + organization.id}
                        aria-current={organization.id === organizationId ? "page" : undefined}
                      >
                        {organization.name}
                      </Link>
                      <span className="org-role">{organizationRoleLabel(organization.role)}</span>
                    </li>
                  ))}
                </ul>
              ) : <div className="org-empty">Chưa có tổ chức nào được cấp cho tài khoản này.</div>}
            </section>

            {activeOrganization ? (
              <section className="org-card" aria-labelledby="active-organization-title">
                <h2 id="active-organization-title">Làm việc với {activeOrganization.name}</h2>
                <p className="org-card-sub">Đi nhanh tới các khu vực quản trị và dữ liệu chính.</p>
                <div className="org-shortcuts">
                  <Link className="org-shortcut" to={"/organizations/" + activeOrganization.id + "/projects"}>
                    <strong>Dự án</strong>
                    <small>Bản đồ, lớp dữ liệu và công cụ GIS</small>
                  </Link>
                  <Link className="org-shortcut" to={"/organizations/" + activeOrganization.id + "/workspaces"}>
                    <strong>Không gian làm việc</strong>
                    <small>Sắp xếp dự án theo nhóm làm việc</small>
                  </Link>
                  <Link className="org-shortcut" to={"/organizations/" + activeOrganization.id + "/membership"}>
                    <strong>Thành viên & phân quyền</strong>
                    <small>Quản lý người dùng và quyền truy cập</small>
                  </Link>
                </div>
                <dl className="org-meta" data-testid="active-organization">
                  <dt>Vai trò</dt>
                  <dd>{organizationRoleLabel(activeOrganization.role)}</dd>
                  <dt>Mã tổ chức</dt>
                  <dd>{activeOrganization.id}</dd>
                </dl>
              </section>
            ) : null}
          </div>

          <aside className="org-card">
            <h2>Tạo tổ chức mới</h2>
            <p className="org-card-sub">Tạo một không gian quản trị độc lập cho nhóm hoặc đơn vị mới.</p>
            <form className="org-form" onSubmit={submit}>
              <label>
                Tên tổ chức
                <input
                  aria-label="Tên tổ chức"
                  value={name}
                  onChange={(event) => {
                    setName(event.target.value);
                    if (!slug) setSlug(slugify(event.target.value));
                  }}
                  placeholder="Ví dụ: Trung tâm GIS"
                  required
                />
              </label>
              <label>
                Mã định danh
                <input
                  aria-label="Mã định danh tổ chức"
                  value={slug}
                  onChange={(event) => setSlug(event.target.value)}
                  placeholder="trung-tam-gis"
                  required
                />
              </label>
              <button className="org-primary-btn" type="submit" disabled={create.isPending}>
                {create.isPending ? "Đang tạo…" : "Tạo tổ chức"}
              </button>
            </form>
            {create.error instanceof ApiError && create.error.status === 409 ? (
              <p className="org-alert" role="alert">Hệ thống phải được khởi tạo trước khi tạo thêm tổ chức.</p>
            ) : create.error ? (
              <p className="org-alert" role="alert">Không thể tạo tổ chức.</p>
            ) : null}
          </aside>
        </div>
      </main>
    </div>
  );
}

function InstallationGate({ organizationId, account }: { organizationId: string | undefined; account: CurrentUser }) {
  const status = useQuery({
    queryKey: ["bootstrap-status"],
    queryFn: () => getBootstrapStatus(apiClient),
    ...protectedQueryOptions
  });
  const installation = currentProtectedData(status);
  if (!installation) {
    return status.error ? <p role="alert">Không thể kiểm tra trạng thái hệ thống.</p> :
      <p>Đang kiểm tra trạng thái hệ thống…</p>;
  }
  if (!installation.initialized) {
    return <BootstrapPage onInitialized={async () => { await status.refetch(); }} />;
  }
  return <OrganizationList key={account.id} organizationId={organizationId} account={account} />;
}

export function OrganizationHome() {
  const { organizationId } = useParams();
  return (
    <FreshAccount key={organizationId ?? "home"}>
      {(account) => (
        <InstallationGate
          key={account.id}
          organizationId={organizationId}
          account={account}
        />
      )}
    </FreshAccount>
  );
}
