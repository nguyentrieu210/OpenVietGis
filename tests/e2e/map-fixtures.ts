import { expect, type Page } from "@playwright/test";
import { signInIdentity } from "./frontend-auth-helpers";
import type {
  MapConfiguration, MapViewpoint, ProjectMap, SaveMapInput, MapSaveReceipt,
  SpatialLayer, SpatialSource
} from "../../packages/client-sdk/src";

export const normalMapViewpoint: MapViewpoint = {
  longitude: 106.604, latitude: 10.704, zoom: 15, bearing: 0, pitch: 0, crs: 4326
};
export const referenceFamilies = [
  { key: "reference_points_4326", name: "Points", count: 4000, color: "#e11d48" },
  { key: "reference_lines_4326", name: "Lines", count: 3000, color: "#2563eb" },
  { key: "reference_polygons_4326", name: "Polygons", count: 3000, color: "#16a34a" }
] as const;

export async function mapSignIn(page: Page, member = false): Promise<void> {
  const username = member ? "member-a" : "owner-a";
  const password = member ? "openvietgis-test-member-a" : "openvietgis-test-owner-a";
  await signInIdentity(page, username, password);
  await expect(
    page.locator(".ovg-account-name").filter({
      hasText: member ? "Member Alpha" : "Owner Alpha"
    })
  ).toBeVisible();
}

/** The CSRF token remains inside the browser and is never attached to test evidence. */
export async function mapMutation<T>(page: Page, path: string, method: "POST" | "PUT" | "PATCH", body?: unknown) {
  return page.evaluate(async ({ path, method, body }) => {
    const csrfResponse = await fetch("/api/v1/auth/csrf");
    if (!csrfResponse.ok) throw new Error("Unable to obtain current mutation context");
    const csrf = await csrfResponse.json() as { token: string };
    const response = await fetch(path, {
      method, headers: { "content-type": "application/json", "x-openvietgis-csrf": csrf.token },
      ...(body === undefined ? {} : { body: JSON.stringify(body) })
    });
    const content = await response.text();
    return { status: response.status, body: content ? JSON.parse(content) as T : null };
  }, { path, method, body });
}

export async function mapRead<T>(page: Page, path: string): Promise<T> {
  const response = await page.request.get(path);
  expect(response.status(), "authorized fixture read").toBe(200);
  return await response.json() as T;
}

export interface MapFixture {
  organizationId: string;
  projectId: string;
  root: string;
  api: string;
  sources: SpatialSource[];
  layers: SpatialLayer[];
}

export async function createMapProject(page: Page, suffix: string): Promise<MapFixture> {
  const result = await mapMutation<{ organization: { id: string } }>(page, "/api/v1/organizations", "POST", {
    name: "Map team " + suffix, slug: "map-team-" + suffix
  });
  expect(result.status).toBe(201);
  const organizationId = result.body!.organization.id;
  const project = await mapMutation<{ id: string }>(
    page, "/api/v1/organizations/" + organizationId + "/projects", "POST",
    { name: "Map project " + suffix, slug: "map-project-" + suffix }
  );
  expect(project.status).toBe(201);
  const projectId = project.body!.id;
  const apiRoot = "/organizations/" + organizationId + "/projects/" + projectId;
  const root = "/app" + apiRoot;
  return { organizationId, projectId, root, api: "/api/v1" + apiRoot, sources: [], layers: [] };
}

export async function registerMapReferences(page: Page, fixture: MapFixture, allFamilies = true): Promise<void> {
  for (const definition of allFamilies ? referenceFamilies : referenceFamilies.slice(0, 1)) {
    const source = await mapMutation<{ resource_id: string }>(page, fixture.api + "/spatial/sources", "POST", {
      reference_key: definition.key, name: definition.name
    });
    expect(source.status).toBe(201);
    const sourceDetail = await mapRead<SpatialSource>(page, fixture.api + "/spatial/sources/" + source.body!.resource_id);
    expect(sourceDetail.feature_count).toBe(definition.count);
    expect(sourceDetail.validation_state).toBe("ready");
    const layer = await mapMutation<{ resource_id: string }>(page, fixture.api + "/layers", "POST", {
      source_id: sourceDetail.id, name: definition.name
    });
    expect(layer.status).toBe(201);
    const layerDetail = await mapRead<SpatialLayer>(page, fixture.api + "/layers/" + layer.body!.resource_id);
    fixture.sources.push(sourceDetail);
    fixture.layers.push(layerDetail);
  }
}

export function fixtureConfiguration(fixture: MapFixture): MapConfiguration {
  return {
    layers: [...fixture.layers].reverse().map(layer => ({
      layer_id: layer.id, visible: true,
      style: {
        color: referenceFamilies.find(family => family.name === layer.name)?.color ?? "#334455",
        opacity: 0.8, width: 2, radius: 6
      }
    })),
    basemap: "grid", default_viewpoint: normalMapViewpoint
  };
}

export async function saveFixtureMap(page: Page, fixture: MapFixture): Promise<void> {
  const configuration = fixtureConfiguration(fixture);
  const input: SaveMapInput = {
    operation_id: crypto.randomUUID(), expected_revision: 0,
    layers: configuration.layers, basemap: "grid",
    default_viewpoint: { mode: "set", value: normalMapViewpoint }
  };
  const result = await mapMutation<MapSaveReceipt>(page, fixture.api + "/map", "PUT", input);
  expect(result.status).toBe(200);
  expect(result.body!.configuration_revision).toBe(1);
}

/** Requires real renderer-idle state after applying the latest authorized viewport payloads. */
export async function expectUsableMap(page: Page): Promise<void> {
  await expect(page.getByTestId("map-workspace")).toBeVisible();
  await expect(page.getByTestId("map-engine-ready")).toHaveAttribute("data-ready", "true");
  await expect(page.getByTestId("map-status")).toHaveAttribute("data-state", "usable");
}

export async function openFixtureMap(page: Page, fixture: MapFixture): Promise<void> {
  await page.goto(fixture.root + "/map");
  await expectUsableMap(page);
}

export async function currentMap(page: Page, fixture: MapFixture): Promise<ProjectMap> {
  return mapRead<ProjectMap>(page, fixture.api + "/map");
}

export async function grantMapCollaborator(owner: Page, recipient: Page, fixture: MapFixture, role: "editor" | "viewer") {
  // This real one-time invitation is used in memory only. No network/DOM/trace snapshots are retained.
  const invitation = await mapMutation<{ id: string; secret: string }>(
    owner, "/api/v1/organizations/" + fixture.organizationId + "/invitations", "POST", {
      target: { kind: "email", email: "member-a@example.test" }, organization_role: "member",
      expires_at: new Date(Date.now() + 3_600_000).toISOString(),
      resource_grant: { scope_kind: "project", project_id: fixture.projectId, role }
    }
  );
  if (invitation.status !== 201 || !invitation.body?.secret) throw new Error("Synthetic collaborator invitation failed");
  const accepted = await mapMutation<{ grant: { id: string } | null }>(recipient, "/api/v1/invitations/accept", "POST", {
    organization_id: fixture.organizationId, invitation_id: invitation.body.id, secret: invitation.body.secret
  });
  if (accepted.status !== 200 || !accepted.body?.grant) throw new Error("Synthetic collaborator acceptance failed");
  return accepted.body.grant.id;
}
