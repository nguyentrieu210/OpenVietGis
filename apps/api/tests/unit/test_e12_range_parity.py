"""Shared synthetic immutable range fixtures for the E12 Rust policy candidate."""

from pathlib import Path

import pytest

from openvietgis_api.adapters.http.external_provider_transport import fetch_provider_range
from openvietgis_api.domains.external_sources.provider import ProviderUnavailable
from tests.unit.test_external_provider import HOST, FixtureProvider

FIXTURES = Path(__file__).resolve().parents[4] / "fixtures/064-e12/cog-ranges.tsv"
CASES = [line.split("|") for line in FIXTURES.read_text().splitlines() if not line.startswith("#")]


@pytest.mark.parametrize("case", CASES, ids=[case[0] for case in CASES])
def test_e12_shared_range_policy(monkeypatch, case):
    (
        name,
        accepted,
        first,
        last,
        pin_tag,
        pin_total,
        status,
        media,
        encoding,
        location,
        content_range,
        tag,
        body_len,
    ) = case
    provider = FixtureProvider(monkeypatch)
    headers = f"Content-Range: {content_range}\r\nETag: {tag}\r\n"
    if encoding:
        headers += f"Content-Encoding: {encoding}\r\n"
    if location:
        headers += f"Location: {location}\r\n"
    provider.response = lambda _path: (f"{status} Fixture", media, b"x" * int(body_len), headers)
    args = (
        f"https://{HOST}/cog.tif",
        int(first),
        int(last),
        pin_tag or None,
        int(pin_total) if pin_total else None,
        2.0,
    )
    if accepted == "1":
        result = fetch_provider_range(*args)
        assert len(result.data) == int(body_len), name
        assert result.etag == tag
        assert result.total == int(content_range.rsplit("/", 1)[1])
    else:
        with pytest.raises(ProviderUnavailable):
            fetch_provider_range(*args)
