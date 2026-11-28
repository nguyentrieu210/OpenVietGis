import { defineConfig } from "@playwright/test";
import { execPath } from "node:process";

export default defineConfig({
  testDir: "./tests/e2e",
  testMatch: "**/house-numbering.spec.ts",
  workers: 1,
  timeout: 180_000,
  reporter: [["list"]],
  use: {
    baseURL: "http://127.0.0.1:5178",
    trace: "off",
    screenshot: "off",
    video: "off"
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
        "uv run --project apps/api python tools/configure_keycloak_e2e_redirect.py " +
        "--redirect-uri http://127.0.0.1:8019/api/v1/auth/callback/keycloak " +
        "--web-origin http://127.0.0.1:5178 && " +
        "uv run --project apps/api alembic -c apps/api/alembic.ini upgrade head && " +
        "PYTHONPATH=apps/api:. uv run --project apps/api python tools/migrate_official_modules.py && " +
        "uv run --project apps/api python -m tools.seed_identity_e2e && " +
        "PYTHONPATH=apps/api:. uv run --project apps/api uvicorn openvietgis_api.main:app " +
        "--app-dir apps/api --host 127.0.0.1 --port 8019 --no-access-log",
      url: "http://127.0.0.1:8019/api/v1/health/ready",
      reuseExistingServer: false,
      timeout: 90_000,
      env: {
        OPENVIETGIS_ENVIRONMENT: "test",
        OPENVIETGIS_DATABASE_URL: "postgresql+psycopg://openvietgis:openvietgis@127.0.0.1:54330/openvietgis_test",
        OPENVIETGIS_TEST_DATABASE_URL: "postgresql+psycopg://openvietgis:openvietgis@127.0.0.1:54330/openvietgis_test",
        OPENVIETGIS_MIGRATION_DATABASE_URL: "postgresql+psycopg://openvietgis:openvietgis@127.0.0.1:54330/openvietgis_test",
        OPENVIETGIS_RUNTIME_DATABASE_URL: "postgresql+psycopg://openvietgis_runtime:openvietgis-runtime-dev@127.0.0.1:54330/openvietgis_test",
        OPENVIETGIS_APPLICATION_SESSION_SECRET: "openvietgis-test-session-secret",
        OPENVIETGIS_CSRF_SECRET: "openvietgis-test-csrf-secret",
        OPENVIETGIS_BOOTSTRAP_SECRET: "openvietgis-test-bootstrap-secret",
        OPENVIETGIS_WEB_BASE_URL: "http://127.0.0.1:5178",
        OPENVIETGIS_API_PUBLIC_BASE_URL: "http://127.0.0.1:8019",
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
      command: `"${execPath}" node_modules/vite/bin/vite.js build && "${execPath}" node_modules/vite/bin/vite.js preview --host 127.0.0.1 --port 5178 --strictPort`,
      cwd: "./apps/web-next",
      env: { OPENVIETGIS_API_PROXY_PORT: "8019" },
      url: "http://127.0.0.1:5178/api/v1/health/ready",
      reuseExistingServer: false,
      timeout: 90_000
    }
  ]
});
