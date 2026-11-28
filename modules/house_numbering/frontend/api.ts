import { ApiClient } from "@openvietgis/client-sdk";

export type HouseNumberingRule = {
  id: string;
  version: string;
  status: "draft" | "authoritative";
  authority_ref: string | null;
  left_odd: boolean;
  suffix_style: "letters" | "dash";
  max_distance_m: number;
  revision: number;
};

export type HouseNumberingArea = {
  id: string;
  name: string;
  kind: "current" | "historic" | "custom";
  parent_id: string | null;
  linked_source_id: string | null;
  linked_feature_id: string | null;
  revision: number;
};

export type HouseNumberingRoute = {
  id: string;
  name: string;
  route_type: "street" | "alley";
  parent_route_id: string | null;
  prefix: string | null;
  linked_source_id: string;
  linked_feature_id: string;
  direction_state: "forward" | "reverse";
  source_label: string;
  status: "draft" | "confirmed";
  revision: number;
  building_count?: number;
  segments?: { source_id: string; feature_id: string; code?: string }[];
};

export type HouseNumberingBuilding = {
  id: string;
  route_id: string | null;
  area_id: string | null;
  parcel_ref: string | null;
  linked_source_id: string;
  linked_feature_id: string;
  eligibility: "unknown" | "eligible" | "ineligible";
  ineligibility_reason: string | null;
  plaque_status: "not_planned" | "planned" | "installed" | "replaced" | "removed";
  plaque_updated_at: string | null;
  state: string;
  revision: number;
};

export type HouseNumberingBuildingLineage = {
  id: string;
  operation_id: string;
  relation_kind: "split" | "merge";
  predecessor_building_id: string;
  successor_building_id: string;
  created_by_actor_id: string;
  created_at: string;
};

export type HouseNumberingInspection = {
  location: {
    measure_m: number;
    distance_m: number;
    side: "left" | "right" | null;
    crs_code: number;
  };
  suggestion: { number: string | null; reason: string; rule_version?: string };
  neighbors?: {
    before: Record<string, unknown> | null;
    after: Record<string, unknown> | null;
  };
  context?: {
    building: {
      id: string;
      parcel_ref: string | null;
      linked_source_id: string | null;
      linked_feature_id: string | null;
      core_feature_attributes: Record<string, unknown> | null;
    } | null;
    evidence: Array<Record<string, unknown>>;
  };
};

export type HouseNumberingAuditDetail = {
  route_id: string;
  route_name: string;
  existing: number;
  proposed: number;
  duplicate_count: number;
  parity_error_count: number;
  invalid_format_count: number;
  ordering_error_count: number;
  gap_count: number;
  gap_ranges: number;
  blocking_issues: number;
  duplicates: Array<{number:string;occupants:Array<{origin:string;building_id:string|null}>}>;
  parity_errors: Array<{number:string;building_id:string|null;side:string;origin:string}>;
  invalid_formats: Array<{number:string;building_id:string|null;origin:string}>;
  ordering_errors: Array<{side:string;before:string;after:string;before_building_id:string;after_building_id:string}>;
  gaps: Array<{side:string;from_number:string;to_number:string;unused_slots:number}>;
  cross_route_duplicates?: Array<{root_route_id:string;number:string;occupants:Array<{route_id:string;building_id:string|null;origin:string}>}>;
};

export type HouseNumberingAudit = {
  routes: number;
  existing: number;
  proposed: number;
  duplicate_count: number;
  parity_error_count: number;
  invalid_format_count: number;
  ordering_error_count: number;
  gap_count: number;
  gap_ranges: number;
  blocking_issues: number;
  can_save: boolean;
  cross_route_duplicate_count?: number;
  cross_route_duplicates?: Array<{root_route_id:string;number:string;occupants:Array<{route_id:string;building_id:string|null;origin:string}>}>;
  details: HouseNumberingAuditDetail[];
};

export type HouseNumberingPreview = {
  audit?: HouseNumberingAudit;
  id: string;
  operation_id: string;
  candidate_changes: Array<{
    building_id: string;
    route_id: string;
    old_number: string | null;
    number: string;
    side: string | null;
    measure: number;
    reason: string;
    result_number_id?: string;
  }>;
  warnings: string[];
  expires_at: string;
  applied_at: string | null;
};

export type HouseNumberingProposal = {
  id: string;
  scope_kind: "route" | "area";
  scope_id: string;
  rule_id: string;
  status: "draft" | "submitted" | "approved";
  revision: number;
  decision_comment: string | null;
};

export type HouseNumberingSurvey = {
  id: string;
  client_uuid: string;
  building_id: string | null;
  point_x: number | null;
  point_y: number | null;
  crs_code: number | null;
  gps_accuracy_m: number | null;
  gps_deviation_m: number | null;
  warnings: Array<Record<string, unknown>>;
  photo_artifact_ids: string[];
  condition: "house" | "vacant" | "removed" | "other";
  state: "draft" | "submitted" | "accepted" | "rejected";
  recorded_by_actor_id: string;
  submitted_by_actor_id: string | null;
  revision: number;
  created_at: string;
  updated_at: string;
};

