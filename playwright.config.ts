import { defineConfig } from "@playwright/test";
import { execPath } from "node:process";

export function makeConfig(bootstrap = false) {
  return defineConfig({
  testDir: "./tests/e2e",
  ...(bootstrap ? { testMatch: "**/bootstrap-service-identity.spec.ts" } :
    { testIgnore: "**/bootstrap-service-identity.spec.ts" }),
  timeout: 45_000,
  reporter: [
    ["list"],
    ["json", { outputFile: bootstrap
      ? "dist/security-evidence/playwright-bootstrap.json"
      : "dist/security-evidence/playwright-p1.json" }]
  ],
  use: {
    baseURL: "http://127.0.0.1:5178",
    trace: "off"
  },
  webServer: [
    {
      command:
        "docker compose -f infra/dev/compose.yaml -f infra/dev/compose.identity.yaml up keycloak",
      url: "http://127.0.0.1:18080/realms/openvietgis/.well-known/openid-configuration",
      reuseExistingServer: true,
      timeout: 120_000
    },
    {
      command:
        "uv run --project apps/api python -m tools.seed_identity_e2e" +
        (bootstrap ? " --uninitialized" : "") + " && " +
        "uv run --project apps/api uvicorn openvietgis_api.main:app " +
        "--app-dir apps/api --host 127.0.0.1 --port 8000 --no-access-log",
      url: "http://127.0.0.1:8000/api/v1/health/live",
      reuseExistingServer: false,
      timeout: 60_000,
      env: {
        OPENVIETGIS_WEB_BASE_URL: "http://127.0.0.1:5178",
        OPENVIETGIS_API_PUBLIC_BASE_URL: "http://127.0.0.1:8000",
        OPENVIETGIS_SESSION_COOKIE_SECURE: "false",
        OPENVIETGIS_OIDC_PROVIDERS: JSON.stringify({
          keycloak: {
            issuer: "http://127.0.0.1:18080/realms/openvietgis",
            client_id: "openvietgis-web",
            display_name: "OpenVietGIS Development"
          }
        })
      }
    },
    {
      // Reuse the exact Node runtime already running Playwright. Corepack is not
      // bundled with Node 26 and an old global shim can select a different Node.
      command: `"${execPath}" node_modules/vite/bin/vite.js --host 127.0.0.1 --port 5178`,
      cwd: "./apps/web",
      url: "http://127.0.0.1:5178",
      reuseExistingServer: false,
      timeout: 60_000
    }
  ]
  });
}

export default makeConfig();
