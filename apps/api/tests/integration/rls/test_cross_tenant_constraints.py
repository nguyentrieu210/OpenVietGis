from __future__ import annotations

from uuid import uuid4

import pytest
from sqlalchemy import text
from sqlalchemy.exc import DBAPIError
from sqlalchemy.orm import Session

from openvietgis_api.adapters.persistence.db import get_engine
from openvietgis_api.adapters.persistence.rls import set_request_context
from tests.fixtures.identity_tenancy import seed_access_world


def test_runtime_cannot_insert_or_reference_foreign_tenant(
    migrated_database: None,
) -> None:
    a, b = seed_access_world(), seed_access_world()
    values = {
        "a": a.organization_id,
        "b": b.organization_id,
        "actor": a.owner_user_id,
        "workspace": b.workspace_a_id,
        "project": b.project_id,
        "member": a.member_membership_id,
        "foreign_member": b.member_membership_id,
        "a_project": a.project_id,
    }
    statements = (
        """INSERT INTO workspace.workspaces
            (id, organization_id, name, slug, status, created_by_actor_type,
            created_by_actor_id, created_at, updated_at)
            VALUES (:id,:b,'Foreign','cross-write','active','human',:actor,now(),now())""",
        """UPDATE workspace.projects SET workspace_id=:workspace
           WHERE id=:a_project AND organization_id=:a""",
        """INSERT INTO workspace.projects
            (id, organization_id, workspace_id, name, slug, status,
             created_by_actor_type, created_by_actor_id, created_at, updated_at)
            VALUES (:id,:a,:workspace,'Cross','cross-project','active',
                    'human',:actor,now(),now())""",
        """INSERT INTO org.resource_grants
            (id, organization_id, subject_kind, membership_id, scope_kind,
             role, created_at, created_by_actor_type, created_by_actor_id)
            VALUES (:id,:a,'membership',:foreign_member,'organization',
                    'viewer',now(),'human',:actor)""",
        """INSERT INTO org.resource_grants
            (id, organization_id, subject_kind, membership_id, scope_kind, workspace_id,
             role, created_at, created_by_actor_type, created_by_actor_id)
            VALUES (:id,:a,'membership',:member,'workspace',:workspace,
                    'viewer',now(),'human',:actor)""",
        """INSERT INTO org.resource_grants
            (id, organization_id, subject_kind, membership_id, scope_kind, project_id,
             role, created_at, created_by_actor_type, created_by_actor_id)
            VALUES (:id,:a,'membership',:member,'project',:project,
                    'viewer',now(),'human',:actor)""",
        """INSERT INTO org.invitations
            (id, organization_id, status, target_kind, target_email_normalized,
             organization_role, resource_scope_kind, resource_workspace_id,
             resource_role, token_hash, token_prefix, expires_at, created_at,
             created_by_actor_type, created_by_actor_id)
            VALUES (:id,:a,'pending','email','rls@example.test','member',
                    'workspace',:workspace,'viewer',decode('0102','hex'),'test',
                    now()+interval '1 hour',now(),'human',:actor)""",
        """INSERT INTO org.invitations
            (id, organization_id, status, target_kind, target_email_normalized,
             organization_role, resource_scope_kind, resource_project_id,
             resource_role, token_hash, token_prefix, expires_at, created_at,
             created_by_actor_type, created_by_actor_id)
            VALUES (:id,:a,'pending','email','rls@example.test','member',
                    'project',:project,'viewer',decode('0102','hex'),'test',
                    now()+interval '1 hour',now(),'human',:actor)""",
    )
    with Session(bind=get_engine()) as runtime:
        set_request_context(
            runtime,
            organization_id=a.organization_id,
            actor_user_id=a.owner_user_id,
        )
        for statement in statements:
            with pytest.raises(DBAPIError), runtime.begin_nested():
                runtime.execute(text(statement), {**values, "id": uuid4()})
        assert (
            runtime.execute(
                text("SELECT workspace_id FROM workspace.projects WHERE id=:a_project"), values
            ).scalar_one()
            == a.workspace_a_id
        )
        runtime.rollback()