export type HouseNumberingAssignment = {
  id: string;
  scope_kind: "area" | "route";
  scope_id: string;
  assignee_actor_id: string;
  target_count: number | null;
  starts_on: string;
  ends_on: string | null;
  revision: number;
};

export type HouseNumberingReconcileCase = {
  id: string;
  building_id: string | null;
  evidence_ids: string[];
  case_class:
    | "matched"
    | "number_mismatch"
    | "cad_certificate_mismatch"
    | "new_building"
    | "no_longer_exists"
    | "not_surveyed"
    | "certificate_unmatched";
  estimated: boolean;
  status: "open" | "resolved" | "ignored";
  resolution: string | null;
  revision: number;
};

export type HouseNumberingProgress = {
  positions: number;
  houses: number;
  vacant: number;
  other: number;
  completion: number | null;
  removed: number;
  submitted_pending: number;
};

export type HouseNumberingDocument = {
  id: string;
  building_id: string;
  doc_type: "inspection" | "certificate" | "house_form" | "plate";
  template_revision: string;
  status: "draft" | "official" | "void" | "reissue_required";
  artifact_id: string | null;
  revision: number;
  artifact?: {
    id: string;
    filename: string;
    content_type: string;
    byte_size: number;
    sha256: string;
  };
};

export type HouseNumberingExport = {
  artifact_id: string;
  filename: string;
  content_type: string;
  byte_size: number;
  sha256: string;
  record_count: number;
  include_private: boolean;
};

function base(orgId: string, projectId: string): string {
  return `/organizations/${encodeURIComponent(orgId)}/projects/${encodeURIComponent(
    projectId
  )}/modules/house-numbering`;
}

function jsonMutation<T>(
  client: ApiClient,
  path: string,
  method: "POST" | "PUT" | "PATCH" | "DELETE",
  body?: unknown
): Promise<T> {
  return client.request<T>(path, {
    method,
    ...(body === undefined ? {} : { body: JSON.stringify(body) })
  });
}

export function listHouseNumberingRules(client: ApiClient, orgId: string, projectId: string) {
  return client.request<{ items: HouseNumberingRule[] }>(`${base(orgId, projectId)}/rules`);
}
export function createHouseNumberingRule(
  client: ApiClient,
  orgId: string,
  projectId: string,
  body: {
    version: string;
    status: "draft" | "authoritative";
    authority_ref?: string | null;
    left_odd: boolean;
    suffix_style: "letters" | "dash";
    max_distance_m: number;
  }
) {
  return jsonMutation<HouseNumberingRule>(client, `${base(orgId, projectId)}/rules`, "POST", body);
}

export function listHouseNumberingAreas(client: ApiClient, orgId: string, projectId: string) {
  return client.request<{ items: HouseNumberingArea[] }>(`${base(orgId, projectId)}/areas`);
}
export function createHouseNumberingArea(
  client: ApiClient,
  orgId: string,
  projectId: string,
  body: {
    name: string;
    kind: "current" | "historic" | "custom";
    parent_id?: string | null;
    source_id?: string | null;
    feature_id?: string | null;
  }
) {
  return jsonMutation<HouseNumberingArea>(client, `${base(orgId, projectId)}/areas`, "POST", body);
}

export function updateHouseNumberingArea(
  client: ApiClient,
  orgId: string,
  projectId: string,
  areaId: string,
  body: {
    expected_revision: number;
    name: string;
    kind: "current" | "historic" | "custom";
    parent_id?: string | null;
    source_id?: string | null;
    feature_id?: string | null;
  }
) {
  return jsonMutation<HouseNumberingArea>(
    client,
    `${base(orgId, projectId)}/areas/${encodeURIComponent(areaId)}`,
    "PUT",
    body
  );
}

export function deleteHouseNumberingArea(
  client: ApiClient,
  orgId: string,
  projectId: string,
  areaId: string,
  expectedRevision: number
) {
  return client.requestWithoutBody(
    `${base(orgId, projectId)}/areas/${encodeURIComponent(areaId)}?expected_revision=${expectedRevision}`,
    { method: "DELETE" }
  );
}

export function listHouseNumberingRoutes(client: ApiClient, orgId: string, projectId: string) {
  return client.request<{ items: HouseNumberingRoute[] }>(`${base(orgId, projectId)}/routes`);
}
export function createHouseNumberingRoute(
  client: ApiClient,
  orgId: string,
  projectId: string,
  body: {
    name: string;
    route_type: "street" | "alley";
    parent_route_id?: string | null;
    prefix?: string | null;
    source_id: string;
    feature_id: string;
    direction_state: "forward" | "reverse";
    source_label: string;
    segments?: { source_id: string; feature_id: string; code?: string }[];
    status: "draft" | "confirmed";
  }
) {
  return jsonMutation<HouseNumberingRoute>(client, `${base(orgId, projectId)}/routes`, "POST", body);
}
export function updateHouseNumberingRoute(
  client: ApiClient,
  orgId: string,
  projectId: string,
  routeId: string,
  body: {
    expected_revision: number;
    name: string;
    direction_state: "forward" | "reverse";
    status: "draft" | "confirmed";
  }
) {
  return jsonMutation<HouseNumberingRoute>(
    client,
    `${base(orgId, projectId)}/routes/${encodeURIComponent(routeId)}`,
    "PUT",
    body
  );
}
export function deleteHouseNumberingRoute(
  client: ApiClient,
  orgId: string,
  projectId: string,
  routeId: string
) {
  return client.requestWithoutBody(
    `${base(orgId, projectId)}/routes/${encodeURIComponent(routeId)}`,
    { method: "DELETE" }
  );
}

