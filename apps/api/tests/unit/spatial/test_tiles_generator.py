import time

import pytest

from openvietgis_api.domains.spatial.tiles_generator import (
    TileGenerationError,
    build_tiles,
    ecef,
    frame,
)


def rectangle(x, y, size=0.00005, height=10):
    a, b, c, d = [x, y], [x + size, y], [x + size, y + size], [x, y + size]
    return {
        "geometry": {"type": "Polygon", "coordinates": [[a, b, c, d, a]]},
        "triangles": {
            "type": "GeometryCollection",
            "geometries": [
                {"type": "Polygon", "coordinates": [[a, b, c, a]]},
                {"type": "Polygon", "coordinates": [[a, c, d, a]]},
            ],
        },
        "height": height,
    }


def test_wgs84_and_scale():
    assert ecef(0, 0, 0) == (6378137, 0, 0)
    matrix, local = frame(106, 10, 20)
    assert len(matrix) == 16
    x, y, z = local((106, 10, 45))
    assert abs(x) < 1e-8 and abs(z) < 1e-8 and abs(y - 25) < 1e-8
    rows = [rectangle(106 + (i % 100) * 0.0002, 10 + (i // 100) * 0.0002) for i in range(3000)]
    start = time.monotonic()
    manifest, resources, total = build_tiles(rows, 20)
    elapsed = time.monotonic() - start
    assert total == 36000 and 1 < len(resources) <= 128
    assert len(manifest["root"]["children"]) == len(resources)
    assert max(len(v) for v in resources.values()) < 10 * 1024 * 1024
    print(
        f"Generated {len(resources)} tiles / {total} triangles / "
        f"3000 synthetic buildings in {elapsed:.2f}s"
    )


@pytest.mark.parametrize("height", [0, -1, None, "NaN", "3001", "0.001", True])
def test_bad_height(height):
    with pytest.raises(TileGenerationError):
        build_tiles([rectangle(106, 10, height=height)], 0)


def test_bad_extent():
    with pytest.raises(TileGenerationError):
        build_tiles([rectangle(106, 10, size=0.03)], 0)
