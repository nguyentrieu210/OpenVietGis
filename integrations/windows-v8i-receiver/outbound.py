"""R028 V8i local geometry binding + reviewed R026 Changeset write-back.

Cad geometry reads are COM attach-only. Local GeoPackage edits are strictly
limited to previously mapped feature IDs. No mutation of a DGN or of the
received baseline. Network commit is opt-in and idempotency journaled.
"""
from __future__ import annotations

import hashlib
import json
import math
import os
import shutil
import sqlite3
import struct
import tempfile
from contextlib import closing
from pathlib import Path
from typing import Any
from uuid import UUID

from receiver import _verify_geopackage
from windows_com import _element_id, _geometry, _running_app
from bridge_contract import _geometry as validate_geometry

MAX_SCAN = 5000
MAX_MAPPED = 100
MAP_FILE = "v8i-mappings.json"
PLAN_FILE = "outbound-plan.json"
OUT_FILE = "outbound-review.gpkg"
JOURNAL_FILE = "outbound-journal.json"


class CadSyncError(ValueError):
    pass


def digest(data: Any) -> str:
    return hashlib.sha256(json.dumps(data, sort_keys=True, separators=(",", ":"), allow_nan=False).encode()).hexdigest()


def _write_json(path: Path, value: dict[str, Any]) -> None:
    # Replaces metadata only; the original received GeoPackage is immutable.
    fd, temp = tempfile.mkstemp(prefix=".ovg-", dir=path.parent)
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as file:
            json.dump(value, file, indent=2, ensure_ascii=False, sort_keys=True)
            file.flush()
            os.fsync(file.fileno())
        os.replace(temp, path)
    finally:
        if os.path.exists(temp):
            os.unlink(temp)


def receipt(bundle: str | Path) -> tuple[Path, dict[str, Any]]:
    directory = Path(bundle).resolve()
    try:
        meta = json.loads((directory / "receipt.json").read_text(encoding="utf-8"))
        if meta["schema_version"] != "ovg.windows.inbound.v1" or meta["cad_write_back_enabled"] is not False:
            raise CadSyncError("Unsupported received bundle contract.")
        for key, filename in (("attributes_geopackage_sha256", "attributes.gpkg"), ("geometry_dxf_sha256", "geometry.dxf")):
            if hashlib.sha256((directory / filename).read_bytes()).hexdigest() != meta[key]:
                raise CadSyncError("Received bundle changed after download.")
        if not isinstance(meta["content_revision"], str) or len(meta["content_revision"]) != 64:
            raise CadSyncError("R026 SHA-256 revisions required for write-back.")
        if not isinstance(meta["schema_revision"], str) or len(meta["schema_revision"]) != 64:
            raise CadSyncError("R026 SHA-256 schema revision required.")
        UUID(meta["layer_id"])
        _verify_geopackage(directory / "attributes.gpkg", meta["crs"], meta["feature_count"])
        return directory, meta
    except CadSyncError:
        raise
    except (OSError, KeyError, ValueError, TypeError, json.JSONDecodeError) as exc:
        raise CadSyncError("Missing or invalid received data; download again.") from exc


def _read_geo(blob: bytes) -> dict[str, Any]:
    """Read strict 2D GeoPackage WKB; refuses unknown extensions, Z/M and holes."""
    if not isinstance(blob, bytes) or len(blob) < 13 or blob[:3] != b"GP\x00":
        raise CadSyncError("Invalid GeoPackage geometry header.")
    flags = blob[3]
    envelope = (flags >> 1) & 7
    if flags & 0xF0 or flags & 0x10 or envelope not in (0, 1, 2, 3, 4):
        raise CadSyncError("Unsupported GeoPackage geometry flags.")
    pos = 8 + {0: 0, 1: 32, 2: 48, 3: 48, 4: 64}[envelope]
    if len(blob) < pos + 5:
        raise CadSyncError("Truncated WKB.")
    endian = "<" if blob[pos] == 1 else ">" if blob[pos] == 0 else None
    if endian is None:
        raise CadSyncError("Bad WKB byte order.")
    kind = struct.unpack_from(endian + "I", blob, pos + 1)[0]
    pos += 5

    def number(fmt: str) -> Any:
        nonlocal pos
        size = struct.calcsize(fmt)
        if pos + size > len(blob):
            raise CadSyncError("Truncated WKB coordinates.")
        result = struct.unpack_from(endian + fmt, blob, pos)
        pos += size
        return result[0] if len(result) == 1 else result

    def coords() -> list[float]:
        a, b = number("dd")
        if not math.isfinite(a) or not math.isfinite(b):
            raise CadSyncError("Non-finite source coordinate.")
        return [a, b]

    def points() -> list[list[float]]:
        count = number("I")
        if count > 50000:
            raise CadSyncError("Too many vertices.")
        return [coords() for _ in range(count)]

    if kind == 1:
        geom = {"type": "Point", "coordinates": coords()}
    elif kind == 2:
        geom = {"type": "LineString", "coordinates": points()}
    elif kind == 3:
        rings = number("I")
        if rings != 1:
            raise CadSyncError("V8i bridge cannot safely edit polygons with holes.")
        geom = {"type": "Polygon", "coordinates": [points()]}
    else:
        raise CadSyncError("Only simple 2D Point/LineString/Polygon supported.")
    if pos != len(blob):
        raise CadSyncError("Unexpected WKB geometry payload.")
    return validate_geometry(geom)


