from uuid import uuid4

import pytest

from openvietgis_api.domains.authorization.context import Actor, AuthorizationContext
from openvietgis_api.domains.authorization.permissions import Permission
from openvietgis_api.domains.authorization.service import (
    AuthorizationDeniedError,
    AuthorizationService,
)


class StaticPermissionProvider:
    def __init__(self, permissions: frozenset[Permission]) -> None:
        self.permissions = permissions

    def resolve(self, context: AuthorizationContext) -> frozenset[Permission]:
        assert context.organization_id
        return self.permissions


def test_authorization_service_allows_effective_permission() -> None:
    service = AuthorizationService(StaticPermissionProvider(frozenset({Permission.PROJECT_READ})))
    context = AuthorizationContext(
        actor=Actor(actor_type="human", actor_id=uuid4(), display_name="Tester"),
        organization_id=uuid4(),
    )

    service.require(context, Permission.PROJECT_READ)


def test_authorization_service_denies_missing_permission() -> None:
    service = AuthorizationService(StaticPermissionProvider(frozenset()))
    context = AuthorizationContext(
        actor=Actor(actor_type="human", actor_id=uuid4(), display_name="Tester"),
        organization_id=uuid4(),
    )

    with pytest.raises(AuthorizationDeniedError):
        service.require(context, Permission.PROJECT_READ)
