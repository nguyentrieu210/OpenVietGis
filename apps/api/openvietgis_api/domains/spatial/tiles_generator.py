"""Generate bounded, portable 3D Tiles GLBs from pre-triangulated WGS84 polygons."""

import json
import math
import re
import struct
from collections import defaultdict
from decimal import Decimal

MAX_FEATURES = 3000
MAX_VERTICES = 60000
MAX_TILE_TRIANGLES = 30000
MAX_TILES = 128
DEG = math.pi / 180
A = 6378137.0
E2 = 6.6943799901413165e-3


class TileGenerationError(ValueError):
    pass


def require(ok, message):
    if not ok:
        raise TileGenerationError(message)


def ecef(lon, lat, height):
    lon, lat = lon * DEG, lat * DEG
    n = A / math.sqrt(1 - E2 * math.sin(lat) ** 2)
    return (
        (n + height) * math.cos(lat) * math.cos(lon),
        (n + height) * math.cos(lat) * math.sin(lon),
        (n * (1 - E2) + height) * math.sin(lat),
    )


def frame(lon, lat, height):
    l, p = lon * DEG, lat * DEG
    east = (-math.sin(l), math.cos(l), 0)
    north = (-math.sin(p) * math.cos(l), -math.sin(p) * math.sin(l), math.cos(p))
    up = (math.cos(p) * math.cos(l), math.cos(p) * math.sin(l), math.sin(p))
    origin = ecef(lon, lat, height)
    matrix = [*east, 0, *north, 0, *up, 0, *origin, 1]

    def local(point):
        x, y, z = ecef(*point)
        d = (x - origin[0], y - origin[1], z - origin[2])
        e, n, u = (sum(a * b for a, b in zip(d, axis, strict=True)) for axis in (east, north, up))
        require(max(abs(e), abs(n), abs(u)) <= 10000, "Tile local coordinates exceed 10 km")
        return e, u, -n  # glTF +Y up, +Z south

    return matrix, local


def orient(ring, ccw):
    # Subtract a common origin to avoid cancellation on small geographic polygons.
    x, y = ring[0]
    area = sum(
        (a[0] - x) * (b[1] - y) - (b[0] - x) * (a[1] - y)
        for a, b in zip(ring, ring[1:], strict=False)
    )
    require(area != 0 and ring[0] == ring[-1], "Degenerate polygon ring")
    return ring if (area > 0) == ccw else list(reversed(ring))


def feature_mesh(geometry, triangles, height, base):
    if isinstance(height, str):
        # The platform wire format represents exact decimal attributes as strings.
        require(
            re.fullmatch(r"[0-9]{1,4}(?:\.[0-9]{1,18})?", height), "Invalid exact decimal height"
        )
        value = Decimal(height)
        require(Decimal("0.01") <= value <= 3000, "Height outside [0.01,3000] m")
        height = float(value)
    require(
        type(height) in (int, float) and math.isfinite(height) and 0.01 <= height <= 3000,
        "Every feature needs a metre height in [0.01,3000]",
    )
    require(
        geometry["type"] == "Polygon" and triangles["type"] == "GeometryCollection",
        "Expected triangulated polygon",
    )
    rings = geometry["coordinates"]
    require(bool(rings), "Empty polygon")
    coordinates = [p for ring in rings for p in ring]
    require(
        all(
            len(p) == 2 and -179.99 <= p[0] <= 179.99 and -84.9 <= p[1] <= 84.9 for p in coordinates
        ),
        "Unsupported geographic extent",
    )
    west, south = (min(p[i] for p in coordinates) for i in (0, 1))
    east, north = (max(p[i] for p in coordinates) for i in (0, 1))
    require(east - west <= 0.02 and north - south <= 0.02, "Footprint exceeds 0.02 degree extent")
    top = base + height
    mesh = []

    def point(p, h):
        return (*p, h)

    for triangle in triangles["geometries"]:
        require(
            triangle["type"] == "Polygon" and len(triangle["coordinates"]) == 1,
            "Invalid triangulation",
        )
        ring = triangle["coordinates"][0]
        require(len(ring) == 4, "Invalid triangle")
        a, b, c = orient(ring, True)[:3]
        mesh.extend(
            (
                point(a, top),
                point(b, top),
                point(c, top),
                point(a, base),
                point(c, base),
                point(b, base),
            )
        )
    require(bool(mesh), "Polygon has no triangles")
    for i, ring in enumerate(rings):
        ring = orient(ring, i == 0)
        for a, b in zip(ring, ring[1:], strict=False):
            require(a != b, "Repeated boundary vertex")
            mesh.extend(
                (
                    point(a, base),
                    point(b, base),
                    point(b, top),
                    point(a, base),
                    point(b, top),
                    point(a, top),
                )
            )
    return mesh, (west, south, east, north, base, top)


