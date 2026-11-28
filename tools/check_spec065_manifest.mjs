#!/usr/bin/env node
import { existsSync, readFileSync } from "node:fs";
import { isAbsolute, relative, resolve, sep } from "node:path";

const root = process.cwd();
const matrixPath = "specs/065-qgis-ux-parity/feature-matrix.md";
const tasksPath = "specs/065-qgis-ux-parity/tasks.md";
const manifestPath = "specs/065-qgis-ux-parity/evidence/manifest.json";
const errors = [];

function read(path) {
  try {
    return readFileSync(resolve(root, path), "utf8");
  } catch (error) {
    errors.push("Cannot read " + path + ": " + (error instanceof Error ? error.message : String(error)));
    return "";
  }
}

const matrix = read(matrixPath);
const tasks = read(tasksPath);
const manifestText = read(manifestPath);
let manifest;
try {
  manifest = JSON.parse(manifestText);
} catch (error) {
  errors.push("Invalid JSON in " + manifestPath + ": " + (error instanceof Error ? error.message : String(error)));
}

const matrixIds = [...matrix.matchAll(/^\|\s*([A-Z][A-Z0-9]*-\d{2})\s*\|/gm)].map((match) => match[1]);
const taskById = new Map();
const taskDone = new Map();
for (const line of tasks.split(/\r?\n/)) {
  const status = line.match(/^\s*-\s*\[([ xX])\]\s+(T\d{3})\b/);
  if (status) {
    if (taskDone.has(status[2])) errors.push("tasks.md contains duplicate task " + status[2]);
    taskDone.set(status[2], status[1].toLowerCase() === "x");
  }
  const match = line.match(/^\s*-\s*\[[ xX]\]\s+(T\d{3})\s+\[US\d+\]\s+\[([A-Z][A-Z0-9]*-\d{2})\]/);
  if (match) {
    if (taskById.has(match[2])) errors.push("tasks.md contains duplicate mapping for " + match[2]);
    taskById.set(match[2], match[1]);
  }
}

if (matrixIds.length !== new Set(matrixIds).size) errors.push("feature-matrix.md contains duplicate row IDs");
if (!manifest || manifest.schema_version !== 1 || !Array.isArray(manifest.rows)) {
  errors.push("manifest must use schema_version 1 and contain a rows array");
}

const rows = Array.isArray(manifest?.rows) ? manifest.rows : [];
const rowsById = new Map();
for (const row of rows) {
  if (!row || typeof row.id !== "string") {
    errors.push("manifest contains a row without a string id");
    continue;
  }
  if (rowsById.has(row.id)) errors.push("manifest contains duplicate row " + row.id);
  rowsById.set(row.id, row);
}
for (const id of matrixIds) {
  if (!rowsById.has(id)) errors.push("matrix row " + id + " has no manifest entry");
}
for (const id of rowsById.keys()) {
  if (!matrixIds.includes(id)) errors.push("manifest row " + id + " is absent from the matrix");
}

const counts = {};
let explicitTestMappings = 0;
for (const [id, row] of rowsById) {
  counts[row.status] = (counts[row.status] ?? 0) + 1;
  if (!taskById.has(id) || row.task !== taskById.get(id)) {
    errors.push(id + " does not map to the matching task in tasks.md");
  }

  if (!Array.isArray(row.candidate_paths) || row.candidate_paths.length === 0) {
    errors.push(id + " must list candidate_paths");
  } else {
    for (const candidate of row.candidate_paths) {
      if (typeof candidate !== "string" || candidate.length === 0 || candidate.includes("\\")) {
        errors.push(id + " contains an invalid candidate path");
        continue;
      }
      const absolute = resolve(root, candidate);
      const rel = relative(root, absolute);
      if (isAbsolute(candidate) || rel === ".." || rel.startsWith(".." + sep)) {
        errors.push(id + " candidate path escapes the repository: " + candidate);
      } else if (!existsSync(absolute)) {
        errors.push(id + " candidate path does not exist: " + candidate);
      }
    }
  }

  if (!["partial", "blocked", "deferred", "accepted"].includes(row.status)) {
    errors.push(id + " has unsupported status " + String(row.status));
  }
  if (row.status !== "accepted" && (typeof row.blocker !== "string" || !row.blocker.trim())) {
    errors.push(id + " needs an explicit blocker until accepted");
  }
  if (row.test_ids !== undefined) {
    if (!Array.isArray(row.test_ids) || row.test_ids.some((testId) => typeof testId !== "string" || !testId.trim())) {
      errors.push(id + " test_ids must be an array of non-empty strings");
    } else {
      explicitTestMappings += 1;
      for (const testId of row.test_ids) {
        const found = row.candidate_paths.some((candidate) => {
          try {
            return readFileSync(resolve(root, candidate), "utf8").includes(testId);
          } catch {
            return false;
          }
        });
        if (!found) errors.push(id + " test identifier is not present in its candidate files: " + testId);
      }
    }
  }
  if (row.status === "accepted") {
    const evidence = row.evidence;
    const required = ["qgis_version", "fixture", "steps", "expected", "actual", "reviewer"];
    const hasEvidence = (value) => typeof value === "string" ? value.trim().length > 0 : Array.isArray(value) && value.length > 0 && value.every((item) => typeof item === "string" && item.trim().length > 0);
    if (!evidence || required.some((key) => !hasEvidence(evidence[key]))) {
      errors.push(id + " accepted status requires QGIS version, fixture, steps, expected/actual results, and reviewer evidence");
    }
    if (!Array.isArray(row.test_ids) || row.test_ids.length === 0) {
      errors.push(id + " accepted status requires explicit test_ids");
    }
  }
}

const accepted = counts.accepted ?? 0;
const statuses = Object.entries(counts).map(([status, count]) => status + "=" + count).join(", ");
console.log(
  "Spec 065 manifest structure: " + rows.length + "/" + matrixIds.length +
  " rows; " + explicitTestMappings + " rows have explicit test IDs; statuses: " + statuses + "."
);
console.log("This structural check does not certify QGIS parity or mark any row accepted.");

if (process.argv.includes("--require-accepted")) {
  if (accepted !== matrixIds.length) errors.push("release gate requires all " + matrixIds.length + " rows accepted; currently " + accepted + " are accepted");
  // T100/T101 are release and deployment evidence, so they cannot precede this gate.
  const unfinished = Array.from({length:99}, (_, i) => "T" + String(i + 1).padStart(3, "0")).filter(id => taskDone.get(id) !== true);
  if (unfinished.length) errors.push("release gate requires pre-release tasks completed: " + unfinished.join(", "));
}
if (errors.length) {
  console.error("Spec 065 manifest validation failed:");
  for (const error of errors) console.error("- " + error);
  process.exitCode = 1;
}
