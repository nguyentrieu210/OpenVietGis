import type { components } from "./generated/import-export";
import { ApiClient, ApiError, projectApiPath } from "./http";

export type TransferFormat = components["schemas"]["TransferFormat"];
export type TransferJobKind = components["schemas"]["JobKind"];
export type TransferJobStatus = components["schemas"]["JobStatus"];
export type ArtifactState = components["schemas"]["ArtifactState"];
export type SourceAsset = components["schemas"]["SourceAsset"];
export type ValidationFinding = components["schemas"]["ValidationFinding"];
export type InspectedField = components["schemas"]["InspectedField"];
export type InspectedMember = components["schemas"]["InspectedMember"];
export type ConversionDisclosure = components["schemas"]["ConversionDisclosure"];
export type ImportInspectionReport = components["schemas"]["ImportInspectionReport"];
export type ImportInspectionResponse = components["schemas"]["ImportInspectionResponse"];
export type ImportInspectInput = components["schemas"]["ImportInspectInput"];
export type ImportConfirmInput = components["schemas"]["ImportConfirmInput"];
export type ExportPreviewInput = components["schemas"]["ExportPreviewInput"];
export type ExportPreviewResponse = components["schemas"]["ExportPreviewResponse"];
export type ExportConfirmInput = components["schemas"]["ExportConfirmInput"];
export type DerivedArtifact = components["schemas"]["DerivedArtifact"];
export type TransferJob = components["schemas"]["TransferJob"];
export type TransferJobList = components["schemas"]["TransferJobList"];

export class TransferApiError extends ApiError {
  constructor(
    status: number,
    response: Response,
    public readonly code: string,
    public readonly detail: string
  ) {
    super(status, response);
    this.name = "TransferApiError";
  }
}

function projectBase(organizationId: string, projectId: string): string {
  return projectApiPath(organizationId, projectId);
}

async function wrapTransfer<T>(call: () => Promise<T>): Promise<T> {
  try {
    return await call();
  } catch (error) {
    if (error instanceof ApiError) {
      const payload = await error.readPayload();
      const code = payload.code ?? "TRANSFER_ERROR";
      const detail = payload.message ?? payload.detail ?? error.message;
      throw new TransferApiError(error.status, error.response, code, detail);
    }
    throw error;
  }
}

export async function uploadImportAsset(
  client: ApiClient,
  organizationId: string,
  projectId: string,
  file: Blob,
  filename: string
): Promise<ImportInspectionResponse> {
  const form = new FormData();
  form.append("file", file, filename);
  return wrapTransfer(() =>
    client.request<ImportInspectionResponse>(
      `${projectBase(organizationId, projectId)}/imports/upload`,
      { method: "POST", body: form }
    )
  );
}

export async function inspectImportAsset(
  client: ApiClient,
  organizationId: string,
  projectId: string,
  input: ImportInspectInput
): Promise<ImportInspectionResponse> {
  return wrapTransfer(() =>
    client.request<ImportInspectionResponse>(
      `${projectBase(organizationId, projectId)}/imports/inspect`,
      { method: "POST", body: JSON.stringify(input) }
    )
  );
}

export async function confirmImport(
  client: ApiClient,
  organizationId: string,
  projectId: string,
  input: ImportConfirmInput
): Promise<TransferJob> {
  return wrapTransfer(() =>
    client.request<TransferJob>(
      `${projectBase(organizationId, projectId)}/imports/confirm`,
      { method: "POST", body: JSON.stringify(input) }
    )
  );
}

export async function previewExport(
  client: ApiClient,
  organizationId: string,
  projectId: string,
  input: ExportPreviewInput
): Promise<ExportPreviewResponse> {
  return wrapTransfer(() =>
    client.request<ExportPreviewResponse>(
      `${projectBase(organizationId, projectId)}/exports/preview`,
      { method: "POST", body: JSON.stringify(input) }
    )
  );
}

export async function confirmExport(
  client: ApiClient,
  organizationId: string,
  projectId: string,
  input: ExportConfirmInput
): Promise<TransferJob> {
  return wrapTransfer(() =>
    client.request<TransferJob>(
      `${projectBase(organizationId, projectId)}/exports/confirm`,
      { method: "POST", body: JSON.stringify(input) }
    )
  );
}

export async function listTransferJobs(
  client: ApiClient,
  organizationId: string,
  projectId: string,
  limit = 50
): Promise<TransferJobList> {
  return wrapTransfer(() =>
    client.request<TransferJobList>(
      `${projectBase(organizationId, projectId)}/jobs?limit=${encodeURIComponent(String(limit))}`
    )
  );
}

export async function getTransferJob(
  client: ApiClient,
  organizationId: string,
  projectId: string,
  jobId: string
): Promise<TransferJob> {
  return wrapTransfer(() =>
    client.request<TransferJob>(
      `${projectBase(organizationId, projectId)}/jobs/${encodeURIComponent(jobId)}`
    )
  );
}

export async function cancelTransferJob(
  client: ApiClient,
  organizationId: string,
  projectId: string,
  jobId: string
): Promise<TransferJob> {
  return wrapTransfer(() =>
    client.request<TransferJob>(
      `${projectBase(organizationId, projectId)}/jobs/${encodeURIComponent(jobId)}/cancel`,
      { method: "POST" }
    )
  );
}

export async function retryTransferJob(
  client: ApiClient,
  organizationId: string,
  projectId: string,
  jobId: string
): Promise<TransferJob> {
  return wrapTransfer(() =>
    client.request<TransferJob>(
      `${projectBase(organizationId, projectId)}/jobs/${encodeURIComponent(jobId)}/retry`,
      { method: "POST" }
    )
  );
}

export async function getDerivedArtifact(
  client: ApiClient,
  organizationId: string,
  projectId: string,
  artifactId: string
): Promise<DerivedArtifact> {
  return wrapTransfer(() =>
    client.request<DerivedArtifact>(
      `${projectBase(organizationId, projectId)}/artifacts/${encodeURIComponent(artifactId)}`
    )
  );
}

export function artifactDownloadUrl(
  organizationId: string,
  projectId: string,
  artifactId: string
): string {
  return `/api/v1${projectBase(organizationId, projectId)}/artifacts/${encodeURIComponent(artifactId)}/download`;
}

export async function downloadDerivedArtifact(
  client: ApiClient,
  organizationId: string,
  projectId: string,
  artifactId: string
): Promise<{ bytes: ArrayBuffer; sha256: string | null; contentType: string | null }> {
  return wrapTransfer(async () => {
    const response = await client.requestRaw(
      `${projectBase(organizationId, projectId)}/artifacts/${encodeURIComponent(artifactId)}/download`
    );
    return {
      bytes: await response.arrayBuffer(),
      sha256: response.headers.get("x-content-sha256"),
      contentType: response.headers.get("content-type")
    };
  });
}
