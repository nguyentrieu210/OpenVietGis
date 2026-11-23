"""Human project feature leases, acquired under the existing source write lock."""

from uuid import UUID

from fastapi import APIRouter
from pydantic import BaseModel, ConfigDict, Field
from sqlalchemy import text

from openvietgis_api.adapters.http.team_chat import BASE, DB, ReadAuth, WriteAuth, scope
from openvietgis_api.domains.editing.domain import validate_feature_id
from openvietgis_api.domains.platform_errors import PlatformConflictError, PlatformNotFoundError

router = APIRouter(tags=["team-locks"])
TABLE = "collaboration.feature_edit_leases"
WHERE = "organization_id=:o AND project_id=:p"
COLS = "source_id,feature_id,actor_id,actor_name,expires_at"


class LeaseRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")
    source_id: UUID
    feature_id: str = Field(min_length=1, max_length=128)
    token: UUID


@router.get(BASE + "/edit-locks")
def list_leases(organization_id: UUID, project_id: UUID, auth: ReadAuth, session: DB):
    scope(session, auth, organization_id, project_id)
    return {
        "items": [
            dict(r)
            for r in session.execute(
                text(
                    f"SELECT {COLS} FROM {TABLE} WHERE {WHERE} AND expires_at>clock_timestamp() "
                    f"ORDER BY source_id,feature_id LIMIT 2000"
                ),
                {"o": organization_id, "p": project_id},
            ).mappings()
        ]
    }


@router.post(BASE + "/edit-locks")
def acquire(
    organization_id: UUID, project_id: UUID, body: LeaseRequest, auth: WriteAuth, session: DB
):
    ctx = scope(session, auth, organization_id, project_id, write=True)
    args = {
        "o": organization_id,
        "p": project_id,
        "s": body.source_id,
        "f": validate_feature_id(body.feature_id),
        "a": ctx.actor.actor_id,
        "n": ctx.actor.display_name[:200],
        "t": body.token,
    }
    # Same serialization boundary as SqlEditingRepository.commit_changeset.
    source = session.execute(
        text(f"""SELECT id FROM spatial.data_sources WHERE {WHERE}
      AND id=:s AND lifecycle='active' AND validation_state='ready' AND kind<>'reference_fixture'
      FOR UPDATE"""),
        args,
    ).first()
    if (
        not source
        or not session.execute(
            text(
                f"SELECT 1 FROM spatial.reference_features WHERE {WHERE} AND source_id=:s AND "
                f"feature_id=:f"
            ),
            args,
        ).first()
    ):
        raise PlatformNotFoundError("editable feature not found")
    session.execute(
        text(
            f"DELETE FROM {TABLE} WHERE {WHERE} AND source_id=:s AND expires_at<=clock_timestamp()"
        ),
        args,
    )
    existing = (
        session.execute(
            text(
                f"SELECT actor_id,token FROM {TABLE} WHERE {WHERE} AND source_id=:s AND "
                f"feature_id=:f"
            ),
            args,
        )
        .mappings()
        .first()
    )
    if existing:
        if existing["actor_id"] != args["a"] or existing["token"] != args["t"]:
            raise PlatformConflictError("feature already being edited", code="FEATURE_LOCKED")
        result = (
            session.execute(
                text(f"""UPDATE {TABLE} SET expires_at=clock_timestamp()+interval '45 seconds',
                actor_name=:n
          WHERE {WHERE} AND source_id=:s AND feature_id=:f AND actor_id=:a AND token=:t RETURNING
          {COLS}"""),
                args,
            )
            .mappings()
            .one()
        )
    else:
        result = (
            session.execute(
                text(f"""INSERT INTO {TABLE} (organization_id,project_id,source_id,feature_id,
                actor_id,actor_name,token,expires_at)
          VALUES (:o,:p,:s,:f,:a,:n,:t,clock_timestamp()+interval '45 seconds') RETURNING
          {COLS}"""),
                args,
            )
            .mappings()
            .one()
        )
    return dict(result)


@router.post(BASE + "/edit-locks/release")
def release(
    organization_id: UUID, project_id: UUID, body: LeaseRequest, auth: WriteAuth, session: DB
):
    ctx = scope(session, auth, organization_id, project_id, write=True)
    args = {
        "o": organization_id,
        "p": project_id,
        "s": body.source_id,
        "f": validate_feature_id(body.feature_id),
        "a": ctx.actor.actor_id,
        "t": body.token,
    }
    session.execute(
        text(f"SELECT id FROM spatial.data_sources WHERE {WHERE} AND id=:s FOR UPDATE"), args
    )
    session.execute(
        text(
            f"DELETE FROM {TABLE} WHERE {WHERE} AND source_id=:s AND feature_id=:f AND "
            f"actor_id=:a AND token=:t"
        ),
        args,
    )
    return {"released": True}
