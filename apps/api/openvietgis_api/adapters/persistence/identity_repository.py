from __future__ import annotations

from uuid import UUID

from sqlalchemy import text
from sqlalchemy.engine import RowMapping
from sqlalchemy.orm import Session

from openvietgis_api.domains.identity.domain import (
    IdentityLink,
    UnlinkIdentityResult,
    User,
    UserStatus,
    WebSession,
)


def _user_from_row(mapping: RowMapping) -> User:
    return User(
        id=mapping["user_id"],
        display_name=mapping["display_name"],
        email_snapshot=mapping["user_email_snapshot"],
        status=UserStatus(mapping["user_status"]),
        created_at=mapping["user_created_at"],
        updated_at=mapping["user_updated_at"],
    )


def _identity_from_row(mapping: RowMapping) -> IdentityLink:
    return IdentityLink(
        id=mapping["identity_link_id"],
        user_id=mapping["user_id"],
        issuer=mapping["issuer"],
        subject=mapping["subject"],
        email_snapshot=mapping["identity_email_snapshot"],
        email_verified=mapping["email_verified"],
        display_name_snapshot=mapping["display_name_snapshot"],
        created_at=mapping["identity_created_at"],
        last_login_at=mapping["last_login_at"],
    )


class SqlIdentityRepository:
    def __init__(self, session: Session) -> None:
        self._session = session

    def lock_external_identity(self, *, issuer: str, subject: str) -> None:
        # Claim the verified external identity for this transaction, including when
        # it has no existing row to lock. Hash collisions only serialize requests.
        key = f"openvietgis.identity:{len(issuer)}:{issuer}{subject}"
        self._session.execute(
            text("SELECT pg_advisory_xact_lock(hashtextextended(:key, 0))"), {"key": key}
        )

    def find_by_external_identity(
        self,
        *,
        issuer: str,
        subject: str,
    ) -> tuple[User, IdentityLink] | None:
        row = (
            self._session.execute(
                text(
                    """
                    SELECT
                        u.id AS user_id,
                        u.display_name,
                        u.email_snapshot AS user_email_snapshot,
                        u.status AS user_status,
                        u.created_at AS user_created_at,
                        u.updated_at AS user_updated_at,
                        l.id AS identity_link_id,
                        l.issuer,
                        l.subject,
                        l.email_snapshot AS identity_email_snapshot,
                        l.email_verified,
                        l.display_name_snapshot,
                        l.created_at AS identity_created_at,
                        l.last_login_at
                    FROM identity.identity_links AS l
                    JOIN identity.users AS u ON u.id = l.user_id
                    WHERE l.issuer = :issuer AND l.subject = :subject
                    """
                ),
                {"issuer": issuer, "subject": subject},
            )
            .mappings()
            .one_or_none()
        )
        if row is None:
            return None
        return _user_from_row(row), _identity_from_row(row)

    def create_user_with_identity(self, user: User, identity_link: IdentityLink) -> None:
        self._session.execute(
            text(
                """
                INSERT INTO identity.users (
                    id, display_name, email_snapshot, status, created_at, updated_at
                )
                VALUES (:id, :display_name, :email_snapshot, :status, :created_at, :updated_at)
                """
            ),
            {
                "id": user.id,
                "display_name": user.display_name,
                "email_snapshot": user.email_snapshot,
                "status": user.status.value,
                "created_at": user.created_at,
                "updated_at": user.updated_at,
            },
        )
        self.add_identity_link(identity_link)

    def add_identity_link(self, identity_link: IdentityLink) -> None:
        self._insert_identity_link(identity_link, ignore_identity_conflict=False)

    def try_add_identity_link(self, identity_link: IdentityLink) -> bool:
        return self._insert_identity_link(identity_link, ignore_identity_conflict=True)

    def _insert_identity_link(
        self, identity_link: IdentityLink, *, ignore_identity_conflict: bool
    ) -> bool:
        statement = """
            INSERT INTO identity.identity_links (
                id, user_id, issuer, subject, email_snapshot, email_verified,
                display_name_snapshot, created_at, last_login_at
            )
            VALUES (
                :id, :user_id, :issuer, :subject, :email_snapshot, :email_verified,
                :display_name_snapshot, :created_at, :last_login_at
            )
            """
        if ignore_identity_conflict:
            # Only suppress this expected unique-key conflict. Other persistence
            # errors propagate and roll back the complete request.
            statement += " ON CONFLICT (issuer, subject) DO NOTHING RETURNING id"
        result = self._session.execute(
            text(statement),
            {
                "id": identity_link.id,
                "user_id": identity_link.user_id,
                "issuer": identity_link.issuer,
                "subject": identity_link.subject,
                "email_snapshot": identity_link.email_snapshot,
                "email_verified": identity_link.email_verified,
                "display_name_snapshot": identity_link.display_name_snapshot,
                "created_at": identity_link.created_at,
                "last_login_at": identity_link.last_login_at,
            },
        )
        if ignore_identity_conflict:
            return result.scalar_one_or_none() is not None
        return True

    def list_identity_links(self, user_id: UUID) -> list[IdentityLink]:
        rows = (
            self._session.execute(
                text(
                    """
                    SELECT id AS identity_link_id, user_id, issuer, subject,
                           email_snapshot AS identity_email_snapshot, email_verified,
                           display_name_snapshot, created_at AS identity_created_at, last_login_at
                    FROM identity.identity_links
                    WHERE user_id = :user_id
                    ORDER BY created_at
                    """
                ),
                {"user_id": user_id},
            )
            .mappings()
            .all()
        )
        return [_identity_from_row(row) for row in rows]

    def count_identity_links(self, user_id: UUID) -> int:
        return int(
            self._session.execute(
                text("SELECT count(*) FROM identity.identity_links WHERE user_id = :user_id"),
                {"user_id": user_id},
            ).scalar_one()
        )

    def unlink_identity(self, *, user_id: UUID, identity_link_id: UUID) -> UnlinkIdentityResult:
        # One guarded database operation owns the count/delete invariant.
        result = self._session.execute(
            text("SELECT identity.unlink_identity(:user_id, :identity_link_id)"),
            {"user_id": user_id, "identity_link_id": identity_link_id},
        ).scalar_one()
        return UnlinkIdentityResult(result)

    def lock_active_web_session(self, *, user_id: UUID, session_id: UUID) -> bool:
        # Serialize the OIDC link mutation with logout or unlinking its login identity.
        locked_id = self._session.execute(
            text(
                """
                SELECT id
                FROM identity.web_sessions
                WHERE id = :session_id
                  AND user_id = :user_id
                  AND revoked_at IS NULL
                  AND expires_at > clock_timestamp()
                FOR UPDATE
                """
            ),
            {"session_id": session_id, "user_id": user_id},
        ).scalar_one_or_none()
        return locked_id is not None

    def refresh_identity_login(self, identity_link: IdentityLink) -> None:
        self._session.execute(
            text(
                """
                UPDATE identity.identity_links
                SET last_login_at = :last_login_at,
                    email_snapshot = :email_snapshot,
                    email_verified = :email_verified,
                    display_name_snapshot = :display_name_snapshot
                WHERE id = :identity_link_id AND user_id = :user_id
                """
            ),
            {
                "identity_link_id": identity_link.id,
                "user_id": identity_link.user_id,
                "last_login_at": identity_link.last_login_at,
                "email_snapshot": identity_link.email_snapshot,
                "email_verified": identity_link.email_verified,
                "display_name_snapshot": identity_link.display_name_snapshot,
            },
        )

    def create_web_session(self, web_session: WebSession) -> None:
        self._session.execute(
            text(
                """
                INSERT INTO identity.web_sessions (
                    id, user_id, identity_link_id, secret_hash,
                    issued_at, expires_at, last_seen_at, revoked_at
                )
                VALUES (
                    :id, :user_id, :identity_link_id, :secret_hash,
                    :issued_at, :expires_at, :last_seen_at, :revoked_at
                )
                """
            ),
            {
                "id": web_session.id,
                "user_id": web_session.user_id,
                "identity_link_id": web_session.identity_link_id,
                "secret_hash": web_session.secret_hash,
                "issued_at": web_session.issued_at,
                "expires_at": web_session.expires_at,
                "last_seen_at": web_session.last_seen_at,
                "revoked_at": web_session.revoked_at,
            },
        )

    def revoke_web_session(self, session_id: UUID) -> None:
        self._session.execute(
            text("UPDATE identity.web_sessions SET revoked_at = now() WHERE id = :session_id"),
            {"session_id": session_id},
        )

    def get_user(self, user_id: UUID) -> User | None:
        row = (
            self._session.execute(
                text(
                    """
                    SELECT id AS user_id, display_name,
                           email_snapshot AS user_email_snapshot, status AS user_status,
                           created_at AS user_created_at, updated_at AS user_updated_at
                    FROM identity.users
                    WHERE id = :user_id
                    """
                ),
                {"user_id": user_id},
            )
            .mappings()
            .one_or_none()
        )
        if row is None:
            return None
        return _user_from_row(row)
