"""Transactional invalidation of managed clients when GIS access changes."""

from __future__ import annotations

import json
from uuid import UUID

from sqlalchemy import text
from sqlalchemy.orm import Session


def bump_project_permission_epoch(session: Session, org: UUID, project: UUID) -> None:
    session.execute(
        text("""
            INSERT INTO integration_hub.project_event_counters
                (organization_id, project_id, last_cursor, permission_epoch,
                 retention_floor, updated_at)
            VALUES (:org, :project, 0, 2, 0, now())
            ON CONFLICT (organization_id, project_id) DO UPDATE
            SET permission_epoch=integration_hub.project_event_counters.permission_epoch+1,
                updated_at=now()
        """),
        {"org": org, "project": project},
    )
    session.execute(
        text("SELECT pg_notify('ovg_team_collaboration', :payload)"),
        {"payload": json.dumps({"o": str(org), "p": str(project), "kind": "session_revoked"})},
    )
