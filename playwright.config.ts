import { defineConfig } from "@playwright/test";
import { execPath } from "node:process";

function loopbackPort(name: string, fallback: number, legacyName: string): number {
  const current = process.env[name];
  const legacy = process.env[legacyName];
  if (current && legacy && current !== legacy) {
    throw new Error(`${name} and ${legacyName} disagree`);
  }
  const raw = current ?? legacy ?? String(fallback);
  if (!/^\d{1,5}$/.test(raw) || Number(raw) < 1024 || Number(raw) > 65535) {
    throw new Error(`${name} must be a valid unprivileged loopback port`);
  }
  return Number(raw);
}

export function makeConfig(bootstrap = false) {
  const apiPort = loopbackPort("OPENVIETGIS_E2E_API_PORT", 8000, "OVG_E2E_API_PORT");
  const webPort = loopbackPort("OPENVIETGIS_E2E_WEB_PORT", 5178, "OVG_E2E_WEB_PORT");
  const oidcPort = loopbackPort("OPENVIETGIS_E2E_OIDC_PORT", 18080, "OVG_E2E_KEYCLOAK_PORT");
  if (new Set([apiPort, webPort, oidcPort]).size !== 3) {
    throw new Error("E2E ports must be distinct");
  }
  const apiBase = `http://127.0.0.1:${apiPort}`;
  const webBase = `http://127.0.0.1:${webPort}`;
  const issuer = `http://127.0.0.1:${oidcPort}/realms/openvietgis`;
  return defineConfig({
  testDir: "./tests/e2e",
  // Reports are scanned as security evidence; a PR diff in report metadata trips that scan
  // on ordinary code (e.g. `token = ...`). Keep commit info, drop the diff.
  captureGitInfo: { commit: true, diff: false },
  ...(bootstrap ? { testMatch: "**/bootstrap-service-identity.spec.ts" } :
    { testIgnore: "**/bootstrap-service-identity.spec.ts" }),
  // Real OIDC journeys mutate the same disposable identity/tenant fixture.
  // Run browsers sequentially; API concurrency remains covered independently.
  workers: 1,
  timeout: 45_000,
  reporter: [
    ["list"],
    ["json", { outputFile: bootstrap
      ? "dist/security-evidence/playwright-bootstrap.json"
      : "dist/security-evidence/playwright-p1.json" }]
  ],
  use: {
    baseURL: webBase,
    actionTimeout: 15_000,
    navigationTimeout: 30_000,
    trace: "off"
  },
  webServer: [
    // A non-default synthetic realm is operator-provisioned; do not launch or
    // modify its container implicitly.
    ...(oidcPort === 18080 ? [{
      command:
        "docker compose -f infra/dev/compose.yaml -f infra/dev/compose.identity.yaml up keycloak",
      url: issuer + "/.well-known/openid-configuration",
      reuseExistingServer: true,
      timeout: 120_000
    }] : []),
    {
      command:
        "uv run --project apps/api python -m tools.seed_identity_e2e" +
        (bootstrap ? " --uninitialized" : "") + " && " +
        "uv run --project apps/api uvicorn openvietgis_api.main:app " +
        `--app-dir apps/api --host 127.0.0.1 --port ${apiPort} --no-access-log`,
      url: apiBase + "/api/v1/health/ready",
      reuseExistingServer: false,
      timeout: 60_000,
      env: {
        OPENVIETGIS_WEB_BASE_URL: webBase,
        OPENVIETGIS_API_PUBLIC_BASE_URL: apiBase,
        OPENVIETGIS_SESSION_COOKIE_SECURE: "false",
        OPENVIETGIS_OIDC_PROVIDERS: JSON.stringify({
          keycloak: {
            issuer,
            client_id: "openvietgis-web",
            display_name: "OpenVietGIS Development"
          }
        })
      }
    },
    {
      // Reuse the exact Node runtime already running Playwright. Corepack is not
      // bundled with Node 26 and an old global shim can select a different Node.
      command: `"${execPath}" node_modules/vite/bin/vite.js build && "${execPath}" scripts/build-offline-shell.mjs && "${execPath}" node_modules/vite/bin/vite.js preview --host 127.0.0.1 --port ${webPort} --strictPort`,
      cwd: "./apps/web-next",
      // Ensure the SPA proxy can reach the ready API before any real login.
      url: webBase + "/api/v1/health/ready",
      reuseExistingServer: false,
      timeout: 60_000,
      env: { OPENVIETGIS_API_PROXY_PORT: String(apiPort) }
    }
  ]
  });
}

export default makeConfig();
