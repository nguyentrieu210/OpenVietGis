import type { ProcessingAlgorithm } from "@openvietgis/client-sdk";
import type { CoreCommand } from "./CoreCommandBar";
import { searchCommands } from "./command-search";

export interface ProcessingToolboxEntry {
  command: CoreCommand;
  algorithm: ProcessingAlgorithm;
}

const commandByAlgorithm: Readonly<Record<ProcessingAlgorithm["id"], string>> = {
  buffer: "BUFFER",
  clip: "CLIP",
  dissolve: "DISSOLVE"
};

const normalize = (value: string) =>
  value.normalize("NFD").replace(/[\u0300-\u036f]/g, "").replace(/đ/g, "d").toLowerCase();

function hasProviderAndCapabilities(algorithm: ProcessingAlgorithm): boolean {
  return typeof algorithm.provider_id === "string" &&
    algorithm.provider_id.trim().length > 0 &&
    typeof algorithm.provider_name === "string" &&
    algorithm.provider_name.trim().length > 0 &&
    typeof algorithm.category === "string" &&
    algorithm.category.trim().length > 0 &&
    Array.isArray(algorithm.capabilities) &&
    algorithm.capabilities.length > 0 &&
    algorithm.capabilities.every(capability => typeof capability === "string" && capability.trim().length > 0);
}

/** Intersect executable UI commands with the server's authenticated processing registry. */
export function processingToolboxEntries(
  commands: CoreCommand[],
  algorithms: ProcessingAlgorithm[]
): ProcessingToolboxEntry[] {
  const commandsByAlias = new Map(
    commands
      .filter(command => command.group === "Phân tích")
      .map(command => [command.alias.toUpperCase(), command] as const)
  );
  const seen = new Set<ProcessingAlgorithm["id"]>();
  const entries: ProcessingToolboxEntry[] = [];

  for (const algorithm of algorithms) {
    if (seen.has(algorithm.id) || !hasProviderAndCapabilities(algorithm)) continue;
    const alias = commandByAlgorithm[algorithm.id];
    const command = commandsByAlias.get(alias);
    if (!command) continue;
    seen.add(algorithm.id);
    entries.push({ command, algorithm });
  }

  return entries;
}

export function filterProcessingToolboxEntries(
  entries: ProcessingToolboxEntry[],
  filter: { providerId?: string; category?: string; capability?: string; availableOnly?: boolean }
): ProcessingToolboxEntry[] {
  return entries.filter(({ command, algorithm }) =>
    (!filter.providerId || algorithm.provider_id === filter.providerId) &&
    (!filter.category || algorithm.category === filter.category) &&
    (!filter.capability || algorithm.capabilities.includes(filter.capability)) &&
    (!filter.availableOnly || !command.disabled)
  );
}

export function searchProcessingToolboxEntries(
  entries: ProcessingToolboxEntry[],
  query: string
): ProcessingToolboxEntry[] {
  const term = normalize(query.trim());
  if (!term) return entries;

  const commands = searchCommands(entries.map(entry => entry.command), query, {}, entries.length);
  const commandMatches = new Set(commands.map(command => command.alias));
  return entries.filter(({ command, algorithm }) =>
    commandMatches.has(command.alias) ||
    normalize([
      algorithm.name,
      algorithm.description,
      algorithm.provider_id,
      algorithm.provider_name,
      algorithm.category,
      ...algorithm.capabilities
    ].join(" ")).includes(term)
  );
}
