import {
  createProject,
  listProjects,
  listWorkspaces,
  loadCsrfToken,
  updateProject
} from "@openvietgis/client-sdk";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import type { FormEvent } from "react";
import { useState } from "react";
import { Link, useParams } from "react-router-dom";

import { apiClient } from "../../app/api";
import { currentProtectedData, protectedQueryOptions } from "../../app/protected-query";
import { FreshAccount } from "../auth/FreshAccount";

function slugify(value: string): string {
  return value
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-|-$/g, "");
}

function projectStatusLabel(status: string): string {
  const labels: Record<string, string> = {
    active: "Đang hoạt động",
    archived: "Đã lưu trữ",
    deleted: "Đã xóa"
  };
  return labels[status] ?? status;
}

function OrganizationProjects({ organizationId, userId }: { organizationId: string; userId: string }) {
  const queryClient = useQueryClient();
  const [name, setName] = useState("");
  const [slug, setSlug] = useState("");
  const [workspaceId, setWorkspaceId] = useState("");
  const [moveTargets, setMoveTargets] = useState<Record<string, string>>({});

  const workspaces = useQuery({
    queryKey: ["workspaces", organizationId, userId],
    queryFn: () => listWorkspaces(apiClient, organizationId),
    ...protectedQueryOptions
  });
  const projects = useQuery({
    queryKey: ["projects", organizationId, userId],
    queryFn: () => listProjects(apiClient, organizationId),
    ...protectedQueryOptions
  });

  const refreshProjects = async () => {
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: ["projects", organizationId, userId] }),
      queryClient.invalidateQueries({ queryKey: ["workspaces", organizationId, userId] })
    ]);
  };

  const create = useMutation({
    mutationFn: async () => {
      await loadCsrfToken(apiClient);
      return createProject(apiClient, organizationId, {
        name: name.trim(),
        slug: slugify(slug || name),
        ...(workspaceId ? { workspace_id: workspaceId } : {})
      });
    },
    onSuccess: async () => {
      setName("");
      setSlug("");
      setWorkspaceId("");
      await refreshProjects();
    },
    onError: refreshProjects
  });

  const update = useMutation({
    mutationFn: async ({
      projectId,
      input
    }: {
      projectId: string;
      input: {
        workspace_id?: string | null;
        status?: "active" | "archived" | "deleted";
      };
    }) => {
      await loadCsrfToken(apiClient);
      return updateProject(apiClient, organizationId, projectId, input);
    },
    onSuccess: refreshProjects,
    onError: refreshProjects
  });

  function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!name.trim()) return;
    create.mutate();
  }

  const visibleProjects = currentProtectedData(projects);
  const visibleWorkspaces = currentProtectedData(workspaces);
  if (!visibleProjects || !visibleWorkspaces) {
    const checking = !projects.error && !workspaces.error &&
      (projects.isPending || projects.fetchStatus !== "idle" ||
        workspaces.isPending || workspaces.fetchStatus !== "idle");
    return (
      <main>
        <Link to="/">← Tổ chức</Link>
        <h1>Dự án</h1>
        {checking ? <p>Đang kiểm tra quyền truy cập dự án…</p> : <p role="alert">Không thể tải quyền truy cập dự án.</p>}
      </main>
    );
  }

  return (
    <main style={{ maxWidth: 960, margin: "40px auto", fontFamily: "system-ui, sans-serif" }}>
      <p>
        <Link to={"/organizations/" + organizationId}>← Tổ chức</Link>{" "}
        <Link to={"/organizations/" + organizationId + "/workspaces"}>Không gian làm việc</Link>
      </p>
      <h1>Dự án</h1>

      <form onSubmit={submit}>
        <label>
          Tên dự án
          <input
            aria-label="Tên dự án"
            value={name}
            onChange={(event) => {
              setName(event.target.value);
              if (!slug) setSlug(slugify(event.target.value));
            }}
            required
          />
        </label>
        <label>
          Mã định danh dự án
          <input
            aria-label="Mã định danh dự án"
            value={slug}
            onChange={(event) => setSlug(event.target.value)}
            required
          />
        </label>
        <label>
          Không gian làm việc
          <select
            aria-label="Không gian làm việc của dự án"
            value={workspaceId}
            onChange={(event) => setWorkspaceId(event.target.value)}
          >
            <option value="">Chưa phân nhóm</option>
            {visibleWorkspaces.map((workspace) => (
              <option key={workspace.id} value={workspace.id}>
                {workspace.name}
              </option>
            ))}
          </select>
        </label>
        <button type="submit" disabled={create.isPending}>
          Tạo dự án
        </button>
      </form>

      {create.error ? <p role="alert">Không thể tạo dự án.</p> : null}
      {update.error ? <p role="alert">Không thể cập nhật dự án.</p> : null}

      <ul data-testid="project-list">
        {visibleProjects.map((project) => {
          const moveTarget = moveTargets[project.id] ?? project.workspace_id ?? "";
          return (
            <li key={project.id} data-testid={"project-" + project.id}>
              <strong>{project.name}</strong> — {projectStatusLabel(project.status)} — không gian làm việc{" "}
              {project.workspace_id ?? "chưa phân nhóm"}{" "}
              {project.status !== "deleted" ? (
                <><Link to={"/organizations/" + encodeURIComponent(organizationId) + "/projects/" + encodeURIComponent(project.id) + "/spatial"}>Danh mục Dữ liệu</Link>{" · "}<Link to={"/organizations/" + encodeURIComponent(organizationId) + "/projects/" + encodeURIComponent(project.id) + "/map"}>Mở Bản đồ</Link></>
              ) : null}
              <div>
                <label>
                  Không gian làm việc của {project.name}
                  <select
                    aria-label={"Không gian làm việc của " + project.name}
                    value={moveTarget}
                    onChange={(event) =>
                      setMoveTargets((current) => ({
                        ...current,
                        [project.id]: event.target.value
                      }))
                    }
                  >
                    <option value="">Chưa phân nhóm</option>
                    {visibleWorkspaces.map((workspace) => (
                      <option key={workspace.id} value={workspace.id}>
                        {workspace.name}
                      </option>
                    ))}
                  </select>
                </label>
                <button
                  type="button"
                  onClick={() =>
                    update.mutate({
                      projectId: project.id,
                      input: {
                        workspace_id: moveTarget || null
                      }
                    })
                  }
                >
                  Chuyển nhóm {project.name}
                </button>
                <button
                  type="button"
                  onClick={() =>
                    update.mutate({
                      projectId: project.id,
                      input: { workspace_id: null }
                    })
                  }
                >
                  Bỏ khỏi nhóm {project.name}
                </button>
                {project.status === "active" ? (
                  <button
                    type="button"
                    onClick={() =>
                      update.mutate({
                        projectId: project.id,
                        input: { status: "archived" }
                      })
                    }
                  >
                    Lưu trữ {project.name}
                  </button>
                ) : project.status === "archived" ? (
                  <button
                    type="button"
                    onClick={() =>
                      update.mutate({
                        projectId: project.id,
                        input: { status: "active" }
                      })
                    }
                  >
                    Khôi phục {project.name}
                  </button>
                ) : null}
              </div>
            </li>
          );
        })}
      </ul>
    </main>
  );
}

export function ProjectPage() {
  const { organizationId } = useParams();
  if (!organizationId) return <p role="alert">Thiếu ngữ cảnh tổ chức.</p>;
  return (
    <FreshAccount key={organizationId}>
      {(account) => (
        <OrganizationProjects
          key={organizationId + ":" + account.id}
          organizationId={organizationId}
          userId={account.id}
        />
      )}
    </FreshAccount>
  );
}
