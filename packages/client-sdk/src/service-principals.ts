import type { components, operations } from "./generated/service-admin";
import { ApiClient } from "./http";

export type ServicePrincipal = components["schemas"]["ServicePrincipal"];
export type ServiceCredentialIssued =
  components["schemas"]["ServiceCredentialIssued"];
export type CreateServicePrincipalInput =
  operations["createServicePrincipal"]["requestBody"]["content"]["application/json"];
export type IssueServiceCredentialInput =
  operations["issueServiceCredential"]["requestBody"]["content"]["application/json"];

function principalCollectionPath(organizationId: string): string {
  return (
    "/organizations/" +
    encodeURIComponent(organizationId) +
    "/service-principals"
  );
}

function principalPath(organizationId: string, principalId: string): string {
  return (
    principalCollectionPath(organizationId) +
    "/" +
    encodeURIComponent(principalId)
  );
}

export function listServicePrincipals(
  client: ApiClient,
  organizationId: string
): Promise<ServicePrincipal[]> {
  return client.request<ServicePrincipal[]>(principalCollectionPath(organizationId));
}

export function createServicePrincipal(
  client: ApiClient,
  organizationId: string,
  input: CreateServicePrincipalInput
): Promise<ServicePrincipal> {
  return client.request<ServicePrincipal>(principalCollectionPath(organizationId), {
    method: "POST",
    body: JSON.stringify(input)
  });
}

export function revokeServicePrincipal(
  client: ApiClient,
  organizationId: string,
  principalId: string
): Promise<void> {
  return client.requestWithoutBody(
    principalPath(organizationId, principalId) + "/revoke",
    { method: "POST" }
  );
}

export type ServiceCredentialMetadata = components["schemas"]["ServiceCredentialMetadata"];

export function listServiceCredentials(
  client: ApiClient,
  organizationId: string,
  principalId: string
): Promise<ServiceCredentialMetadata[]> {
  return client.request<ServiceCredentialMetadata[]>(
    principalPath(organizationId, principalId) + "/credentials"
  );
}

export function issueServiceCredential(
  client: ApiClient,
  organizationId: string,
  principalId: string,
  input: IssueServiceCredentialInput
): Promise<ServiceCredentialIssued> {
  return client.request<ServiceCredentialIssued>(
    principalPath(organizationId, principalId) + "/credentials",
    { method: "POST", body: JSON.stringify(input) }
  );
}

export function revokeServiceCredential(
  client: ApiClient,
  organizationId: string,
  principalId: string,
  credentialId: string
): Promise<void> {
  return client.requestWithoutBody(
    principalPath(organizationId, principalId) +
      "/credentials/" +
      encodeURIComponent(credentialId) +
      "/revoke",
    { method: "POST" }
  );
}