def _write_geo(geom: dict[str, Any], srid: int) -> bytes:
    geom = validate_geometry(geom)
    kinds = {"Point": 1, "LineString": 2, "Polygon": 3}
    kind = kinds[geom["type"]]
    data = bytearray(b"GP\x00\x01" + struct.pack("<i", srid) + struct.pack("<BI", 1, kind))

    def add_points(points: list[list[float]]) -> None:
        data.extend(struct.pack("<I", len(points)))
        for x, y in points:
            data.extend(struct.pack("<dd", float(x), float(y)))

    if kind == 1:
        data.extend(struct.pack("<dd", *geom["coordinates"]))
    elif kind == 2:
        add_points(geom["coordinates"])
    else:
        data.extend(struct.pack("<I", len(geom["coordinates"])))
        for ring in geom["coordinates"]:
            add_points(ring)
    return bytes(data)


def _vertices(geom: dict[str, Any]) -> list[list[float]]:
    if geom["type"] == "Point":
        return [geom["coordinates"]]
    if geom["type"] == "Polygon":
        return [p for ring in geom["coordinates"] for p in ring]
    return geom["coordinates"]


def _similar(a: dict[str, Any], b: dict[str, Any], *, tol: float = 1e-7) -> bool:
    if a["type"] != b["type"]:
        return False
    aa, bb = _vertices(a), _vertices(b)
    return len(aa) == len(bb) and all(
        abs(x1 - x2) <= tol and abs(y1 - y2) <= tol
        for (x1, y1), (x2, y2) in zip(aa, bb)
    )


def _feature_geom(bundle: Path, fid: str) -> tuple[dict[str, Any], str, str]:
    with closing(sqlite3.connect((bundle / "attributes.gpkg").resolve().as_uri() + "?mode=ro", uri=True)) as db:
        db.execute("PRAGMA query_only = ON")
        row = db.execute("SELECT table_name FROM gpkg_contents WHERE data_type='features'").fetchone()
        if not row:
            raise CadSyncError("Missing single feature table.")
        table = str(row[0])
        col_row = db.execute("SELECT column_name FROM gpkg_geometry_columns WHERE table_name=?", (table,)).fetchone()
        if not col_row:
            raise CadSyncError("Missing GeoPackage geometry column.")
        geom_col = str(col_row[0])
        table_q = '"' + table.replace('"', '""') + '"'
        col_q = '"' + geom_col.replace('"', '""') + '"'
        rows = db.execute(f'SELECT {col_q} FROM {table_q} WHERE ovg_feature_id=?', (fid,)).fetchall()
        if len(rows) != 1 or rows[0][0] is None:
            raise CadSyncError("Feature ID not found exactly once in received layer.")
        return _read_geo(bytes(rows[0][0])), table, geom_col


def _cad_session() -> tuple[Any, str, str]:
    try:
        app = _running_app()  # attaches to already running COM only, no Dispatch
        drawing = str(app.ActiveDesignFile.FullName)
        model = str(app.ActiveModelReference.Name)
        if not drawing or not model:
            raise CadSyncError("Missing active V8i DGN/model.")
        return app, drawing.lower(), model
    except CadSyncError:
        raise
    except Exception as exc:
        raise CadSyncError("Cannot read running MicroStation V8i session.") from exc


def selected_element(app: Any) -> tuple[str, dict[str, Any]]:
    try:
        e = app.ActiveModelReference.GetSelectedElements()
        if not e.MoveNext():
            raise CadSyncError("Select exactly one existing V8i element.")
        obj = e.Current
        eid, geom = _element_id(obj), _geometry(obj, int(obj.Type))
        if e.MoveNext():
            raise CadSyncError("Only one CAD element may be mapped at a time.")
        return eid, validate_geometry(geom)
    except CadSyncError:
        raise
    except Exception as exc:
        raise CadSyncError("Cannot inspect selected V8i element or unsupported CAD type.") from exc