export function listHouseNumberingBuildings(
  client: ApiClient,
  orgId: string,
  projectId: string,
  routeId?: string
) {
  const qs = routeId ? `?route_id=${encodeURIComponent(routeId)}` : "";
  return client.request<{ items: HouseNumberingBuilding[] }>(
    `${base(orgId, projectId)}/buildings${qs}`
  );
}
export type NumberingTarget = Pick<HouseNumberingBuilding, "id" | "route_id" | "parcel_ref" | "eligibility">;
export function listNumberingTargets(client: ApiClient, orgId: string, projectId: string, routeId: string, descendants: boolean) {
  const qs = new URLSearchParams({route_id: routeId, include_descendants: String(descendants)});
  return client.request<{items: NumberingTarget[]; total: number; truncated: boolean}>(`${base(orgId, projectId)}/numbering-targets?${qs}`);
}
export function createHouseNumberingBuilding(
  client: ApiClient,
  orgId: string,
  projectId: string,
  body: {
    route_id?: string | null;
    area_id?: string | null;
    parcel_ref?: string | null;
    source_id: string;
    feature_id: string;
    eligibility: "unknown" | "eligible" | "ineligible";
    ineligibility_reason?: string | null;
  }
) {
  return jsonMutation<HouseNumberingBuilding>(
    client,
    `${base(orgId, projectId)}/buildings`,
    "POST",
    body
  );
}
export function updateHouseNumberingBuilding(
  client: ApiClient,
  orgId: string,
  projectId: string,
  buildingId: string,
  body: {
    expected_revision: number;
    route_id?: string | null;
    area_id?: string | null;
    parcel_ref?: string | null;
    source_id: string;
    feature_id: string;
  }
) {
  return jsonMutation<HouseNumberingBuilding>(
    client,
    `${base(orgId, projectId)}/buildings/${encodeURIComponent(buildingId)}`,
    "PUT",
    body
  );
}

export function retireHouseNumberingBuilding(
  client: ApiClient,
  orgId: string,
  projectId: string,
  buildingId: string,
  expectedRevision: number
) {
  return client.requestWithoutBody(
    `${base(orgId, projectId)}/buildings/${encodeURIComponent(buildingId)}?expected_revision=${expectedRevision}`,
    { method: "DELETE" }
  );
}

export function setHouseNumberingEligibility(
  client: ApiClient,
  orgId: string,
  projectId: string,
  buildingId: string,
  body: {
    expected_revision: number;
    eligibility: "unknown" | "eligible" | "ineligible";
    reason?: string | null;
  }
) {
  return jsonMutation<HouseNumberingBuilding>(
    client,
    `${base(orgId, projectId)}/buildings/${encodeURIComponent(buildingId)}/eligibility`,
    "PATCH",
    body
  );
}

export function setHouseNumberingPlaqueStatus(
  client: ApiClient,
  orgId: string,
  projectId: string,
  buildingId: string,
  body: { expected_revision: number; status: HouseNumberingBuilding["plaque_status"] }
) {
  return jsonMutation<HouseNumberingBuilding>(
    client,
    `${base(orgId, projectId)}/buildings/${encodeURIComponent(buildingId)}/plaque`,
    "PATCH",
    body
  );
}

export function recordHouseNumberingBuildingLineage(
  client: ApiClient,
  orgId: string,
  projectId: string,
  body: {
    relation_kind: "split" | "merge";
    operation_id: string;
    predecessor_building_ids: string[];
    successor_building_ids: string[];
    expected_revisions: Record<string, number>;
  }
) {
  return jsonMutation<{
    operation_id: string;
    relation_kind: "split" | "merge";
    items: HouseNumberingBuildingLineage[];
  }>(client, `${base(orgId, projectId)}/buildings/lineage`, "POST", body);
}

export function listHouseNumberingBuildingLineage(
  client: ApiClient,
  orgId: string,
  projectId: string,
  buildingId: string
) {
  return client.request<{ items: HouseNumberingBuildingLineage[] }>(
    `${base(orgId, projectId)}/buildings/${encodeURIComponent(buildingId)}/lineage`
  );
}

