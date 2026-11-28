import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const smokePath = fileURLToPath(new URL("./qgis-parity-keyboard-smoke.mjs", import.meta.url));
const env = { ...process.env, OVG_SHELL_ONLY: "1" };
// Reuse the canonical runner\x27s loopback preview when supplied; standalone runs still start their own.
// The child validates the loopback hostname before navigating.

const result = spawnSync(
  process.execPath,
  [smokePath],
  { env, stdio: "inherit" }
);
if (result.error) throw result.error;
process.exitCode = result.status ?? 1;
