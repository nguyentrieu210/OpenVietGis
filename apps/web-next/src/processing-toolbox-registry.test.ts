import { describe, expect, it } from "vitest";
import type { ProcessingAlgorithm } from "@openvietgis/client-sdk";
import type { CoreCommand } from "./CoreCommandBar";
import {
  filterProcessingToolboxEntries,
  processingToolboxEntries,
  searchProcessingToolboxEntries
} from "./processing-toolbox-registry";

const command = (alias: string, disabled = false): CoreCommand => ({
  alias,
  label: alias,
  group: "Phân tích",
  disabled,
  run: () => undefined
});

const algorithm = (
  id: ProcessingAlgorithm["id"],
  overrides: Partial<ProcessingAlgorithm> = {}
): ProcessingAlgorithm => ({
  id,
  name: id,
  description: id + " vector analysis",
  provider_id: "openvietgis",
  provider_name: "OpenVietGIS Core",
  category: "vector",
  capabilities: ["vector.read", "geometry." + id],
  required_inputs: ["input_layer_id"],
  parameters_schema: {},
  ...overrides
});

describe("processing toolbox registry", () => {
  it("shows only commands advertised by the server registry", () => {
    const commands = [
      command("BUFFER"),
      command("CLIP"),
      command("DISSOLVE"),
      command("NDVI"),
      command("SLOPE"),
      command("TOOLBOX")
    ];
    const entries = processingToolboxEntries(commands, [
      algorithm("buffer"),
      algorithm("clip"),
      algorithm("dissolve")
    ]);

    expect(entries.map(entry => entry.command.alias)).toEqual(["BUFFER", "CLIP", "DISSOLVE"]);
  });

  it("fails closed when provider or capability metadata is missing", () => {
    const incomplete = { ...algorithm("buffer"), provider_id: "", capabilities: [] };
    expect(processingToolboxEntries([command("BUFFER")], [incomplete])).toEqual([]);
  });

  it("filters by provider, category, capability, and current availability", () => {
    const entries = processingToolboxEntries(
      [command("BUFFER"), command("CLIP", true), command("DISSOLVE")],
      [
        algorithm("buffer"),
        algorithm("clip", { provider_id: "postgis", provider_name: "PostGIS", category: "overlay" }),
        algorithm("dissolve")
      ]
    );

    expect(filterProcessingToolboxEntries(entries, { providerId: "postgis" }).map(x => x.command.alias))
      .toEqual(["CLIP"]);
    expect(filterProcessingToolboxEntries(entries, { category: "vector" }).map(x => x.command.alias))
      .toEqual(["BUFFER", "DISSOLVE"]);
    expect(filterProcessingToolboxEntries(entries, { capability: "geometry.dissolve" }).map(x => x.command.alias))
      .toEqual(["DISSOLVE"]);
    expect(filterProcessingToolboxEntries(entries, { availableOnly: true }).map(x => x.command.alias))
      .toEqual(["BUFFER", "DISSOLVE"]);
  });

  it("searches provider capability metadata and algorithm help", () => {
    const entries = processingToolboxEntries(
      [command("BUFFER"), command("CLIP")],
      [
        algorithm("buffer"),
        algorithm("clip", { description: "Clip input to an overlay polygon" })
      ]
    );

    expect(searchProcessingToolboxEntries(entries, "overlay").map(x => x.command.alias)).toEqual(["CLIP"]);
    expect(searchProcessingToolboxEntries(entries, "geometry.buffer").map(x => x.command.alias)).toEqual(["BUFFER"]);
    expect(searchProcessingToolboxEntries(entries, "PostGIS").map(x => x.command.alias)).toEqual([]);
  });
});
