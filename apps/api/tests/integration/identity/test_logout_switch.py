from openvietgis_api.config import OidcProviderSettings, get_settings
from tests.fixtures.identity_tenancy import seed_access_world
from tests.integration.rls.test_cross_tenant_api import client_for_user


def test_logout_clears_only_same_origin_realm_and_revokes_app_session(migrated_database):
    world = seed_access_world()
    with client_for_user(world.owner_user_id) as client:
        original = get_settings()
        settings = original.model_copy(
            update={
                "web_base_url": "https://gis.example.test",
                "api_public_base_url": "https://gis.example.test",
                "session_cookie_secure": True,
                "oidc_providers": {
                    "keycloak": OidcProviderSettings(
                        issuer="https://gis.example.test/realms/openvietgis",
                        client_id="openvietgis-web",
                        display_name="Keycloak",
                    )
                },
            }
        )
        client.app.dependency_overrides[get_settings] = lambda: settings
        try:
            assert client.get("/api/v1/me").headers["Cache-Control"] == "no-store, private"
            response = client.post("/api/v1/auth/logout")
            assert response.status_code == 204
            cookies = response.headers.get_list("set-cookie")
            for name in (
                "KEYCLOAK_SESSION",
                "KEYCLOAK_IDENTITY",
                "AUTH_SESSION_ID",
                "KC_AUTH_SESSION_HASH",
            ):
                assert any(
                    c.startswith(name + "=")
                    and "Path=/realms/openvietgis/" in c
                    and "Max-Age=0" in c
                    for c in cookies
                )
            assert any(
                c.startswith(settings.session_cookie_name + "=") and "Max-Age=0" in c
                for c in cookies
            )
            assert client.get("/api/v1/me").status_code == 401
        finally:
            client.app.dependency_overrides.pop(get_settings, None)