export function inspectHouseNumberingLocation(
  client: ApiClient,
  orgId: string,
  projectId: string,
  body: {
    x: number;
    y: number;
    crs_code: number;
    route_id: string;
    rule_id: string;
    existing_building_id?: string | null;
  }
) {
  return jsonMutation<HouseNumberingInspection>(
    client,
    `${base(orgId, projectId)}/inspect`,
    "POST",
    body
  );
}
export function previewHouseNumbering(
  client: ApiClient,
  orgId: string,
  projectId: string,
  body: { building_ids: string[]; rule_id: string; operation_id: string }
) {
  return jsonMutation<HouseNumberingPreview>(
    client,
    `${base(orgId, projectId)}/previews`,
    "POST",
    body
  );
}
export function applyHouseNumberingPreview(
  client: ApiClient,
  orgId: string,
  projectId: string,
  previewId: string
) {
  return jsonMutation<{ items: Array<Record<string, unknown>> }>(
    client,
    `${base(orgId, projectId)}/previews/${encodeURIComponent(previewId)}/apply`,
    "POST"
  );
}

export function createHouseNumberingProposal(
  client: ApiClient,
  orgId: string,
  projectId: string,
  body: {
    scope_kind: "route" | "area";
    scope_id: string;
    rule_id: string;
    building_ids: string[];
  }
) {
  return jsonMutation<{ proposal: HouseNumberingProposal; items: Array<Record<string, unknown>> }>(
    client,
    `${base(orgId, projectId)}/proposals`,
    "POST",
    body
  );
}
export function getHouseNumberingProposal(
  client: ApiClient,
  orgId: string,
  projectId: string,
  proposalId: string
) {
  return client.request<{
    proposal: HouseNumberingProposal;
    items: Array<Record<string, unknown>>;
  }>(`${base(orgId, projectId)}/proposals/${encodeURIComponent(proposalId)}`);
}
export function transitionHouseNumberingProposal(
  client: ApiClient,
  orgId: string,
  projectId: string,
  proposalId: string,
  body: {
    expected_revision: number;
    action: "submit" | "withdraw" | "approve" | "reject";
    comment?: string | null;
  }
) {
  return jsonMutation<HouseNumberingProposal>(
    client,
    `${base(orgId, projectId)}/proposals/${encodeURIComponent(proposalId)}/transition`,
    "POST",
    body
  );
}

export async function importHouseNumberingEvidence(
  client: ApiClient,
  orgId: string,
  projectId: string,
  body: {
    file: File;
    source_format: string;
    origin: string;
    mapping: Record<string, string>;
    default_source_reference: string;
    source_crs?: number | null;
    default_confidence?: string;
  }
) {
  const form = new FormData();
  form.set("file", body.file);
  form.set("source_format", body.source_format);
  form.set("origin", body.origin);
  form.set("mapping_json", JSON.stringify(body.mapping));
  form.set("default_source_reference", body.default_source_reference);
  if (body.source_crs !== undefined && body.source_crs !== null) {
    form.set("source_crs", String(body.source_crs));
  }
  form.set("default_confidence", body.default_confidence ?? "observed");
  return client.request<{
    batch: Record<string, unknown>;
    replayed: boolean;
    rejections: Array<Record<string, unknown>>;
  }>(`${base(orgId, projectId)}/imports`, { method: "POST", body: form });
}
export function listHouseNumberingImports(client: ApiClient, orgId: string, projectId: string) {
  return client.request<{ items: Array<Record<string, unknown>> }>(
    `${base(orgId, projectId)}/imports`
  );
}
export function rebuildHouseNumberingReconciliation(
  client: ApiClient,
  orgId: string,
  projectId: string,
  body: { removed_building_ids?: string[]; estimated_building_ids?: string[] }
) {
  return jsonMutation<{ items: HouseNumberingReconcileCase[] }>(
    client,
    `${base(orgId, projectId)}/reconciliation/rebuild`,
    "POST",
    {
      removed_building_ids: body.removed_building_ids ?? [],
      estimated_building_ids: body.estimated_building_ids ?? []
    }
  );
}
export function listHouseNumberingReconciliation(
  client: ApiClient,
  orgId: string,
  projectId: string
) {
  return client.request<{ items: HouseNumberingReconcileCase[] }>(
    `${base(orgId, projectId)}/reconciliation/cases`
  );
}
export function resolveHouseNumberingCase(
  client: ApiClient,
  orgId: string,
  projectId: string,
  caseId: string,
  body: {
    expected_revision: number;
    status: "resolved" | "ignored";
    resolution: string;
    building_id?: string | null;
  }
) {
  return jsonMutation<HouseNumberingReconcileCase>(
    client,
    `${base(orgId, projectId)}/reconciliation/cases/${encodeURIComponent(caseId)}/resolve`,
    "POST",
    body
  );
}

