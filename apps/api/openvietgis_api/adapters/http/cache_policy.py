"""Prevent shared caches from retaining authentication and tenant denials."""

from starlette.datastructures import MutableHeaders
from starlette.types import ASGIApp, Message, Receive, Scope, Send


class PrivateDenialCachePolicy:
    """Match the native API policy without buffering HTTP or SSE bodies."""

    def __init__(self, app: ASGIApp) -> None:
        self.app = app

    async def __call__(self, scope: Scope, receive: Receive, send: Send) -> None:
        if scope["type"] != "http":
            await self.app(scope, receive, send)
            return

        async def private_send(message: Message) -> None:
            if message["type"] == "http.response.start" and message["status"] in (401, 403, 404):
                headers = MutableHeaders(scope=message)
                if "cache-control" not in headers:
                    headers["cache-control"] = "private, no-store"
            await send(message)

        await self.app(scope, receive, private_send)
