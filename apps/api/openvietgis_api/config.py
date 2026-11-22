from __future__ import annotations

from functools import lru_cache
from typing import Literal
from urllib.parse import urlsplit
from uuid import UUID

from pydantic import BaseModel, Field, SecretStr, model_validator
from pydantic_settings import BaseSettings, SettingsConfigDict
from sqlalchemy.engine import make_url
from sqlalchemy.exc import ArgumentError


class OidcProviderSettings(BaseModel):
    issuer: str = Field(min_length=1)
    client_id: str = Field(min_length=1)
    client_secret: SecretStr | None = None
    display_name: str = Field(min_length=1)


class Settings(BaseSettings):
    model_config = SettingsConfigDict(
        env_prefix="OPENVIETGIS_",
        env_file=".env",
        extra="ignore",
        hide_input_in_errors=True,
    )

    environment: Literal["development", "test", "production"]
    database_url: str = Field(min_length=1)
    migration_database_url: str | None = None
    runtime_database_url: str | None = None

    web_base_url: str = "http://127.0.0.1:5173"
    api_public_base_url: str = "http://127.0.0.1:8000"
    session_cookie_name: str = "ovg_session"
    session_cookie_secure: bool = False
    session_ttl_seconds: int = Field(default=28_800, ge=300, le=604_800)
    artifact_store_dir: str = "/tmp/openvietgis-artifacts"
    marketplace_catalog_path: str | None = None
    marketplace_operator_public_key: str | None = None
    marketplace_reviewer_user_ids: list[UUID] = Field(default_factory=list)

    application_session_secret: SecretStr
    csrf_secret: SecretStr
    bootstrap_secret: SecretStr
    oidc_providers: dict[str, OidcProviderSettings] = Field(default_factory=dict)
    resend_api_key: SecretStr | None = None
    invitation_from_email: str | None = None
    invitation_web_path: str = "/rewrite/?live=1"

    @model_validator(mode="after")
    def validate_proposed_production_profile(self) -> Settings:
        if self.environment != "production":
            return self
        if not self.session_cookie_secure:
            raise ValueError("Production session cookies must be secure")
        for public_url in (self.web_base_url, self.api_public_base_url):
            parsed = urlsplit(public_url)
            if (
                parsed.scheme != "https"
                or not parsed.hostname
                or parsed.username is not None
                or parsed.password is not None
                or parsed.path not in ("", "/")
                or parsed.query
                or parsed.fragment
            ):
                raise ValueError("Production public URLs must be HTTPS origins")
        if not self.oidc_providers:
            raise ValueError("Production requires an explicit HTTPS identity provider")
        for provider in self.oidc_providers.values():
            issuer = urlsplit(provider.issuer)
            if (
                issuer.scheme != "https"
                or not issuer.hostname
                or issuer.username is not None
                or issuer.password is not None
                or issuer.query
                or issuer.fragment
            ):
                raise ValueError("Production identity provider issuer must use HTTPS")
        secrets = [
            secret.get_secret_value()
            for secret in (self.application_session_secret, self.csrf_secret, self.bootstrap_secret)
        ]
        if any(len(secret) < 32 for secret in secrets) or len(set(secrets)) != len(secrets):
            raise ValueError(
                "Production application secrets must be distinct and at least 32 chars"
            )
        if not self.runtime_database_url or self.database_url != self.runtime_database_url:
            raise ValueError("Production DATABASE_URL must be the explicit runtime URL")
        try:
            runtime = make_url(self.runtime_database_url)
            if (
                runtime.drivername != "postgresql+psycopg"
                or runtime.username != "openvietgis_runtime"
                or not runtime.password
                or not runtime.host
                or not runtime.database
                or runtime.query
            ):
                raise ValueError("Production runtime database URL must name the restricted role")
            if self.migration_database_url:
                migration = make_url(self.migration_database_url)
                if (
                    migration.drivername != runtime.drivername
                    or not migration.username
                    or migration.username == runtime.username
                    or not migration.password
                    or migration.query
                    or (migration.host, migration.port, migration.database)
                    != (runtime.host, runtime.port, runtime.database)
                ):
                    raise ValueError("Production migration URL must target same DB, distinct owner")
        except ArgumentError:
            raise ValueError("Invalid production database URL") from None
        return self

    @property
    def migration_db_url(self) -> str:
        if self.environment == "production" and not self.migration_database_url:
            raise RuntimeError("Production migrations require an explicit owner database URL")
        return self.migration_database_url or self.database_url

    @property
    def runtime_db_url(self) -> str:
        if not self.runtime_database_url:
            raise RuntimeError("An explicit non-privileged runtime database URL is required")
        migration_url = self.migration_database_url
        if self.environment != "production":
            migration_url = self.migration_db_url
        if self.runtime_database_url == migration_url:
            raise RuntimeError("Runtime and migration database credentials must be distinct")
        return self.runtime_database_url


@lru_cache
def get_settings() -> Settings:
    return Settings()  # type: ignore[call-arg]
