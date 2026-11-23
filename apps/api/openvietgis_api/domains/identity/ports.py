from __future__ import annotations

from typing import Protocol
from uuid import UUID

from openvietgis_api.domains.identity.domain import (
    IdentityLink,
    UnlinkIdentityResult,
    User,
    WebSession,
)


class IdentityRepository(Protocol):
    def lock_external_identity(self, *, issuer: str, subject: str) -> None: ...

    def find_by_external_identity(
        self,
        *,
        issuer: str,
        subject: str,
    ) -> tuple[User, IdentityLink] | None: ...

    def create_user_with_identity(
        self,
        user: User,
        identity_link: IdentityLink,
    ) -> None: ...

    def try_add_identity_link(self, identity_link: IdentityLink) -> bool: ...

    def list_identity_links(self, user_id: UUID) -> list[IdentityLink]: ...

    def count_identity_links(self, user_id: UUID) -> int: ...

    def unlink_identity(self, *, user_id: UUID, identity_link_id: UUID) -> UnlinkIdentityResult: ...

    def refresh_identity_login(self, identity_link: IdentityLink) -> None: ...

    def create_web_session(self, web_session: WebSession) -> None: ...

    def revoke_web_session(self, session_id: UUID) -> None: ...

    def get_user(self, user_id: UUID) -> User | None: ...


class AuditAppender(Protocol):
    def append(self, event: object) -> None: ...
