from openvietgis_api.domains.audit.service import redact_audit_metadata


def test_audit_redaction_removes_all_secret_classes_recursively() -> None:
    metadata = {
        "session": "ovgs_0123456789abcdef0123456789abcdef.s_AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        "oidc": {
            "access_token": "eyJheader.payload.signature",
            "issuer": "https://id.test",
        },
        "invitation": "inv_BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB",
        "service": ("ovgsvc0123456789abcdef0123456789abcdef_CCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCC"),
        "bootstrap_secret": "local-bootstrap-canary",
        "safe": {"organization_id": "org-1", "count": 3},
    }

    redacted = redact_audit_metadata(metadata)

    assert redacted["session"] == "[REDACTED]"
    assert redacted["oidc"]["access_token"] == "[REDACTED]"
    assert redacted["oidc"]["issuer"] == "https://id.test"
    assert redacted["invitation"] == "[REDACTED]"
    assert redacted["service"] == "[REDACTED]"
    assert redacted["bootstrap_secret"] == "[REDACTED]"
    assert redacted["safe"] == {"organization_id": "org-1", "count": 3}


def test_audit_redaction_uses_sensitive_keys_even_for_unknown_secret_format() -> None:
    redacted = redact_audit_metadata(
        {
            "provider_token": "opaque-provider-value",
            "client_secret": "opaque-client-value",
            "nested": [{"authorization": "Bearer opaque-value"}],
        }
    )
    assert redacted == {
        "provider_token": "[REDACTED]",
        "client_secret": "[REDACTED]",
        "nested": [{"authorization": "[REDACTED]"}],
    }