def bind_selected(bundle: str | Path, feature_id: str, *, verified_crs_units: bool, app: Any = None) -> dict[str, Any]:
    """Explicit human mapping, only when initial CAD and OVG geometry already match."""
    folder, meta = receipt(bundle)
    if not verified_crs_units:
        raise CadSyncError("Verify DGN master units, model origin and CRS control points first.")
    if not isinstance(feature_id, str) or len(feature_id) > 128 or not feature_id.strip():
        raise CadSyncError("Exact OVG feature ID required.")
    if app is None:
        app, drawing, model = _cad_session()
    else:
        drawing = str(app.ActiveDesignFile.FullName).lower()
        model = str(app.ActiveModelReference.Name)
    eid, cadgeom = selected_element(app)
    src_geom, *_ = _feature_geom(folder, feature_id)
    if not _similar(src_geom, cadgeom):
        raise CadSyncError("CAD geometry differs from received OVG geometry; cannot establish trusted baseline.")
    path = folder / MAP_FILE
    if path.exists():
        stored = json.loads(path.read_text(encoding="utf-8"))
        if stored["drawing"] != drawing or stored["model"] != model or stored["content_revision"] != meta["content_revision"]:
            raise CadSyncError("Mapping belongs to a different DGN/model or OVG revision.")
    else:
        stored = {
            "schema_version": "ovg.v8i.mapping.v1", "drawing": drawing, "model": model,
            "content_revision": meta["content_revision"], "bindings": {}, "verified_crs_units": True,
        }
    bindings = stored["bindings"]
    if len(bindings) >= MAX_MAPPED and feature_id not in bindings:
        raise CadSyncError("Maximum 100 manually mapped features per batch.")
    if feature_id in bindings and bindings[feature_id]["cad_element_id"] != eid:
        raise CadSyncError("Feature already mapped to another CAD element.")
    if any(key != feature_id and entry["cad_element_id"] == eid for key, entry in bindings.items()):
        raise CadSyncError("CAD element already bound to another OVG feature.")
    bindings[feature_id] = {"cad_element_id": eid, "base_geometry": src_geom, "base_geometry_digest": digest(src_geom)}
    _write_json(path, stored)
    return {"feature_id": feature_id, "cad_element_id": eid, "mapped_count": len(bindings)}


def _scan_mapped(app: Any, wanted: set[str]) -> dict[str, dict[str, Any]]:
    found: dict[str, dict[str, Any]] = {}
    try:
        enumeration = app.ActiveModelReference.Scan()
        scanned = 0
        while enumeration.MoveNext():
            scanned += 1
            if scanned > MAX_SCAN:
                raise CadSyncError("V8i model exceeds bounded 5,000-element scan.")
            raw = enumeration.Current
            eid = _element_id(raw)
            if eid in wanted:
                if eid in found:
                    raise CadSyncError("Duplicate CAD element ID.")
                found[eid] = validate_geometry(_geometry(raw, int(raw.Type)))
    except CadSyncError:
        raise
    except Exception as exc:
        raise CadSyncError("V8i scan failed or mapped geometry unsupported.") from exc
    if set(found) != wanted:
        raise CadSyncError("A mapped CAD element was deleted/moved or is missing; no automatic deletes.")
    return found