export function syncHouseNumberingSurveys(
  client: ApiClient,
  orgId: string,
  projectId: string,
  operations: Array<Record<string, unknown>>,
  receiptVersion?: 2
) {
  return jsonMutation<{
    applied: string[];
    replayed: string[];
    conflicts: Array<Record<string, unknown>>;
    warnings: Array<Record<string, unknown>>;
    receipts?: import("./survey-outbox").SyncReceipt[];
  }>(
    client,
    `${base(orgId, projectId)}/survey/sync`,
    "POST",
    { operations, ...(receiptVersion ? { receipt_version: receiptVersion } : {}) }
  );
}
export function listHouseNumberingSurveys(
  client: ApiClient,
  orgId: string,
  projectId: string,
  state?: HouseNumberingSurvey["state"]
) {
  const qs = state ? `?state=${encodeURIComponent(state)}` : "";
  return client.request<{ items: HouseNumberingSurvey[] }>(
    `${base(orgId, projectId)}/surveys${qs}`
  );
}
export function reviewHouseNumberingSurvey(
  client: ApiClient,
  orgId: string,
  projectId: string,
  surveyId: string,
  body: { expected_revision: number; decision: "accepted" | "rejected" }
) {
  return jsonMutation<HouseNumberingSurvey>(
    client,
    `${base(orgId, projectId)}/surveys/${encodeURIComponent(surveyId)}/review`,
    "POST",
    body
  );
}
export function listHouseNumberingAssignments(client: ApiClient, orgId: string, projectId: string) {
  return client.request<{ items: HouseNumberingAssignment[] }>(
    `${base(orgId, projectId)}/assignments`
  );
}
export function createHouseNumberingAssignment(
  client: ApiClient,
  orgId: string,
  projectId: string,
  body: {
    scope_kind: "area" | "route";
    scope_id: string;
    assignee_actor_id: string;
    starts_on: string;
    ends_on?: string | null;
    target_count?: number | null;
  }
) {
  return jsonMutation<HouseNumberingAssignment>(
    client,
    `${base(orgId, projectId)}/assignments`,
    "POST",
    body
  );
}
export function getHouseNumberingProgress(client: ApiClient, orgId: string, projectId: string) {
  return client.request<HouseNumberingProgress>(`${base(orgId, projectId)}/progress`);
}
export function houseNumberingProgressReportUrl(orgId: string, projectId: string) {
  return `/api/v1${base(orgId, projectId)}/progress/report.xlsx`;
}

export function issueHouseNumberingDocument(
  client: ApiClient,
  orgId: string,
  projectId: string,
  body: {
    building_id: string;
    doc_type: "inspection" | "certificate" | "house_form" | "plate";
    output_format: "docx" | "pdf";
    template_revision: string;
    template_verified: boolean;
    replaces_document_id?: string | null;
  }
) {
  return jsonMutation<HouseNumberingDocument>(
    client,
    `${base(orgId, projectId)}/documents`,
    "POST",
    body
  );
}
export function listHouseNumberingDocuments(
  client: ApiClient,
  orgId: string,
  projectId: string,
  buildingId: string
) {
  return client.request<{ items: HouseNumberingDocument[] }>(
    `${base(orgId, projectId)}/buildings/${encodeURIComponent(buildingId)}/documents`
  );
}
export function houseNumberingDocumentDownloadUrl(
  orgId: string,
  projectId: string,
  documentId: string
) {
  return `/api/v1${base(orgId, projectId)}/documents/${encodeURIComponent(documentId)}/download`;
}

export function searchHouseNumbers(
  client: ApiClient,
  orgId: string,
  projectId: string,
  query: string
) {
  return client.request<{ items: Array<Record<string, unknown>> }>(
    `${base(orgId, projectId)}/search?query=${encodeURIComponent(query)}`
  );
}
export function createHouseNumberingExport(
  client: ApiClient,
  orgId: string,
  projectId: string,
  body: {
    fields: string[];
    output_format: "csv" | "geojson" | "dxf" | "shapefile_zip";
    include_private?: boolean;
    purpose?: string | null;
  }
) {
  return jsonMutation<HouseNumberingExport>(
    client,
    `${base(orgId, projectId)}/exports`,
    "POST",
    body
  );
}
export function houseNumberingExportDownloadUrl(
  orgId: string,
  projectId: string,
  artifactId: string
) {
  return `/api/v1${base(orgId, projectId)}/exports/${encodeURIComponent(artifactId)}/download`;
}

export async function uploadHouseNumberingSurveyPhoto(
  client: ApiClient,
  orgId: string,
  projectId: string,
  file: File
) {
  const form = new FormData();
  form.set("file", file);
  return client.request<{ id: string; content_type: string; byte_size: number; sha256: string }>(
    `${base(orgId, projectId)}/survey/photos`,
    { method: "POST", body: form }
  );
}

export function reviewHouseNumberingAssignment(
  client: ApiClient,
  orgId: string,
  projectId: string,
  assignmentId: string,
  body: { expected_revision: number; assignee_actor_id: string }
) {
  return jsonMutation<HouseNumberingAssignment>(
    client,
    `${base(orgId, projectId)}/assignments/${encodeURIComponent(assignmentId)}`,
    "PATCH",
    body
  );
}

export function issueHouseNumberingDocumentBatch(
  client: ApiClient,
  orgId: string,
  projectId: string,
  body: {
    building_ids: string[];
    doc_type: "inspection" | "certificate" | "house_form" | "plate";
    output_format: "docx" | "pdf";
    template_revision: string;
    template_verified: boolean;
  }
) {
  return jsonMutation<{ items: HouseNumberingDocument[] }>(
    client,
    `${base(orgId, projectId)}/documents/batch`,
    "POST",
    body
  );
}

