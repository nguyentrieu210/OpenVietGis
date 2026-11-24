"""R014 RLS-enforced repository for generic extension manifests, activations & records."""

from __future__ import annotations

import json
from datetime import UTC, datetime
from typing import Any
from uuid import UUID, uuid4

from sqlalchemy import text
from sqlalchemy.engine import RowMapping
from sqlalchemy.orm import Session

from openvietgis_api.adapters.persistence.rls import (
    bind_authenticated_actor,
    bind_authenticated_service_actor,
    require_member_context,
)
from openvietgis_api.adapters.persistence.scope_helper import (
    append_project_audit,
    prepare_project_scope,
)
from openvietgis_api.adapters.persistence.tenancy_repository import SqlTenancyRepository
from openvietgis_api.domains.authorization.context import AuthorizationContext
from openvietgis_api.domains.authorization.permissions import Permission
from openvietgis_api.domains.authorization.roles import ORGANIZATION_ROLE_PERMISSIONS
from openvietgis_api.domains.extensions.domain import (
    MAX_BUNDLE_BYTES,
    MAX_RECORDS,
    MAX_REVISIONS,
    PORTABLE_FORMAT,
    ExtensionDataRecord,
    ExtensionManifestRecord,
    ProjectActivationRecord,
    bundle_digest,
    canonical_bytes,
    manifest_contract,
    validate_manifest_contract,
    validate_record_against_schema,
    validate_stored_manifest_for_mutation,
    verify_transition,
)
from openvietgis_api.domains.platform_errors import (
    PlatformConflictError,
    PlatformForbiddenError,
    PlatformNotFoundError,
    PlatformValidationError,
)
from openvietgis_api.domains.tenancy.domain import MembershipStatus, OrganizationStatus


def _manifest_from_row(row: RowMapping) -> ExtensionManifestRecord:
    return ExtensionManifestRecord(
        id=row["id"],
        organization_id=row["organization_id"],
        extension_key=str(row["extension_key"]),
        version=str(row["version"]),
        api_compatibility=str(row["api_compatibility"]),
        display_name=str(row["display_name"]),
        description=str(row["description"]),
        permissions=[str(x) for x in row["permissions"]],
        commands=list(row["commands"]),
        capabilities=[str(x) for x in row["capabilities"]],
        entity_schemas=dict(row["entity_schemas"]),
        enabled=bool(row["enabled"]),
        registered_by_actor_id=row["registered_by_actor_id"],
        created_at=row["created_at"],
        ui_contributions=list(row.get("ui_contributions") or []),
    )


def _act_from_row(row: RowMapping) -> ProjectActivationRecord:
    return ProjectActivationRecord(
        id=row["id"],
        organization_id=row["organization_id"],
        project_id=row["project_id"],
        manifest_id=row["manifest_id"],
        extension_key=str(row["extension_key"]),
        status=str(row["status"]),
        activated_by_actor_id=row["activated_by_actor_id"],
        activated_at=row["activated_at"],
        updated_at=row["updated_at"],
        revision=int(row.get("revision") or 1),
        disabled_by_actor_id=row.get("disabled_by_actor_id"),
        disabled_at=row.get("disabled_at"),
    )


def _rec_from_row(row: RowMapping) -> ExtensionDataRecord:
    return ExtensionDataRecord(
        id=row["id"],
        organization_id=row["organization_id"],
        project_id=row["project_id"],
        extension_key=str(row["extension_key"]),
        entity_name=str(row["entity_name"]),
        payload=dict(row["payload"]),
        linked_layer_id=row["linked_layer_id"],
        linked_feature_id=str(row["linked_feature_id"]) if row["linked_feature_id"] else None,
        created_by_actor_id=row["created_by_actor_id"],
        created_at=row["created_at"],
        updated_at=row["updated_at"],
        manifest_id=row["manifest_id"],
        revision=int(row["revision"]),
        link_context=dict(row["link_context"]) if row["link_context"] is not None else None,
        operation_id=row.get("operation_id"),
        updated_by_actor_id=row.get("updated_by_actor_id"),
    )


