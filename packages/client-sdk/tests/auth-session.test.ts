import { describe, expect, it } from "vitest";

import {
  ApiClient,
  createOrganization,
  getCurrentUser,
  listIdentityProviders,
  listMyOrganizations,
  loadCsrfToken
} from "../src";

function json(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" }
  });
}

describe("identity and organization client", () => {
  it("loads providers, current user, and organizations", async () => {
    const paths: string[] = [];
    const client = new ApiClient({
      baseUrl: "http://example.test/api/v1",
      fetchImpl: (async (input: RequestInfo | URL) => {
        const path = new URL(String(input)).pathname;
        paths.push(path);
        if (path.endsWith("/auth/providers")) {
          return json([{ id: "keycloak", display_name: "Keycloak" }]);
        }
        if (path.endsWith("/me/organizations")) {
          return json([{ id: "o1", name: "Org", slug: "org", role: "owner" }]);
        }
        return json({
          id: "u1",
          display_name: "User",
          email_snapshot: null,
          status: "active"
        });
      }) as typeof fetch
    });

    expect((await listIdentityProviders(client))[0]?.id).toBe("keycloak");
    expect((await getCurrentUser(client)).id).toBe("u1");
    expect((await listMyOrganizations(client))[0]?.role).toBe("owner");
    expect(paths).toHaveLength(3);
  });

  it("loads CSRF then sends it on organization creation", async () => {
    const calls: Array<{ path: string; init: RequestInit | undefined }> = [];
    const client = new ApiClient({
      baseUrl: "http://example.test/api/v1",
      fetchImpl: (async (input: RequestInfo | URL, init?: RequestInit) => {
        const path = new URL(String(input)).pathname;
        calls.push({ path, init });
        if (path.endsWith("/auth/csrf")) {
          return json({ token: "csrf-123" });
        }
        return json(
          {
            organization: { id: "o1", name: "Org", slug: "org", status: "active" },
            owner_membership: {
              id: "m1",
              organization_id: "o1",
              user_id: "u1",
              role: "owner",
              status: "active"
            }
          },
          201
        );
      }) as typeof fetch
    });

    await loadCsrfToken(client);
    const result = await createOrganization(client, { name: "Org", slug: "org" });

    expect(result.owner_membership.role).toBe("owner");
    const headers = new Headers(calls[1]?.init?.headers);
    expect(headers.get("x-openvietgis-csrf")).toBe("csrf-123");
    expect(calls[1]?.init?.credentials).toBe("include");
  });

  it("reuses the cached CSRF token and refreshes it once after a 403", async () => {
    const calls: string[] = [];
    let token = "t1", created = 0;
    const client = new ApiClient({
      baseUrl: "http://example.test/api/v1",
      fetchImpl: (async (input: RequestInfo | URL, init?: RequestInit) => {
        const path = new URL(String(input)).pathname;
        calls.push(path);
        if (path.endsWith("/auth/csrf")) return json({ token });
        if (new Headers(init?.headers).get("x-openvietgis-csrf") !== token) return json({ detail: "csrf" }, 403);
        created++;
        return json({ organization: { id: "o" + created }, owner_membership: { role: "owner" } }, 201);
      }) as typeof fetch
    });
    await loadCsrfToken(client);
    await loadCsrfToken(client);
    expect(calls.filter(p => p.endsWith("/auth/csrf"))).toHaveLength(1);
    token = "t2"; // session rotated
    await createOrganization(client, { name: "Org", slug: "org" });
    expect(created).toBe(1);
    expect(calls.filter(p => p.endsWith("/auth/csrf"))).toHaveLength(2);
  });
});