import { describe, expect, it } from "vitest";
import { ApiClient } from "../src/http";
import {
  changeQgisBindingDraftLifecycle,
  createQgisBindingDraft,
  listIntegrationBindingDrafts,
  listQgisBindingCandidates,
  type CreateQgisBindingDraft,
  type IntegrationBindingDraft
} from "../src/integration-hub";

function mock() {
  const calls: { url: string; init: RequestInit }[] = [];
  const binding: IntegrationBindingDraft = {
    id: "binding-1", layer_id: "layer-1", adapter_key: "qgis",
    external_dataset_ref: "landmap", direction: "bidirectional_manual",
    source_crs_code: 5899, target_crs_code: 5899,
    external_primary_key: "ovg_feature_id", allowed_field_ids: ["parcel"],
    base_content_revision: "a".repeat(64), base_schema_revision: "b".repeat(64),
    state: "draft", revision: 1, created_at: "2026-11-23T00:00:00Z",
    updated_at: "2026-11-23T00:00:00Z",
    writes_allowed: false, deletes_allowed: false, connected: false,
    reason: "DRAFT_ONLY_AGENT_NOT_PAIRED"
  };
  const client = new ApiClient({
    baseUrl: "https://gis.example/api/v1",
    fetchImpl: (async (input, init) => {
      calls.push({ url: String(input), init: init ?? {} });
      return new Response(JSON.stringify(String(input).endsWith("/bindings") &&
        (init?.method === "GET") ? { items: [binding] } : binding), {
        status: init?.method === "POST" ? 201 : 200,
        headers: { "content-type": "application/json" }
      });
    }) as typeof fetch
  });
  client.setCsrfToken("csrf-token");
  return { client, calls, binding };
}

const payload: CreateQgisBindingDraft = {
  layer_id: "layer-1", adapter_key: "qgis",
  external_dataset_ref: "landmap", direction: "bidirectional_manual",
  source_crs_code: 5899, target_crs_code: 5899,
  external_primary_key: "ovg_feature_id", allowed_field_ids: ["parcel"]
};

describe("R027 draft-only QGIS binding client", () => {
  it("lists project drafts without a write method or bearer token", async () => {
    const { client, calls, binding } = mock();
    const result = await listIntegrationBindingDrafts(client, "org/path", "proj?x");
    expect(result.items).toEqual([binding]);
    expect(calls[0]!.url).toContain("/organizations/org%2Fpath/projects/proj%3Fx/integration-hub/bindings");
    expect(calls[0]!.init.method).toBe("GET");
    expect(new Headers(calls[0]!.init.headers).has("authorization")).toBe(false);
  });

  it("fetches project-scoped eligible QGIS fields only", async () => {
    const { client, calls } = mock();
    await listQgisBindingCandidates(client, "org", "proj");
    expect(calls[0]!.url).toContain(
      "/organizations/org/projects/proj/integration-hub/bindings/candidates"
    );
    expect(calls[0]!.init.method).toBe("GET");
  });

  it("creates only disabled draft metadata with CSRF, never credentials", async () => {
    const { client, calls } = mock();
    const binding = await createQgisBindingDraft(client, "org", "proj", payload);
    expect(binding.connected).toBe(false);
    expect(binding.writes_allowed).toBe(false);
    const sent = calls[0]!;
    expect(sent.init.method).toBe("POST");
    expect(new Headers(sent.init.headers).get("x-openvietgis-csrf")).toBe("csrf-token");
    expect(JSON.parse(String(sent.init.body))).toEqual(payload);
    expect(JSON.stringify(sent.init)).not.toMatch(/password|bearer_token/i);
  });

  it("pauses with bound revision, never offers activate", async () => {
    const { client, calls } = mock();
    await changeQgisBindingDraftLifecycle(client, "org", "proj", "binding/id", "pause", 3);
    expect(calls[0]!.url).toContain("/bindings/binding%2Fid/lifecycle");
    expect(JSON.parse(String(calls[0]!.init.body))).toEqual({
      action: "pause", expected_revision: 3
    });
    await expect(changeQgisBindingDraftLifecycle(
      client, "org", "proj", "binding", "revoke", 0
    )).rejects.toThrow(/positive integer/);
    expect(calls).toHaveLength(1);
  });
});