def union_bounds(bounds):
    return [
        min(b[i] for b in bounds) if i in (0, 1, 4) else max(b[i] for b in bounds) for i in range(6)
    ]


def region(bounds):
    w, s, e, n, h, H = bounds
    # Padded bounds cover float32 roundoff, WGS84 curvature between vertices and straight edges.
    return [w * DEG - 1e-6, s * DEG - 1e-6, e * DEG + 1e-6, n * DEG + 1e-6, h - 5, H + 5]


def glb(points, local):
    xyz = [local(p) for p in points]
    binary = struct.pack("<" + "f" * (3 * len(xyz)), *(v for p in xyz for v in p))
    # accessor bounds must enclose the actual stored float32 values, not the pre-round values.
    stored = struct.unpack("<" + "f" * (3 * len(xyz)), binary)
    doc = {
        "asset": {"version": "2.0"},
        "buffers": [{"byteLength": len(binary)}],
        "bufferViews": [{"buffer": 0, "byteLength": len(binary), "target": 34962}],
        "accessors": [
            {
                "bufferView": 0,
                "componentType": 5126,
                "count": len(xyz),
                "type": "VEC3",
                "min": [min(stored[i::3]) for i in range(3)],
                "max": [max(stored[i::3]) for i in range(3)],
            }
        ],
        "materials": [
            {
                "pbrMetallicRoughness": {
                    "baseColorFactor": [0.24, 0.56, 0.79, 1],
                    "metallicFactor": 0,
                    "roughnessFactor": 1,
                }
            }
        ],
        "meshes": [{"primitives": [{"attributes": {"POSITION": 0}, "material": 0}]}],
        "nodes": [{"mesh": 0}],
        "scenes": [{"nodes": [0]}],
        "scene": 0,
    }
    raw = json.dumps(doc, separators=(",", ":")).encode()
    raw += b" " * (-len(raw) % 4)
    return (
        struct.pack("<5I", 0x46546C67, 2, 28 + len(raw) + len(binary), len(raw), 0x4E4F534A)
        + raw
        + struct.pack("<2I", len(binary), 0x004E4942)
        + binary
    )


def build_tiles(rows, base):
    require(1 <= len(rows) <= MAX_FEATURES, "Feature count outside generator budget")
    require(math.isfinite(base) and -1000 <= base <= 10000, "Unsupported base elevation")
    buckets = defaultdict(list)
    total = 0
    vertex_count = 0
    for row in rows:
        geometry, triangles, height = row["geometry"], row["triangles"], row["height"]
        vertex_count += sum(len(r) for r in geometry["coordinates"])
        require(vertex_count <= MAX_VERTICES, "Vertex budget exceeded")
        mesh, bounds = feature_mesh(geometry, triangles, height, base)
        total += len(mesh) // 3
        require(len(mesh) // 3 <= MAX_TILE_TRIANGLES, "Feature exceeds per-tile triangle budget")
        bucket = (
            math.floor((bounds[0] + bounds[2]) / 0.02),
            math.floor((bounds[1] + bounds[3]) / 0.02),
        )
        buckets[bucket].append((mesh, bounds))
    resources = {}
    children = []

    def emit(mesh, bounds):
        require(len(children) < MAX_TILES, "Tile count exceeds 128")
        b = union_bounds(bounds)
        matrix, local = frame((b[0] + b[2]) / 2, (b[1] + b[3]) / 2, base)
        uri = f"tiles/tile-{len(children):04d}.glb"
        resources[uri] = glb(mesh, local)
        children.append(
            {
                "boundingVolume": {"region": region(b)},
                "geometricError": 0,
                "transform": matrix,
                "content": {"uri": uri},
            }
        )

    for _, features in sorted(buckets.items()):
        mesh, bounds = [], []
        for vertices, b in features:
            if len(mesh) + len(vertices) > 3 * MAX_TILE_TRIANGLES:
                emit(mesh, bounds)
                mesh, bounds = [], []
            mesh.extend(vertices)
            bounds.append(b)
        emit(mesh, bounds)
    root = union_bounds([c["boundingVolume"]["region"] for c in children])
    require(root[2] - root[0] <= math.pi, "Tileset spans more than 180 degrees")
    manifest = {
        "asset": {"version": "1.1"},
        "geometricError": 1000,
        "root": {
            "boundingVolume": {"region": root},
            "geometricError": 1000,
            "refine": "REPLACE",
            "children": children,
        },
    }
    return manifest, resources, total