export function searchHouseNumbersNear(
  client: ApiClient,
  orgId: string,
  projectId: string,
  coordinates: { x: number; y: number; crs_code: number; max_distance?: number; limit?: number }
) {
  const query = new URLSearchParams({
    x: String(coordinates.x),
    y: String(coordinates.y),
    crs_code: String(coordinates.crs_code),
    max_distance: String(coordinates.max_distance ?? 100),
    limit: String(coordinates.limit ?? 100)
  });
  return client.request<{ items: Array<Record<string, unknown>> }>(
    `${base(orgId, projectId)}/search?${query}`
  );
}

export type HouseNumberingPrivateHousehold = {
  id: string;
  building_id: string;
  purpose: string;
  consent: boolean;
  lawful_basis_ref: string | null;
  protected_data: Record<string, unknown>;
  expires_at: string | null;
  revision: number;
};

export function upsertHouseNumberingPrivateHousehold(
  client: ApiClient,
  orgId: string,
  projectId: string,
  body: {
    building_id: string;
    purpose: string;
    consent: boolean;
    lawful_basis_ref?: string | null;
    protected_data: Record<string, unknown>;
    expires_at?: string | null;
    record_id?: string | null;
    expected_revision?: number | null;
  }
) {
  return jsonMutation<HouseNumberingPrivateHousehold>(
    client,
    `${base(orgId, projectId)}/private-households`,
    "POST",
    body
  );
}

export function listHouseNumberingPrivateHouseholds(
  client: ApiClient,
  orgId: string,
  projectId: string,
  buildingId: string,
  purpose: string
) {
  return client.request<{ items: HouseNumberingPrivateHousehold[] }>(
    `${base(orgId, projectId)}/buildings/${encodeURIComponent(buildingId)}/private-households?purpose=${encodeURIComponent(purpose)}`
  );
}

export type HouseNumberingProposalListItem = HouseNumberingProposal & {
  rule_version: string;
  rule_status: "draft" | "authoritative";
  authority_ref: string | null;
  item_count: number;
  created_at: string;
  updated_at: string;
  submitted_at: string | null;
  decided_at: string | null;
};

export function listHouseNumberingProposals(
  client: ApiClient, orgId: string, projectId: string,
  options: { status?: "draft" | "submitted" | "approved"; offset?: number; limit?: number } = {}
) {
  const params = new URLSearchParams();
  if (options.status) params.set("status", options.status);
  params.set("offset", String(options.offset ?? 0));
  params.set("limit", String(options.limit ?? 100));
  return client.request<{items: HouseNumberingProposalListItem[]; next_offset: number | null}>(
    `${base(orgId, projectId)}/proposals?${params.toString()}`
  );
}

export function houseNumberingProposalPacketUrl(
  orgId: string, projectId: string, proposalId: string,
  kind: "decision" | "register" | "appendix" | "plates", options: {offset?: number; limit?: number} = {}
) {
  const params = new URLSearchParams();
  if (kind === "plates" || kind === "appendix") {
    params.set("offset", String(options.offset ?? 0));
    params.set("limit", String(options.limit ?? 24));
  }
  const suffix = params.size ? `?${params.toString()}` : "";
  return `/api/v1${base(orgId, projectId)}/proposals/${encodeURIComponent(proposalId)}/packets/${kind}${suffix}`;
}

// Spec 051: reviewer sign-off (and lock) of a reconciled hamlet or road.
export type SurveySignoff = {
  id: string;
  source_id: string;
  ap: string;
  ten_duong: string | null;
  point_count: number;
  signed_by_actor_id: string;
  signed_at: string;
  note: string | null;
};
export function listSurveySignoffs(client: ApiClient, orgId: string, projectId: string, sourceId: string) {
  return client.request<{ items: SurveySignoff[] }>(
    `${base(orgId, projectId)}/survey-signoffs?source_id=${encodeURIComponent(sourceId)}`
  );
}
export function signSurveyScope(
  client: ApiClient,
  orgId: string,
  projectId: string,
  body: { source_ids: string[]; ap: string; ten_duong?: string | null; note?: string | null }
) {
  return jsonMutation<{ items: SurveySignoff[] }>(client, `${base(orgId, projectId)}/survey-signoffs`, "POST", body);
}
export function revokeSurveySignoff(client: ApiClient, orgId: string, projectId: string, signoffId: string, reason: string) {
  return jsonMutation<SurveySignoff>(
    client,
    `${base(orgId, projectId)}/survey-signoffs/${encodeURIComponent(signoffId)}/revoke`,
    "POST",
    { reason }
  );
}

