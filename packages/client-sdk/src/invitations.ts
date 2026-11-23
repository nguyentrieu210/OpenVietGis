import type { components, operations } from "./generated/tenant-core";
import { ApiClient } from "./http";

export type InvitationCreate = components["schemas"]["InvitationCreate"];
export type InvitationIssued = components["schemas"]["InvitationIssued"];
export type InvitationResourceGrant = components["schemas"]["InvitationResourceGrant"];
export type InvitationAccept = operations["acceptInvitation"]["requestBody"]["content"]["application/json"];
export type InvitationAcceptResult = operations["acceptInvitation"]["responses"][200]["content"]["application/json"];

export function createInvitation(client: ApiClient, organizationId: string, input: InvitationCreate): Promise<InvitationIssued> {
  return client.request<InvitationIssued>("/organizations/" + encodeURIComponent(organizationId) + "/invitations", {
    method: "POST",
    body: JSON.stringify(input)
  });
}

export type EmailInvitationCreate = operations["emailInvitation"]["requestBody"]["content"]["application/json"];
export type EmailInvitationIssued = operations["emailInvitation"]["responses"][201]["content"]["application/json"];
export function emailInvitation(client: ApiClient, organizationId: string, input: EmailInvitationCreate): Promise<EmailInvitationIssued> {
  return client.request<EmailInvitationIssued>("/organizations/" + encodeURIComponent(organizationId) + "/invitations/email", {
    method: "POST", body: JSON.stringify(input)
  });
}

export function revokeInvitation(client: ApiClient, organizationId: string, invitationId: string): Promise<void> {
  return client.requestWithoutBody("/organizations/" + encodeURIComponent(organizationId) + "/invitations/" + encodeURIComponent(invitationId) + "/revoke", {
    method: "POST"
  });
}

export function acceptInvitation(client: ApiClient, input: InvitationAccept): Promise<InvitationAcceptResult> {
  return client.request<InvitationAcceptResult>("/invitations/accept", {
    method: "POST",
    body: JSON.stringify(input)
  });
}