def draft_outbound(bundle: str | Path, *, app: Any = None) -> dict[str, Any]:
    """Prepare independent GeoPackage with mapped CAD geometry; never alter received baseline."""
    folder, meta = receipt(bundle)
    path = folder / MAP_FILE
    if not path.is_file():
        raise CadSyncError("Map selected V8i objects before preparing a sync.")
    stored = json.loads(path.read_text(encoding="utf-8"))
    if stored["schema_version"] != "ovg.v8i.mapping.v1" or not stored["verified_crs_units"]:
        raise CadSyncError("Unverified CAD identity/CRS mapping.")
    if stored["content_revision"] != meta["content_revision"]:
        raise CadSyncError("Mapping is stale against received OVG revision.")
    if app is None:
        app, drawing, model = _cad_session()
    else:
        drawing = str(app.ActiveDesignFile.FullName).lower()
        model = str(app.ActiveModelReference.Name)
    if drawing != stored["drawing"] or model != stored["model"]:
        raise CadSyncError("Current V8i DGN/model differs from mapping.")
    bindings = stored["bindings"]
    if not isinstance(bindings, dict) or not 1 <= len(bindings) <= MAX_MAPPED:
        raise CadSyncError("No valid CAD bindings.")
    if len({b["cad_element_id"] for b in bindings.values()}) != len(bindings):
        raise CadSyncError("CAD IDs must be unique.")
    found = _scan_mapped(app, {b["cad_element_id"] for b in bindings.values()})
    edits: dict[str, dict[str, Any]] = {}
    for fid, record in bindings.items():
        base, _, _ = _feature_geom(folder, fid)
        if digest(base) != record["base_geometry_digest"]:
            raise CadSyncError("Baseline mapping tampered or refreshed.")
        current = found[record["cad_element_id"]]
        if current["type"] != base["type"]:
            raise CadSyncError("Mapped CAD geometry type changed; manual review required.")
        if not _similar(base, current):
            edits[fid] = current
    if not edits:
        raise CadSyncError("No geometry changes in mapped CAD elements.")

    output = folder / OUT_FILE
    plan_path = folder / PLAN_FILE
    if output.exists() or plan_path.exists() or (folder / JOURNAL_FILE).exists():
        raise CadSyncError("Existing outbound draft/journal; do not overwrite. Resolve or archive it first.")
    fd, staging = tempfile.mkstemp(suffix=".gpkg", prefix=".ovg-out-", dir=folder)
    os.close(fd)
    try:
        shutil.copyfile(folder / "attributes.gpkg", staging)
        with closing(sqlite3.connect(staging)) as db:
            db.execute("PRAGMA busy_timeout=1000")
            with db:
                for fid, geom in edits.items():
                    _old, table, column = _feature_geom(folder, fid)
                    tq = '"' + table.replace('"', '""') + '"'
                    cq = '"' + column.replace('"', '""') + '"'
                    result = db.execute(f"UPDATE {tq} SET {cq}=? WHERE ovg_feature_id=?", (
                        _write_geo(geom, int(meta["crs"].split(":")[1])), fid,
                    ))
                    if result.rowcount != 1:
                        raise CadSyncError("Feature identity changed in staged output.")
                # GeoPackage content extent must match the new coordinates.
                rows = db.execute("SELECT table_name FROM gpkg_contents WHERE data_type='features'").fetchall()
                if len(rows) != 1:
                    raise CadSyncError("Multiple GPKG tables are not supported.")
                table = rows[0][0]
                col = db.execute("SELECT column_name FROM gpkg_geometry_columns WHERE table_name=?", (table,)).fetchone()[0]
                tq = '"' + table.replace('"', '""') + '"'
                cq = '"' + col.replace('"', '""') + '"'
                pts = [p for (blob,) in db.execute(f"SELECT {cq} FROM {tq}")
                       for p in _vertices(_read_geo(bytes(blob)))]
                if not pts:
                    raise CadSyncError("Empty output geometry dataset.")
                xs, ys = [p[0] for p in pts], [p[1] for p in pts]
                db.execute("UPDATE gpkg_contents SET min_x=?,min_y=?,max_x=?,max_y=? WHERE table_name=?", (
                    min(xs), min(ys), max(xs), max(ys), table,
                ))
            if db.execute("PRAGMA quick_check").fetchone() != ("ok",):
                raise CadSyncError("Staged GeoPackage failed integrity validation.")
        if Path(staging).stat().st_size > 16 * 1024 * 1024:
            raise CadSyncError("R026 upload max 16 MiB exceeded.")
        _verify_geopackage(Path(staging), meta["crs"], meta["feature_count"])
        sha = hashlib.sha256(Path(staging).read_bytes()).hexdigest()
        plan = {
            "schema_version": "ovg.v8i.outbound.v1", "layer_id": meta["layer_id"],
            "content_revision": meta["content_revision"], "schema_revision": meta["schema_revision"],
            "gpkg_sha256": sha, "edits": [
                {"feature_id": fid, "cad_element_id": bindings[fid]["cad_element_id"],
                 "geometry_digest": digest(geom)} for fid, geom in sorted(edits.items())
            ], "drawing": drawing, "model": model, "server_preview": None,
        }
        Path(staging).rename(output)
        _write_json(plan_path, plan)
        return {"changes": len(edits), "layer_id": meta["layer_id"], "file": str(output), "plan": str(plan_path), "sha256": sha}
    finally:
        if Path(staging).exists():
            Path(staging).unlink()