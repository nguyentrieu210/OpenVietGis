"""Server-only invitation email transport. The one-time secret is never logged."""

from __future__ import annotations

from html import escape
from typing import Literal
from urllib.parse import urlencode
from uuid import UUID

import httpx

from openvietgis_api.config import Settings

DeliveryStatus = Literal["accepted_by_provider", "rejected", "unknown"]


def deliver_invitation(
    settings: Settings,
    *,
    recipient: str,
    organization_name: str,
    organization_id: UUID,
    invitation_id: UUID,
    secret: str,
) -> DeliveryStatus:
    if not settings.resend_api_key or not settings.invitation_from_email:
        return "rejected"
    fragment = urlencode(
        {
            "organization_id": str(organization_id),
            "invitation_id": str(invitation_id),
            "secret": secret,
        }
    )
    # A fragment is not sent in HTTP requests. The browser removes it before
    # beginning the OIDC round trip and submits it only to the existing accept API.
    link = settings.web_base_url.rstrip("/") + settings.invitation_web_path + "#" + fragment
    safe_org, safe_link = escape(organization_name), escape(link, quote=True)
    html = (
        '<div style="font:16px system-ui;color:#172a50;max-width'
        ':480px;margin:auto;padding:24px"><p style="font-size:12'
        'px;letter-spacing:2px;color:#2159d1">OPENVIETGIS '
        "STUDIO</p><h2>Bạn được mời tham gia nhóm</h2><p>Tổ chức <strong>"
        f"{safe_org}"
        "</strong> đã gửi lời mời cho bạn.</p><p>Nhấn nút bên "
        "dưới, tiếp tục bằng Google hoặc đăng nhập bằng email "
        "này, rồi chọn Tham gia tổ chức. Nếu chưa có tài khoản, "
        "bạn có thể đăng ký ngay. Lời mời có hiệu lực 48 "
        'giờ.</p><p><a href="'
        f"{safe_link}"
        '" style="display:inline-block;background:#2159d1;color:'
        "#fff;padding:14px 24px;border-radius:8px;text-decoratio"
        'n:none">Tham gia OpenVietGIS</a></p><p '
        'style="color:#687b95;font-size:13px">Nếu bạn không '
        "mong đợi lời mời này, bỏ qua thư.</p></div>"
    )
    try:
        result = httpx.post(
            "https://api.resend.com/emails",
            headers={
                "Authorization": "Bearer " + settings.resend_api_key.get_secret_value(),
                "Idempotency-Key": "ovg-invite-" + str(invitation_id),
            },
            json={
                "from": settings.invitation_from_email,
                "to": [recipient],
                "subject": "OpenVietGIS · Lời mời tham gia nhóm",
                "html": html,
                "text": f"Bạn được mời vào {organization_name}. Mở lời mời: {link}\n"
                "Lời mời dùng một lần, hết hạn sau 48 giờ.",
            },
            timeout=12,
            follow_redirects=False,
        )
    except httpx.RequestError:
        # A timeout can mean the provider accepted the request; never assert failure
        # or immediately issue another invitation on the user's behalf.
        return "unknown"
    if 200 <= result.status_code < 300:
        try:
            if isinstance(result.json().get("id"), str):
                return "accepted_by_provider"
        except (ValueError, AttributeError):
            pass
        return "unknown"
    if 400 <= result.status_code < 500:
        return "rejected"
    return "unknown"