export type IssuanceState="recorded"|"printed"|"delivered"|"installed"|"void";
export type IssuanceRecord={id:string;proposal_id:string;building_id:string;number_id:string;route_id:string;number_text:string;route_name:string;authority_name:string;decision_ref:string;certificate_ref:string;effective_date:string;template_revision:string;recorded_at:string;replaces_issuance_id:string|null;replacement_reason:string|null};
export type IssuanceEvent={revision:number;state:IssuanceState;evidence_ref:string;occurred_at:string};
export type IssuanceDetail={record:IssuanceRecord;history:IssuanceEvent[]};
export type IssuanceInput={proposal_id:string;number_id:string;authority_name:string;decision_ref:string;certificate_ref:string;effective_date:string;template_revision:string};
export function recordHouseNumberingIssuance(client:ApiClient,org:string,project:string,input:IssuanceInput){
 return jsonMutation<IssuanceDetail>(client,base(org,project)+"/issuances","POST",input);
}
export function listHouseNumberingIssuances(client:ApiClient,org:string,project:string,building:string){
 return client.request<{items:(IssuanceRecord&{state:IssuanceState;revision:number})[]}>(base(org,project)+"/buildings/"+encodeURIComponent(building)+"/issuances");
}
export function getHouseNumberingIssuance(client:ApiClient,org:string,project:string,id:string){
 return client.request<IssuanceDetail>(base(org,project)+"/issuances/"+encodeURIComponent(id));
}
export function transitionHouseNumberingIssuance(client:ApiClient,org:string,project:string,id:string,input:{expected_revision:number;state:Exclude<IssuanceState,"recorded">;evidence_ref:string}){
 return jsonMutation<IssuanceDetail>(client,base(org,project)+"/issuances/"+encodeURIComponent(id)+"/transition","POST",input);
}
export function houseNumberingAddressLookupUrl(org:string,project:string,id:string){
 return "/api/v1"+base(org,project)+"/address-lookup/"+encodeURIComponent(id);
}

export function replaceHouseNumberingIssuance(client:ApiClient,org:string,project:string,id:string,input:Omit<IssuanceInput,"proposal_id"|"number_id">&{expected_revision:number;reason:string}){
 return jsonMutation<IssuanceDetail>(client,base(org,project)+"/issuances/"+encodeURIComponent(id)+"/replace","POST",input);
}

export type LifecycleKpi={active_buildings:number;eligible:number;unknown:number;ineligible:number;approved_buildings:number;registered_buildings:number;installed_buildings:number;unresolved_buildings:number};
export function getHouseNumberingLifecycleKpi(client:ApiClient,org:string,project:string){
 return client.request<LifecycleKpi>(base(org,project)+"/lifecycle-kpi");
}

export type EntranceState="observed"|"confirmed"|"rejected"|"retired";
export type BuildingEntrance={id:string;building_id:string;route_id:string;route_name:string;source_id:string;feature_id:string;evidence_ref:string;observed_number:string|null;state:EntranceState;revision:number;geometry_ready:boolean;choice_revision:number;primary_selected:boolean;primary_valid:boolean};
export type EntranceInput={route_id:string;source_id:string;feature_id:string;evidence_ref:string;observed_number:string|null};
export function listBuildingEntrances(client:ApiClient,org:string,project:string,id:string){
 return client.request<{items:BuildingEntrance[]}>(base(org,project)+"/buildings/"+encodeURIComponent(id)+"/entrances");
}
export function observeBuildingEntrance(client:ApiClient,org:string,project:string,id:string,input:EntranceInput){
 return jsonMutation<unknown>(client,base(org,project)+"/buildings/"+encodeURIComponent(id)+"/entrances","POST",input);
}
export function reviewBuildingEntrance(client:ApiClient,org:string,project:string,id:string,input:{expected_revision:number;state:Exclude<EntranceState,"observed">;reason:string}){
 return jsonMutation<unknown>(client,base(org,project)+"/entrances/"+encodeURIComponent(id)+"/review","POST",input);
}

export type LifecycleMetrics=LifecycleKpi&{surveyed_buildings:number;awaiting_review_buildings:number};
export type LifecycleGroup=LifecycleMetrics&{id:string|null;name:string};
export type LifecycleBreakdown={total:LifecycleMetrics;areas:LifecycleGroup[];routes:LifecycleGroup[]};
export type SurveyStaffMetric={actor_id:string;display_name:string;submitted_forms:number;accepted_forms:number;rejected_forms:number;pending_forms:number;observed_buildings:number;accepted_buildings:number};
export type SurveyStaffKpi={start_date:string;end_date_exclusive:string;distinct_observed_buildings:number;distinct_accepted_buildings:number;items:SurveyStaffMetric[]};
export function getHouseNumberingLifecycleBreakdown(client:ApiClient,org:string,project:string){
 return client.request<LifecycleBreakdown>(base(org,project)+"/lifecycle-breakdown");
}
export function getHouseNumberingSurveyStaffKpi(client:ApiClient,org:string,project:string,start:string,endExclusive:string){
 return client.request<SurveyStaffKpi>(base(org,project)+"/survey-staff-kpi?"+new URLSearchParams({start_date:start,end_date_exclusive:endExclusive}).toString());
}

