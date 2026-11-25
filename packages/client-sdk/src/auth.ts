import { ApiClient } from "./http";

export interface IdentityProviderSummary {
  id: string;
  display_name: string;
}

export interface CurrentUser {
  id: string;
  display_name: string;
  email_snapshot: string | null;
  status: "active" | "disabled";
}

export interface IdentityLink {
  id: string;
  issuer: string;
  subject: string;
  email_snapshot: string | null;
  email_verified: boolean;
}

export function listIdentityProviders(client: ApiClient): Promise<IdentityProviderSummary[]> {
  return client.request<IdentityProviderSummary[]>("/auth/providers");
}

/** Cached per client for the session; ApiClient refreshes it once on a 403. */
export async function loadCsrfToken(client: ApiClient): Promise<string> {
  const cached = client.getCsrfToken();
  if (cached) return cached;
  const response = await client.request<{ token: string }>("/auth/csrf");
  client.setCsrfToken(response.token);
  return response.token;
}

export function getCurrentUser(client: ApiClient): Promise<CurrentUser> {
  return client.request<CurrentUser>("/me", { cache: "no-store" });
}

export function listIdentityLinks(client: ApiClient): Promise<IdentityLink[]> {
  return client.request<IdentityLink[]>("/me/identity-links");
}

export async function unlinkIdentity(client: ApiClient, identityLinkId: string): Promise<void> {
  await client.request<void>("/me/identity-links", {
    method: "DELETE",
    body: JSON.stringify({ identity_link_id: identityLinkId })
  });
}

export async function logout(client: ApiClient): Promise<void> {
  await client.request<void>("/auth/logout", { method: "POST" });
}