class SqlExtensionsRepository:
    def __init__(self, session: Session) -> None:
        self._session = session

    def _bind_org(self, context: AuthorizationContext) -> None:
        assert context.organization_id is not None
        if context.actor.actor_type == "human":
            bind_authenticated_actor(self._session, context.actor)
        elif context.actor.actor_type == "service_principal" and context.credential_id is not None:
            bind_authenticated_service_actor(
                self._session,
                context.actor,
                organization_id=context.organization_id,
                credential_id=context.credential_id,
            )
        require_member_context(self._session, context, context.organization_id)

    def register_manifest(
        self,
        *,
        context: AuthorizationContext,
        extension_key: str,
        version: str,
        api_compatibility: str,
        display_name: str,
        description: str,
        permissions: list[str],
        commands: list[dict[str, Any]],
        capabilities: list[str],
        entity_schemas: dict[str, dict[str, Any]],
        ui_contributions: list[dict[str, Any]] | None = None,
    ) -> ExtensionManifestRecord:
        self._bind_org(context)
        assert context.organization_id is not None and context.actor.actor_id is not None
        # Tenant registration is metadata-only, not installation-wide approval.
        # Only a current human tenant administrator may add a manifest.
        tenant = SqlTenancyRepository(self._session)
        org = tenant.get_organization(context.organization_id, context=context, for_update=True)
        if org is None or org.status != OrganizationStatus.ACTIVE:
            raise PlatformNotFoundError("organization not found")
        if context.actor.actor_type != "human":
            raise PlatformForbiddenError("manifest registration requires human administrator")
        member = tenant.get_membership_for_user(
            context=context,
            organization_id=context.organization_id,
            user_id=context.actor.actor_id,
            for_update=True,
        )
        if (
            member is None
            or member.status != MembershipStatus.ACTIVE
            or Permission.ORGANIZATION_MANAGE_SETTINGS
            not in ORGANIZATION_ROLE_PERMISSIONS[member.role]
        ):
            raise PlatformForbiddenError(
                "manifest registration requires organization settings authority"
            )
        validate_manifest_contract(
            extension_key=extension_key,
            version=version,
            api_compatibility=api_compatibility,
            permissions=permissions,
            entity_schemas=entity_schemas,
            commands=commands,
            capabilities=capabilities,
            ui_contributions=ui_contributions or [],
        )
        if not display_name.strip() or len(display_name) > 200 or len(description) > 500:
            raise PlatformValidationError("invalid manifest display name or description")
        exists = self._session.execute(
            text("""
                SELECT id FROM extensions.manifests
                WHERE organization_id = :org AND extension_key = :key AND version = :ver
            """),
            {"org": context.organization_id, "key": extension_key, "ver": version},
        ).first()
        if exists is not None:
            raise PlatformConflictError(
                f"manifest {extension_key}@{version} already registered",
                code="MANIFEST_EXISTS",
            )
        now = datetime.now(UTC)
        row = (
            self._session.execute(
                text("""
                INSERT INTO extensions.manifests (
                    id, organization_id, extension_key, version, api_compatibility,
                    display_name, description, permissions, commands, capabilities,
                    entity_schemas, ui_contributions, enabled, registered_by_actor_id, created_at
                ) VALUES (
                    :id, :org, :key, :ver, :compat,
                    :dname, :desc, CAST(:perms AS jsonb), CAST(:cmds AS jsonb),
                    CAST(:caps AS jsonb), CAST(:schemas AS jsonb), CAST(:ui AS jsonb),
                    true, :actor, :now
                )
                RETURNING *
            """),
                {
                    "id": uuid4(),
                    "org": context.organization_id,
                    "key": extension_key,
                    "ver": version,
                    "compat": api_compatibility,
                    "dname": display_name.strip()[:200],
                    "desc": description.strip()[:500],
                    "perms": json.dumps(permissions),
                    "cmds": json.dumps(commands),
                    "caps": json.dumps(capabilities),
                    "schemas": json.dumps(entity_schemas),
                    "ui": json.dumps(ui_contributions or []),
                    "actor": context.actor.actor_id,
                    "now": now,
                },
            )
            .mappings()
            .one()
        )
        self._session.commit()
        return _manifest_from_row(row)

    def list_manifests(self, *, context: AuthorizationContext) -> list[ExtensionManifestRecord]:
        self._bind_org(context)
        assert context.organization_id is not None
        rows = (
            self._session.execute(
                text("""
                SELECT * FROM extensions.manifests
                WHERE organization_id = :org ORDER BY extension_key ASC, created_at DESC
            """),
                {"org": context.organization_id},
            )
            .mappings()
            .all()
        )
        return [_manifest_from_row(r) for r in rows]

    def activate_for_project(
        self, *, context: AuthorizationContext, manifest_id: UUID
    ) -> ProjectActivationRecord:
        prepare_project_scope(self._session, context, permission=Permission.PROJECT_MANAGE)
        assert context.organization_id is not None and context.project_id is not None
        assert context.actor.actor_id is not None

        mrow = (
            self._session.execute(
                text("""
                SELECT id, extension_key FROM extensions.manifests
                WHERE organization_id = :org AND id = :mid AND enabled = true
                  AND api_compatibility = '1.x'
            """),
                {"org": context.organization_id, "mid": manifest_id},
            )
            .mappings()
            .first()
        )
        if mrow is None:
            raise PlatformNotFoundError("extension manifest not found")
        validate_stored_manifest_for_mutation(self._manifest(context, manifest_id))

        existing = self._session.execute(
            text("""
                SELECT manifest_id FROM extensions.project_activations
                WHERE organization_id = :org AND project_id = :proj AND extension_key = :key
                FOR UPDATE
            """),
            {
                "org": context.organization_id,
                "proj": context.project_id,
                "key": mrow["extension_key"],
            },
        ).scalar_one_or_none()
        if existing is not None and existing != manifest_id:
            raise PlatformConflictError(
                "version transitions require a verified migration; implicit replacement is blocked",
                code="EXTENSION_VERSION_TRANSITION_REQUIRED",
            )
        now = datetime.now(UTC)
        row = (
            self._session.execute(
                text("""
                INSERT INTO extensions.project_activations (
                    id, organization_id, project_id, manifest_id, extension_key,
                    status, revision, activated_by_actor_id, activated_at,
                    disabled_by_actor_id, disabled_at, updated_at
                ) VALUES (
                    :id, :org, :proj, :mid, :key,
                    'active', 1, :actor, :now, NULL, NULL, :now
                )
                ON CONFLICT (organization_id, project_id, extension_key) DO UPDATE
                SET status = 'active',
                    revision = extensions.project_activations.revision + 1,
                    activated_by_actor_id = EXCLUDED.activated_by_actor_id,
                    activated_at = EXCLUDED.activated_at,
                    disabled_by_actor_id = NULL,
                    disabled_at = NULL,
                    updated_at = EXCLUDED.updated_at
                RETURNING *
            """),
                {
                    "id": uuid4(),
                    "org": context.organization_id,
                    "proj": context.project_id,
                    "mid": manifest_id,
                    "key": str(mrow["extension_key"]),
                    "actor": context.actor.actor_id,
                    "now": now,
                },
            )
            .mappings()
            .one()
        )
        append_project_audit(
            self._session,
            context,
            audit_id=uuid4(),
            action="extension.activate",
            resource_type="extension_activation",
            resource_id=row["id"],
            metadata={"manifest_id": str(manifest_id), "extension_key": row["extension_key"]},
            created_at=now,
        )
        self._append_activation_event(context, row, "activate", existing)
        self._session.commit()
        return _act_from_row(row)

    def deactivate_for_project(
        self, *, context: AuthorizationContext, activation_id: UUID
    ) -> ProjectActivationRecord:
        prepare_project_scope(self._session, context, permission=Permission.PROJECT_MANAGE)
        assert context.organization_id is not None and context.project_id is not None
        now = datetime.now(UTC)
        row = (
            self._session.execute(
                text("""
                UPDATE extensions.project_activations
                SET status = 'disabled', updated_at = :now
                WHERE id = :id AND organization_id = :org AND project_id = :proj
                RETURNING *
            """),
                {
                    "id": activation_id,
                    "org": context.organization_id,
                    "proj": context.project_id,
                    "now": now,
                },
            )
            .mappings()
            .one_or_none()
        )
        if row is None:
            raise PlatformNotFoundError("extension activation not found")
        append_project_audit(
            self._session,
            context,
            audit_id=uuid4(),
            action="extension.deactivate",
            resource_type="extension_activation",
            resource_id=activation_id,
            metadata={
                "manifest_id": str(row["manifest_id"]),
                "extension_key": row["extension_key"],
            },
            created_at=now,
        )
        self._append_activation_event(context, row, "deactivate", row["manifest_id"])
        self._session.commit()
        return _act_from_row(row)

    def disable_for_project(
        self, *, context: AuthorizationContext, extension_key: str
    ) -> ProjectActivationRecord:
        prepare_project_scope(self._session, context, permission=Permission.PROJECT_MANAGE)
        assert context.organization_id is not None and context.project_id is not None
        assert context.actor.actor_id is not None
        now = datetime.now(UTC)
        row = (
            self._session.execute(
                text("""
                UPDATE extensions.project_activations
                SET status = 'disabled',
                    revision = revision + 1,
                    disabled_by_actor_id = :actor,
                    disabled_at = :now,
                    updated_at = :now
                WHERE organization_id = :org AND project_id = :proj
                  AND extension_key = :key AND status = 'active'
                RETURNING *
            """),
                {
                    "org": context.organization_id,
                    "proj": context.project_id,
                    "key": extension_key,
                    "actor": context.actor.actor_id,
                    "now": now,
                },
            )
            .mappings()
            .first()
        )
        changed = row is not None
        if row is None:
            row = (
                self._session.execute(
                    text("""
                    SELECT * FROM extensions.project_activations
                    WHERE organization_id = :org AND project_id = :proj
                      AND extension_key = :key
                """),
                    {
                        "org": context.organization_id,
                        "proj": context.project_id,
                        "key": extension_key,
                    },
                )
                .mappings()
                .first()
            )
        if row is None:
            raise PlatformNotFoundError("project extension activation not found")
        if changed:
            append_project_audit(
                self._session,
                context,
                audit_id=uuid4(),
                action="extension.disable",
                resource_type="extension_activation",
                resource_id=row["id"],
                metadata={
                    "manifest_id": str(row["manifest_id"]),
                    "extension_key": str(row["extension_key"]),
                    "revision": int(row["revision"]),
                },
                created_at=now,
            )
            self._append_activation_event(context, row, "deactivate", row["manifest_id"])
        self._session.commit()
        return _act_from_row(row)

    def list_activations(self, *, context: AuthorizationContext) -> list[ProjectActivationRecord]:
        prepare_project_scope(self._session, context, permission=Permission.PROJECT_READ)
        assert context.organization_id is not None and context.project_id is not None
        rows = (
            self._session.execute(
                text("""
                SELECT * FROM extensions.project_activations
                WHERE organization_id = :org AND project_id = :proj
                ORDER BY extension_key ASC
            """),
                {"org": context.organization_id, "proj": context.project_id},
            )
            .mappings()
            .all()
        )
        return [_act_from_row(r) for r in rows]

    @staticmethod
    def _params(context: AuthorizationContext) -> dict[str, Any]:
        assert context.organization_id is not None and context.project_id is not None
        return {"org": context.organization_id, "proj": context.project_id}

    def _manifest(
        self, context: AuthorizationContext, manifest_id: UUID
    ) -> ExtensionManifestRecord:
        row = (
            self._session.execute(
                text("SELECT * FROM extensions.manifests WHERE organization_id=:org AND id=:mid"),
                {"org": context.organization_id, "mid": manifest_id},
            )
            .mappings()
            .one_or_none()
        )
        if row is None:
            raise PlatformNotFoundError("extension manifest not found")
        return _manifest_from_row(row)

    def _activation_row(self, context: AuthorizationContext, extension_key: str) -> RowMapping:
        row = (
            self._session.execute(
                text("""
                SELECT * FROM extensions.project_activations
                WHERE organization_id=:org AND project_id=:proj AND extension_key=:key
            """),
                {**self._params(context), "key": extension_key},
            )
            .mappings()
            .one_or_none()
        )
        if row is None:
            raise PlatformNotFoundError("extension activation not found")
        return row

    def _active_manifest(
        self, context: AuthorizationContext, extension_key: str
    ) -> ExtensionManifestRecord:
        row = self._activation_row(context, extension_key)
        manifest = self._manifest(context, row["manifest_id"])
        if (
            row["status"] != "active"
            or manifest.extension_key != extension_key
            or not manifest.enabled
            or manifest.api_compatibility != "1.x"
        ):
            raise PlatformNotFoundError("extension is not active and compatible in this project")
        validate_stored_manifest_for_mutation(manifest)
        return manifest

    def _append_activation_event(
        self,
        context: AuthorizationContext,
        row: RowMapping,
        event: str,
        previous_manifest_id: UUID | None,
    ) -> None:
        self._session.execute(
            text("""
            INSERT INTO extensions.activation_events (
              id, organization_id, project_id, activation_id, event, previous_manifest_id,
              manifest_id, status, actor_id, occurred_at
            ) VALUES (
              :id, :org, :proj, :aid, :event, :previous, :manifest, :status, :actor, :now
            )
        """),
            {
                **self._params(context),
                "id": uuid4(),
                "aid": row["id"],
                "event": event,
                "previous": previous_manifest_id,
                "manifest": row["manifest_id"],
                "status": row["status"],
                "actor": context.actor.actor_id,
                "now": row["updated_at"],
            },
        )

    def transition_version(
        self,
        *,
        context: AuthorizationContext,
        activation_id: UUID,
        expected_manifest_id: UUID,
        target_manifest_id: UUID,
    ) -> ProjectActivationRecord:
        prepare_project_scope(self._session, context, permission=Permission.PROJECT_MANAGE)
        params = self._params(context)
        current = (
            self._session.execute(
                text("""
            SELECT * FROM extensions.project_activations
            WHERE organization_id=:org AND project_id=:proj AND id=:aid FOR UPDATE
        """),
                {**params, "aid": activation_id},
            )
            .mappings()
            .one_or_none()
        )
        if current is None:
            raise PlatformNotFoundError("extension activation not found")
        if current["manifest_id"] != expected_manifest_id:
            raise PlatformConflictError(
                "activation version changed", code="STALE_EXTENSION_VERSION"
            )
        old = self._manifest(context, expected_manifest_id)
        target = self._manifest(context, target_manifest_id)
        validate_stored_manifest_for_mutation(old)
        validate_stored_manifest_for_mutation(target)
        verify_transition(old, target)
        now = datetime.now(UTC)
        row = (
            self._session.execute(
                text("""
            UPDATE extensions.project_activations
            SET manifest_id=:target, updated_at=:now
            WHERE organization_id=:org AND project_id=:proj AND id=:aid
              AND manifest_id=:expected
            RETURNING *
        """),
                {
                    **params,
                    "aid": activation_id,
                    "expected": expected_manifest_id,
                    "target": target_manifest_id,
                    "now": now,
                },
            )
            .mappings()
            .one()
        )
        self._append_activation_event(context, row, "transition", expected_manifest_id)
        append_project_audit(
            self._session,
            context,
            audit_id=uuid4(),
            action="extension.transition",
            resource_type="extension_activation",
            resource_id=activation_id,
            metadata={
                "previous_manifest_id": str(expected_manifest_id),
                "manifest_id": str(target_manifest_id),
                "mode": "metadata_only",
            },
            created_at=now,
        )
        self._session.commit()
        return _act_from_row(row)

    def _activation_history(
        self,
        context: AuthorizationContext,
        activation_id: UUID,
    ) -> list[dict[str, Any]]:
        rows = (
            self._session.execute(
                text("""
            SELECT * FROM extensions.activation_events
            WHERE organization_id=:org AND project_id=:proj AND activation_id=:aid
            ORDER BY occurred_at, id LIMIT :limit
        """),
                {**self._params(context), "aid": activation_id, "limit": MAX_REVISIONS + 1},
            )
            .mappings()
            .all()
        )
        if len(rows) > MAX_REVISIONS:
            raise PlatformValidationError(
                "activation history exceeds portable limit", code="EXTENSION_TRANSFER_LIMIT"
            )
        return [
            {
                "id": str(r["id"]),
                "event": r["event"],
                "previous_manifest_id": str(r["previous_manifest_id"])
                if r["previous_manifest_id"]
                else None,
                "manifest_id": str(r["manifest_id"]),
                "status": r["status"],
                "actor_id": str(r["actor_id"]),
                "occurred_at": r["occurred_at"].isoformat(),
            }
            for r in rows
        ]

    def activation_history(
        self,
        *,
        context: AuthorizationContext,
        activation_id: UUID,
    ) -> list[dict[str, Any]]:
        prepare_project_scope(self._session, context, permission=Permission.PROJECT_READ)
        visible = self._session.execute(
            text("""
            SELECT id FROM extensions.project_activations
            WHERE organization_id=:org AND project_id=:proj AND id=:aid
        """),
            {**self._params(context), "aid": activation_id},
        ).first()
        if visible is None:
            raise PlatformNotFoundError("extension activation not found")
        return self._activation_history(context, activation_id)

    def _current_link(
        self,
        context: AuthorizationContext,
        linked_layer_id: UUID | None,
        linked_feature_id: str | None,
    ) -> dict[str, Any] | None:
        if linked_feature_id is not None and linked_layer_id is None:
            raise PlatformValidationError("linked feature requires a layer")
        if linked_layer_id is None:
            return None
        if linked_feature_id is not None and (
            not linked_feature_id or len(linked_feature_id) > 128
        ):
            raise PlatformValidationError("invalid linked feature identifier")
        params = self._params(context)
        link = (
            self._session.execute(
                text("""
            SELECT l.source_id, s.content_revision, s.schema_revision, s.stored_crs
            FROM spatial.layers l
            JOIN spatial.data_sources s ON s.id=l.source_id
              AND s.organization_id=l.organization_id AND s.project_id=l.project_id
            WHERE l.organization_id=:org AND l.project_id=:proj AND l.id=:layer
              AND l.lifecycle='active' AND s.lifecycle='active'
              AND s.validation_state='ready' AND s.stored_crs IS NOT NULL
        """),
                {**params, "layer": linked_layer_id},
            )
            .mappings()
            .one_or_none()
        )
        if link is None:
            raise PlatformNotFoundError("linked layer not found or unavailable")
        if linked_feature_id is not None:
            found = self._session.execute(
                text("""
                SELECT feature_id FROM spatial.reference_features
                WHERE organization_id=:org AND project_id=:proj
                  AND source_id=:src AND feature_id=:fid
                  AND content_revision=:content AND schema_revision=:schema
            """),
                {
                    **params,
                    "src": link["source_id"],
                    "fid": linked_feature_id,
                    "content": link["content_revision"],
                    "schema": link["schema_revision"],
                },
            ).scalar_one_or_none()
            if found is None:
                raise PlatformNotFoundError("linked feature not found")
        return {
            "status": "pinned",
            "source_id": str(link["source_id"]),
            "content_revision": link["content_revision"],
            "schema_revision": link["schema_revision"],
            "stored_crs": link["stored_crs"],
        }

    def _assert_link(
        self,
        context: AuthorizationContext,
        linked_layer_id: UUID | None,
        linked_feature_id: str | None,
        expected_link_context: dict[str, Any] | None,
    ) -> dict[str, Any] | None:
        link = self._current_link(context, linked_layer_id, linked_feature_id)
        if link != expected_link_context:
            raise PlatformConflictError(
                "linked Core revision differs or was not explicitly supplied",
                code="STALE_EXTENSION_LINK",
            )
        return link

    def _append_revision(
        self,
        context: AuthorizationContext,
        row: RowMapping,
        event: str,
        import_origin: dict[str, Any] | None = None,
    ) -> None:
        self._session.execute(
            text("""
            INSERT INTO extensions.record_revisions (
                organization_id, project_id, record_id, revision, manifest_id, entity_name,
                payload, linked_layer_id, linked_feature_id, link_context, actor_id, event,
                occurred_at, import_origin
            ) VALUES (
                :org, :proj, :id, :revision, :manifest, :entity, CAST(:payload AS jsonb),
                :lid, :fid, CAST(:link AS jsonb), :actor, :event, :now, CAST(:origin AS jsonb)
            )
        """),
            {
                **self._params(context),
                "id": row["id"],
                "revision": row["revision"],
                "manifest": row["manifest_id"],
                "entity": row["entity_name"],
                "payload": json.dumps(row["payload"]),
                "lid": row["linked_layer_id"],
                "fid": row["linked_feature_id"],
                "link": json.dumps(row["link_context"])
                if row["link_context"] is not None
                else None,
                "actor": context.actor.actor_id,
                "event": event,
                "now": row["updated_at"],
                "origin": json.dumps(import_origin) if import_origin is not None else None,
            },
        )

    def _insert_record(
        self,
        context: AuthorizationContext,
        manifest: ExtensionManifestRecord,
        entity_name: str,
        payload: dict[str, Any],
        linked_layer_id: UUID | None,
        linked_feature_id: str | None,
        link_context: dict[str, Any] | None,
        import_origin: dict[str, Any] | None = None,
        operation_id: UUID | None = None,
    ) -> ExtensionDataRecord:
        cleaned = validate_record_against_schema(
            entity_name=entity_name,
            payload=payload,
            entity_schemas=manifest.entity_schemas,
        )
        now = datetime.now(UTC)
        row = (
            self._session.execute(
                text("""
            INSERT INTO extensions.records (
                id, organization_id, project_id, extension_key, entity_name,
                payload, linked_layer_id, linked_feature_id, created_by_actor_id,
                updated_by_actor_id, created_at, updated_at, manifest_id, revision,
                link_context, operation_id
            ) VALUES (
                :id, :org, :proj, :key, :entity, CAST(:payload AS jsonb),
                :lid, :fid, :actor, :actor, :now, :now, :manifest, 1,
                CAST(:link AS jsonb), :op
            ) RETURNING *
        """),
                {
                    **self._params(context),
                    "id": uuid4(),
                    "key": manifest.extension_key,
                    "entity": entity_name,
                    "payload": json.dumps(cleaned),
                    "lid": linked_layer_id,
                    "fid": linked_feature_id,
                    "actor": context.actor.actor_id,
                    "now": now,
                    "manifest": manifest.id,
                    "link": json.dumps(link_context) if link_context is not None else None,
                    "op": operation_id,
                },
            )
            .mappings()
            .one()
        )
        event = "import" if import_origin else "create"
        self._append_revision(context, row, event, import_origin)
        append_project_audit(
            self._session,
            context,
            audit_id=uuid4(),
            action=f"extension.record.{event}",
            resource_type="extension_record",
            resource_id=row["id"],
            metadata={"manifest_id": str(manifest.id), "revision": 1},
            created_at=now,
        )
        return _rec_from_row(row)

    def create_record(
        self,
        *,
        context: AuthorizationContext,
        extension_key: str,
        entity_name: str,
        payload: dict[str, Any],
        linked_layer_id: UUID | None = None,
        linked_feature_id: str | None = None,
        expected_link_context: dict[str, Any] | None = None,
        operation_id: UUID | None = None,
    ) -> ExtensionDataRecord:
        prepare_project_scope(self._session, context, permission=Permission.PROJECT_EDIT)
        manifest = self._active_manifest(context, extension_key)
        if operation_id is not None:
            existing = (
                self._session.execute(
                    text("""
                    SELECT * FROM extensions.records
                    WHERE organization_id = :org AND project_id = :proj
                      AND extension_key = :key AND operation_id = :op
                """),
                    {
                        "org": context.organization_id,
                        "proj": context.project_id,
                        "key": extension_key,
                        "op": operation_id,
                    },
                )
                .mappings()
                .first()
            )
            if existing is not None:
                return _rec_from_row(existing)
        link = self._assert_link(context, linked_layer_id, linked_feature_id, expected_link_context)
        record = self._insert_record(
            context,
            manifest,
            entity_name,
            payload,
            linked_layer_id,
            linked_feature_id,
            link,
            operation_id=operation_id,
        )
        self._session.commit()
        return record

    def _record_row(
        self,
        context: AuthorizationContext,
        extension_key: str,
        record_id: UUID,
    ) -> RowMapping:
        row = (
            self._session.execute(
                text("""
            SELECT * FROM extensions.records
            WHERE organization_id=:org AND project_id=:proj AND extension_key=:key AND id=:id
        """),
                {**self._params(context), "key": extension_key, "id": record_id},
            )
            .mappings()
            .one_or_none()
        )
        if row is None:
            raise PlatformNotFoundError("extension record not found")
        return row

    def update_record(
        self,
        *,
        context: AuthorizationContext,
        extension_key: str,
        record_id: UUID,
        expected_revision: int,
        expected_manifest_id: UUID | None,
        payload: dict[str, Any],
        expected_link_context: dict[str, Any] | None = None,
        operation_id: UUID | None = None,
    ) -> ExtensionDataRecord:
        prepare_project_scope(self._session, context, permission=Permission.PROJECT_EDIT)
        manifest = self._active_manifest(context, extension_key)
        if operation_id is not None:
            prior = (
                self._session.execute(
                    text("""
                    SELECT * FROM extensions.records
                    WHERE organization_id = :org AND project_id = :proj
                      AND extension_key = :key AND operation_id = :op
                """),
                    {
                        "org": context.organization_id,
                        "proj": context.project_id,
                        "key": extension_key,
                        "op": operation_id,
                    },
                )
                .mappings()
                .first()
            )
            if prior is not None:
                if prior["id"] != record_id:
                    raise PlatformConflictError(
                        "operation_id already used by another record",
                        code="OPERATION_REPLAY_CONFLICT",
                    )
                return _rec_from_row(prior)
        old = self._record_row(context, extension_key, record_id)
        effective_expected_manifest = expected_manifest_id or manifest.id
        if old["revision"] != expected_revision or manifest.id != effective_expected_manifest:
            raise PlatformConflictError(
                "record or active manifest version changed", code="STALE_EXTENSION_RECORD"
            )
        link = self._assert_link(
            context, old["linked_layer_id"], old["linked_feature_id"], expected_link_context
        )
        cleaned = validate_record_against_schema(
            entity_name=old["entity_name"],
            payload=payload,
            entity_schemas=manifest.entity_schemas,
        )
        if (
            cleaned == old["payload"]
            and old["manifest_id"] == manifest.id
            and old["link_context"] == link
        ):
            return _rec_from_row(old)
        now = datetime.now(UTC)
        row = (
            self._session.execute(
                text("""
            UPDATE extensions.records SET payload=CAST(:payload AS jsonb), revision=revision+1,
              manifest_id=:manifest, link_context=CAST(:link AS jsonb),
              operation_id=:op, updated_by_actor_id=:actor, updated_at=:now
            WHERE organization_id=:org AND project_id=:proj AND id=:id AND revision=:expected
            RETURNING *
        """),
                {
                    **self._params(context),
                    "id": record_id,
                    "expected": expected_revision,
                    "payload": json.dumps(cleaned),
                    "manifest": manifest.id,
                    "link": json.dumps(link) if link is not None else None,
                    "op": operation_id,
                    "actor": context.actor.actor_id,
                    "now": now,
                },
            )
            .mappings()
            .one()
        )
        self._append_revision(context, row, "update")
        append_project_audit(
            self._session,
            context,
            audit_id=uuid4(),
            action="extension.record.update",
            resource_type="extension_record",
            resource_id=record_id,
            metadata={
                "manifest_id": str(manifest.id),
                "revision": row["revision"],
                "previous_revision": expected_revision,
            },
            created_at=now,
        )
        self._session.commit()
        return _rec_from_row(row)

    def _history(
        self,
        context: AuthorizationContext,
        record_id: UUID,
    ) -> list[dict[str, Any]]:
        size = self._session.execute(
            text("""
            SELECT count(*), coalesce(sum(octet_length(payload::text)
                + coalesce(octet_length(link_context::text), 0)
                + coalesce(octet_length(import_origin::text), 0)), 0)
            FROM extensions.record_revisions
            WHERE organization_id=:org AND project_id=:proj AND record_id=:id
        """),
            {**self._params(context), "id": record_id},
        ).one()
        if size[0] > MAX_REVISIONS or size[1] > MAX_BUNDLE_BYTES:
            raise PlatformValidationError(
                "record history exceeds portable limit", code="EXTENSION_TRANSFER_LIMIT"
            )
        rows = (
            self._session.execute(
                text("""
            SELECT * FROM extensions.record_revisions
            WHERE organization_id=:org AND project_id=:proj AND record_id=:id
            ORDER BY revision ASC LIMIT :limit
        """),
                {**self._params(context), "id": record_id, "limit": MAX_REVISIONS + 1},
            )
            .mappings()
            .all()
        )
        if len(rows) > MAX_REVISIONS:
            raise PlatformValidationError(
                "record history exceeds portable limit", code="EXTENSION_TRANSFER_LIMIT"
            )
        return [
            {
                "revision": r["revision"],
                "manifest_id": str(r["manifest_id"]),
                "entity_name": r["entity_name"],
                "payload": r["payload"],
                "linked_layer_id": str(r["linked_layer_id"]) if r["linked_layer_id"] else None,
                "linked_feature_id": r["linked_feature_id"],
                "link_context": r["link_context"],
                "actor_id": str(r["actor_id"]),
                "occurred_at": r["occurred_at"].isoformat(),
                "event": r["event"],
                "import_origin": r["import_origin"],
            }
            for r in rows
        ]

    def record_history(
        self,
        *,
        context: AuthorizationContext,
        extension_key: str,
        record_id: UUID,
    ) -> list[dict[str, Any]]:
        prepare_project_scope(self._session, context, permission=Permission.PROJECT_READ)
        self._record_row(context, extension_key, record_id)
        return self._history(context, record_id)

    def export_bundle(
        self,
        *,
        context: AuthorizationContext,
        extension_key: str,
    ) -> dict[str, Any]:
        prepare_project_scope(self._session, context, permission=Permission.PROJECT_READ)
        activation = self._activation_row(context, extension_key)
        events = self._activation_history(context, activation["id"])
        rows = (
            self._session.execute(
                text("""
            SELECT * FROM extensions.records
            WHERE organization_id=:org AND project_id=:proj AND extension_key=:key
            ORDER BY id LIMIT :limit
        """),
                {**self._params(context), "key": extension_key, "limit": MAX_RECORDS + 1},
            )
            .mappings()
            .all()
        )
        if len(rows) > MAX_RECORDS:
            raise PlatformValidationError(
                "record count exceeds portable limit", code="EXTENSION_TRANSFER_LIMIT"
            )
        records: list[dict[str, Any]] = []
        manifest_ids = {str(activation["manifest_id"])}
        manifest_ids.update(e["manifest_id"] for e in events)
        manifest_ids.update(e["previous_manifest_id"] for e in events if e["previous_manifest_id"])
        count = 0
        bytes_used = len(canonical_bytes(events))
        for row in rows:
            history = self._history(context, row["id"])
            count += len(history)
            if count > MAX_REVISIONS:
                raise PlatformValidationError(
                    "revision count exceeds portable limit", code="EXTENSION_TRANSFER_LIMIT"
                )
            latest = {
                "revision": row["revision"],
                "manifest_id": str(row["manifest_id"]),
                "entity_name": row["entity_name"],
                "payload": row["payload"],
                "linked_layer_id": str(row["linked_layer_id"]) if row["linked_layer_id"] else None,
                "linked_feature_id": row["linked_feature_id"],
                "link_context": row["link_context"],
            }
            if not history or canonical_bytes(latest) != canonical_bytes(
                {k: history[-1][k] for k in latest}
            ):
                raise PlatformConflictError("record history is inconsistent")
            bytes_used += len(canonical_bytes(history)) + len(canonical_bytes(latest))
            if bytes_used > MAX_BUNDLE_BYTES:
                raise PlatformValidationError(
                    "portable bundle exceeds byte limit", code="EXTENSION_TRANSFER_LIMIT"
                )
            manifest_ids.update(h["manifest_id"] for h in history)
            records.append(
                {
                    "record_id": str(row["id"]),
                    "revision": row["revision"],
                    "manifest_id": str(row["manifest_id"]),
                    "entity_name": row["entity_name"],
                    "payload": row["payload"],
                    "linked_layer_id": str(row["linked_layer_id"])
                    if row["linked_layer_id"]
                    else None,
                    "linked_feature_id": row["linked_feature_id"],
                    "link_context": row["link_context"],
                    "history": history,
                }
            )
        manifests = []
        for mid in sorted(manifest_ids):
            record = self._manifest(context, UUID(mid))
            bytes_used += len(canonical_bytes(manifest_contract(record)))
            if bytes_used > MAX_BUNDLE_BYTES:
                raise PlatformValidationError(
                    "portable bundle exceeds byte limit", code="EXTENSION_TRANSFER_LIMIT"
                )
            manifests.append(
                {
                    "manifest_id": mid,
                    **manifest_contract(record),
                    "display_name": record.display_name,
                    "description": record.description,
                }
            )
        bundle = {
            "format": PORTABLE_FORMAT,
            "extension_key": extension_key,
            "origin": {
                "organization_id": str(context.organization_id),
                "project_id": str(context.project_id),
            },
            "active_manifest_id": str(activation["manifest_id"]),
            "activation_status": activation["status"],
            "manifests": manifests,
            "activation_history": events,
            "records": records,
        }
        return {"bundle": bundle, "sha256": bundle_digest(bundle)}

    def import_bundle(
        self,
        *,
        context: AuthorizationContext,
        extension_key: str,
        expected_manifest_id: UUID,
        bundle: dict[str, Any],
        sha256: str,
    ) -> dict[str, Any]:
        prepare_project_scope(self._session, context, permission=Permission.PROJECT_EDIT)
        manifest = self._active_manifest(context, extension_key)
        if manifest.id != expected_manifest_id:
            raise PlatformConflictError("active manifest changed", code="STALE_EXTENSION_VERSION")
        if bundle_digest(bundle) != sha256:
            raise PlatformValidationError("portable bundle digest mismatch")
        prior = (
            self._session.execute(
                text("""
            SELECT id, mappings FROM extensions.import_receipts
            WHERE organization_id=:org AND project_id=:proj AND extension_key=:key
              AND bundle_sha256=:digest
        """),
                {**self._params(context), "key": extension_key, "digest": sha256},
            )
            .mappings()
            .one_or_none()
        )
        if prior is not None:
            return {
                "id": str(prior["id"]),
                "sha256": sha256,
                "mappings": prior["mappings"],
                "replayed": True,
            }
        try:
            if (
                bundle.get("format") != PORTABLE_FORMAT
                or bundle.get("extension_key") != extension_key
                or set(bundle)
                != {
                    "format",
                    "extension_key",
                    "origin",
                    "active_manifest_id",
                    "activation_status",
                    "manifests",
                    "activation_history",
                    "records",
                }
            ):
                raise ValueError
            origin = bundle["origin"]
            if not isinstance(origin, dict) or set(origin) != {"organization_id", "project_id"}:
                raise ValueError
            source_org, source_project = UUID(origin["organization_id"]), UUID(origin["project_id"])
            manifests = bundle["manifests"]
            records = bundle["records"]
            if (
                not isinstance(manifests, list)
                or not isinstance(records, list)
                or not 1 <= len(records) <= MAX_RECORDS
            ):
                raise ValueError
            active = [m for m in manifests if m["manifest_id"] == bundle["active_manifest_id"]]
            if len(active) != 1 or any(
                active[0].get(k) != v for k, v in manifest_contract(manifest).items()
            ):
                raise PlatformConflictError(
                    "portable bundle requires exact destination manifest contract",
                    code="EXTENSION_IMPORT_INCOMPATIBLE",
                )
            items: list[
                tuple[
                    UUID, int, str, dict[str, Any], UUID | None, str | None, dict[str, Any] | None
                ]
            ] = []
            seen: set[UUID] = set()
            for item in records:
                if set(item) != {
                    "record_id",
                    "revision",
                    "manifest_id",
                    "entity_name",
                    "payload",
                    "linked_layer_id",
                    "linked_feature_id",
                    "link_context",
                    "history",
                }:
                    raise ValueError
                rid = UUID(item["record_id"])
                revision = item["revision"]
                if rid in seen or type(revision) is not int or revision < 1:
                    raise ValueError
                seen.add(rid)
                if not isinstance(item["payload"], dict) or not isinstance(
                    item["entity_name"], str
                ):
                    raise ValueError
                # Source history is untrusted origin metadata, never inserted as
                # destination audit/actor history or used to grant permission.
                history = item["history"]
                if (
                    not isinstance(history, list)
                    or not history
                    or history[-1]["revision"] != revision
                ):
                    raise ValueError
                if canonical_bytes(
                    {k: item[k] for k in item if k not in ("history", "record_id")}
                ) != canonical_bytes(
                    {k: history[-1][k] for k in item if k not in ("history", "record_id")}
                ):
                    raise ValueError
                lid = UUID(item["linked_layer_id"]) if item["linked_layer_id"] else None
                fid = item["linked_feature_id"]
                if fid is not None and not isinstance(fid, str):
                    raise ValueError
                if lid is not None and (
                    source_org != context.organization_id or source_project != context.project_id
                ):
                    raise PlatformConflictError(
                        "portable link remapping is not supported across scopes",
                        code="EXTENSION_IMPORT_LINK_REMAP_REQUIRED",
                    )
                link = self._assert_link(context, lid, fid, item["link_context"])
                payload = validate_record_against_schema(
                    entity_name=item["entity_name"],
                    payload=item["payload"],
                    entity_schemas=manifest.entity_schemas,
                )
                items.append((rid, revision, item["entity_name"], payload, lid, fid, link))
        except (KeyError, TypeError, ValueError, AttributeError) as exc:
            raise PlatformValidationError("invalid portable bundle") from exc
        mappings = []
        for rid, revision, entity, payload, lid, fid, link in items:
            made = self._insert_record(
                context,
                manifest,
                entity,
                payload,
                lid,
                fid,
                link,
                {
                    "claimed_source_organization_id": str(source_org),
                    "claimed_source_project_id": str(source_project),
                    "claimed_source_record_id": str(rid),
                    "claimed_source_revision": revision,
                    "bundle_sha256": sha256,
                    "verified_origin": False,
                },
            )
            mappings.append({"source_record_id": str(rid), "record_id": str(made.id)})
        receipt_id = uuid4()
        now = datetime.now(UTC)
        self._session.execute(
            text("""
            INSERT INTO extensions.import_receipts (
                id, organization_id, project_id, extension_key, manifest_id,
                bundle_sha256, mappings, actor_id, created_at
            ) VALUES (
                :id, :org, :proj, :key, :manifest, :digest, CAST(:mappings AS jsonb), :actor, :now
            )
        """),
            {
                **self._params(context),
                "id": receipt_id,
                "key": extension_key,
                "manifest": manifest.id,
                "digest": sha256,
                "mappings": canonical_bytes(mappings).decode(),
                "actor": context.actor.actor_id,
                "now": now,
            },
        )
        append_project_audit(
            self._session,
            context,
            audit_id=uuid4(),
            action="extension.import",
            resource_type="extension_import",
            resource_id=receipt_id,
            metadata={"bundle_sha256": sha256, "record_count": len(mappings)},
            created_at=now,
        )
        self._session.commit()
        return {"id": str(receipt_id), "sha256": sha256, "mappings": mappings, "replayed": False}

    def list_records(
        self,
        *,
        context: AuthorizationContext,
        extension_key: str,
        entity_name: str | None = None,
    ) -> list[ExtensionDataRecord]:
        prepare_project_scope(self._session, context, permission=Permission.PROJECT_READ)
        assert context.organization_id is not None and context.project_id is not None
        # Retained Core data remains readable after deactivation or a legacy
        # manifest fails mutation validation, just as in export_bundle.
        # Keep project authorization/RLS and the activation existence check.
        self._activation_row(context, extension_key)
        sql = """
            SELECT * FROM extensions.records
            WHERE organization_id = :org AND project_id = :proj AND extension_key = :key
        """
        params: dict[str, Any] = {
            "org": context.organization_id,
            "proj": context.project_id,
            "key": extension_key,
        }
        if entity_name:
            sql += " AND entity_name = :ename"
            params["ename"] = entity_name
        sql += " ORDER BY created_at DESC LIMIT 200"
        rows = self._session.execute(text(sql), params).mappings().all()
        return [_rec_from_row(r) for r in rows]