export function chooseBuildingPrimaryEntrance(client:ApiClient,org:string,project:string,building:string,input:{entrance_id:string;expected_revision:number;reason:string}){
 return jsonMutation<{items:BuildingEntrance[]}>(client,base(org,project)+"/buildings/"+encodeURIComponent(building)+"/primary-entrance","POST",input);
}

export type ParcelRecord={id:string;reference:string;source_id:string;feature_id:string;geometry_ready:boolean;evidence_ref:string};
export type ParcelLinkState="observed"|"confirmed"|"rejected"|"detached";
export type ParcelLink={parcel_id:string;reference:string;source_id:string;feature_id:string;revision:number;state:ParcelLinkState;geometry_current:boolean;history:{revision:number;state:ParcelLinkState;evidence_ref:string;occurred_at:string}[]};
export function listParcels(client:ApiClient,org:string,project:string,offset:number){
 return client.request<{items:ParcelRecord[];next_offset:number|null}>(base(org,project)+"/parcels?limit=100&offset="+offset);
}
export function createParcel(client:ApiClient,org:string,project:string,input:{source_id:string;feature_id:string;reference:string;evidence_ref:string}){
 return jsonMutation<ParcelRecord>(client,base(org,project)+"/parcels","POST",input);
}
export function listParcelLinks(client:ApiClient,org:string,project:string,building:string){
 return client.request<{items:ParcelLink[]}>(base(org,project)+"/buildings/"+encodeURIComponent(building)+"/parcels");
}
export function transitionParcelLink(client:ApiClient,org:string,project:string,building:string,parcel:string,input:{expected_revision:number;state:ParcelLinkState;evidence_ref:string}){
 return jsonMutation<{items:ParcelLink[]}>(client,base(org,project)+"/buildings/"+encodeURIComponent(building)+"/parcels/"+encodeURIComponent(parcel)+"/transition","POST",input);
}
export function stableAddressLookupUrl(org:string,project:string,number:string){
 return "/api/v1"+base(org,project)+"/addresses/"+encodeURIComponent(number)+"/lookup";
}

export type AddressHistoryItem={id:string;number_text:string;route_id:string;route_name:string;revision:number;retired_at:string|null;retirement:{effective_date:string;decision_ref:string;reason:string}|null;successor:{new_number_id:string;effective_date:string;decision_ref:string}|null};
export function listAddressHistory(client:ApiClient,org:string,project:string,building:string){
 return client.request<{items:AddressHistoryItem[]}>(base(org,project)+"/buildings/"+encodeURIComponent(building)+"/address-history");
}
export function retireAddress(client:ApiClient,org:string,project:string,id:string,input:{expected_revision:number;effective_date:string;decision_ref:string;reason:string}){
 return jsonMutation<unknown>(client,base(org,project)+"/addresses/"+encodeURIComponent(id)+"/retire","POST",input);
}
export function linkAddressSuccessor(client:ApiClient,org:string,project:string,id:string,input:{new_number_id:string;proposal_id:string;effective_date:string;decision_ref:string}){
 return jsonMutation<unknown>(client,base(org,project)+"/addresses/"+encodeURIComponent(id)+"/successor","POST",input);
}

export type ScoreStatus="khop"|"lech_vi_tri"|"sai_so"|"khong_co"|"thieu_so"|"chua_vi_tri";
export type ScorePreviewInput={point_source:string;grid_source:string|null;point_number_field:string;point_road_field:string;grid_number_field:string;grid_road_field:string;metric_crs:number;offset?:number;status?:ScoreStatus};
export type ScorePreview={total:number;counts:Record<ScoreStatus,number>;duplicate_points:number;grid_usable:number;filtered_total:number;next_offset:number|null;point_revision:number;grid_revision:number|null;grid_mode:"source"|"registry";ambiguous_grid_buildings:number;items:{id:string;number:string|null;road:string|null;status:ScoreStatus;grid_id:string|null;grid_number:string|null;grid_source_id:string|null;grid_feature_id:string|null;grid_origin:string|null;distance_m:number|null;duplicate_peers:number}[]};
export function previewScoreReview(client:ApiClient,org:string,project:string,input:ScorePreviewInput){
 return jsonMutation<ScorePreview>(client,base(org,project)+"/score-review/preview","POST",input);
}

export type ReconcileSummary={active_buildings:number;unmatched_certificates:number;total_cases:number;counts:Record<HouseNumberingReconcileCase["case_class"],number>};
export function getReconcileSummary(client:ApiClient,org:string,project:string){
 return client.request<ReconcileSummary>(base(org,project)+"/reconciliation/summary");
}

export type CertificateLocation={evidence_id:string;number:string|null;status:string;lower_number:number|null;upper_number:number|null;lower_building_id:string|null;upper_building_id:string|null;longitude:number|null;latitude:number|null;confidence:"estimated"};
export function listCertificateLocations(client:ApiClient,org:string,project:string,route:string,crs:number,offset:number){
 return client.request<{items:CertificateLocation[];next_offset:number|null}>(base(org,project)+"/certificate-locations?"+new URLSearchParams({route_id:route,metric_crs:String(crs),offset:String(offset)}));
}
