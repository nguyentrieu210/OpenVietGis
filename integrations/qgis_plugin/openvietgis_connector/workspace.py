"""R027 manual QGIS <-> OpenVietGIS GeoPackage workspace.

Uses the already-reviewed R026 HTTPS export, preview and R006 changeset push.
Local edits are never overwritten if the three-way baseline detects a conflict.
Not automatic sync, and not a replacement for R027 durable server-side binding.
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
from dataclasses import dataclass
from pathlib import Path
from typing import Any
from uuid import UUID, uuid4

from .client import (
    ApiError,
    IntegrityError,
    OpenVietGisConfig,
    download_layer_geopackage,
    preview_layer_geopackage_sync,
    push_layer_geopackage_sync,
)

MAX_WORKSPACE_BYTES = 16 * 1024 * 1024
MAX_FEATURES = 25_000
MAX_ACTIONS = 1_000
STATE_FILE = "workspace.json"


class WorkspaceConflict(Exception):
    """Sync cannot proceed without explicit human resolution."""


@dataclass(frozen=True)
class Row:
    feature_id: str
    attrs: dict[str, Any]
    geom: bytes
    canonical_geometry: tuple[Any, ...]


@dataclass(frozen=True)
class Dataset:
    table: str
    geom_col: str
    fields: tuple[str, ...]
    crs: int
    rows: dict[str, Row]


@dataclass(frozen=True)
class LocalAction:
    feature_id: str
    action: str  # "create" | "update" | "delete"
    changed_fields: tuple[str, ...]
    geometry_changed: bool


@dataclass(frozen=True)
class SyncPreview:
    workspace: Path
    remote_file: Path
    local_digest: str
    baseline_digest: str
    remote_digest: str
    remote_content_revision: str
    remote_schema_revision: str
    to_local: tuple[LocalAction, ...]
    to_server: tuple[LocalAction, ...]
    conflicts: tuple[str, ...]
    digest: str


def _sha(path: Path) -> str:
    if not path.is_file() or path.stat().st_size > MAX_WORKSPACE_BYTES:
        raise WorkspaceConflict("Missing or oversized GeoPackage file.")
    # Copying an active WAL database would omit uncheckpointed QGIS edits.
    journal = path.with_name(path.name + "-wal")
    if journal.exists() and journal.stat().st_size > 0:
        raise WorkspaceConflict("Uncheckpointed QGIS WAL. Close editing layers first.")
    return hashlib.sha256(path.read_bytes()).hexdigest()


def _quote(name: str) -> str:
    return '"' + name.replace('"', '""') + '"'


def _decode_geometry(blob: bytes, expected_crs: int) -> tuple[Any, ...]:
    if not isinstance(blob, bytes) or len(blob) < 13 or blob[:2] != b"GP" or blob[2] != 0:
        raise WorkspaceConflict("Invalid GeoPackage geometry.")
    flags = blob[3]
    envelope = (flags >> 1) & 7
    if flags & 0xF0 or envelope not in {0, 1, 2, 3, 4}:
        raise WorkspaceConflict("Unsupported/empty GeoPackage geometry.")
    header_endian = "<" if flags & 1 else ">"
    srid = struct.unpack_from(header_endian + "i", blob, 4)[0]
    if srid != expected_crs:
        raise WorkspaceConflict("Feature geometry CRS mismatch.")
    offset = 8 + {0: 0, 1: 32, 2: 48, 3: 48, 4: 64}[envelope]
    if len(blob) < offset + 5:
        raise WorkspaceConflict("Truncated GeoPackage geometry.")
    endian = "<" if blob[offset] == 1 else ">" if blob[offset] == 0 else None
    if endian is None:
        raise WorkspaceConflict("Invalid WKB endianness.")
    kind = struct.unpack_from(endian + "I", blob, offset + 1)[0]
    if kind not in {1, 2, 3}:
        raise WorkspaceConflict("Only 2D Point/LineString/Polygon are supported.")
    pos = offset + 5

    def point() -> tuple[float, float]:
        nonlocal pos
        if pos + 16 > len(blob):
            raise WorkspaceConflict("Truncated WKB point.")
        x, y = struct.unpack_from(endian + "dd", blob, pos)
        pos += 16
        if not (math.isfinite(x) and math.isfinite(y)):
            raise WorkspaceConflict("Non-finite coordinate.")
        return (float(x), float(y))

    def count() -> int:
        nonlocal pos
        if pos + 4 > len(blob):
            raise WorkspaceConflict("Truncated WKB count.")
        n = struct.unpack_from(endian + "I", blob, pos)[0]
        pos += 4
        if n > 250_000 or n > (len(blob) - pos) // 16 + 1:
            raise WorkspaceConflict("Invalid WKB geometry size.")
        return n

    if kind == 1:
        geom = (kind, point())
    elif kind == 2:
        n = count()
        if n < 2:
            raise WorkspaceConflict("Degenerate LineString.")
        geom = (kind, tuple(point() for _ in range(n)))
    else:
        rings = struct.unpack_from(endian + "I", blob, pos)[0] if pos + 4 <= len(blob) else 0
        pos += 4
        if not 1 <= rings <= 1024:
            raise WorkspaceConflict("Invalid polygon ring count.")
        result = []
        for _ in range(rings):
            n = count()
            if n < 4:
                raise WorkspaceConflict("Degenerate polygon ring.")
            result.append(tuple(point() for _ in range(n)))
        geom = (kind, tuple(result))
    if pos != len(blob):
        raise WorkspaceConflict("Unexpected trailing WKB payload.")
    return geom


def read_dataset(path: Path) -> Dataset:
    try:
        return _read_dataset_checked(path)
    except sqlite3.DatabaseError as exc:
        raise WorkspaceConflict("Corrupt or unsupported GeoPackage SQLite database.") from exc


def _read_dataset_checked(path: Path) -> Dataset:
    _sha(path)
    uri = "file:" + str(path.resolve()) + "?mode=ro"
    with closing(sqlite3.connect(uri, uri=True)) as conn:
        conn.row_factory = sqlite3.Row
        meta = conn.execute("""
            SELECT c.table_name, c.srs_id, g.column_name AS geom_col,
                   g.srs_id AS geometry_srs, g.geometry_type_name, g.z, g.m
            FROM gpkg_contents c
            JOIN gpkg_geometry_columns g ON g.table_name=c.table_name
            WHERE c.data_type='features'
        """).fetchall()
        if len(meta) != 1 or meta[0]["z"] != 0 or meta[0]["m"] != 0:
            raise WorkspaceConflict("Exactly one 2D feature layer is required.")
        table, geom, crs = str(meta[0]["table_name"]), str(meta[0]["geom_col"]), int(meta[0]["srs_id"])
        family = str(meta[0]["geometry_type_name"]).upper()
        expected_kind = {"POINT": 1, "LINESTRING": 2, "POLYGON": 3}.get(family)
        if expected_kind is None or int(meta[0]["geometry_srs"]) != crs:
            raise WorkspaceConflict("Geometry type or layer CRS metadata is inconsistent.")
        srs = conn.execute(
            "SELECT organization, organization_coordsys_id FROM gpkg_spatial_ref_sys "
            "WHERE srs_id=?", (crs,)
        ).fetchone()
        if (srs is None or str(srs["organization"]).upper() != "EPSG"
                or srs["organization_coordsys_id"] != crs):
            raise WorkspaceConflict("GeoPackage CRS registry is not verified EPSG.")
        columns = conn.execute(f"PRAGMA table_info({_quote(table)})").fetchall()
        if "ovg_feature_id" not in {str(c["name"]) for c in columns}:
            raise WorkspaceConflict("Stable ovg_feature_id column required.")
        fields = tuple(sorted(
            str(c["name"]) for c in columns
            if str(c["name"]) not in {"fid", "ovg_feature_id", geom}
        ))
        if len(fields) > 100:
            raise WorkspaceConflict("More than 100 attributes are not supported.")
        values = conn.execute(
            f"SELECT {_quote('ovg_feature_id')}, {_quote(geom)}, "
            + ", ".join(_quote(f) for f in fields)
            + f" FROM {_quote(table)}"
        ).fetchall() if fields else conn.execute(
            f"SELECT {_quote('ovg_feature_id')}, {_quote(geom)} FROM {_quote(table)}"
        ).fetchall()
        if len(values) > MAX_FEATURES:
            raise WorkspaceConflict("Workspace exceeds 25,000 features.")
        result = {}
        for row in values:
            key = row["ovg_feature_id"]
            if not isinstance(key, str) or not key or len(key) > 128 or key.strip() != key:
                raise WorkspaceConflict("Blank/invalid ovg_feature_id. Assign a stable ID before sync.")
            if key in result:
                raise WorkspaceConflict("Duplicate ovg_feature_id.")
            blob = row[geom]
            if blob is None:
                raise WorkspaceConflict("Null geometry cannot be synced.")
            geom_bytes = bytes(blob)
            canonical = _decode_geometry(geom_bytes, crs)
            if canonical[0] != expected_kind:
                raise WorkspaceConflict("Feature geometry conflicts with layer geometry type.")
            attrs = {name: row[name] for name in fields}
            if any(isinstance(v, float) and not math.isfinite(v) for v in attrs.values()):
                raise WorkspaceConflict("Non-finite attribute.")
            result[key] = Row(key, attrs, geom_bytes, canonical)
        return Dataset(table, geom, fields, crs, result)


def _compatible(base: Dataset, local: Dataset, remote: Dataset) -> None:
    if not (
        base.fields == local.fields == remote.fields
        and base.crs == local.crs == remote.crs
        and base.table == local.table == remote.table
        and base.geom_col == local.geom_col == remote.geom_col
    ):
        raise WorkspaceConflict("Layer schema or CRS changed; manual remapping required.")


def _plan(
    base: Dataset,
    local: Dataset,
    remote: Dataset,
    *,
    allow_deletes: bool = False,
):
    to_local: list[LocalAction] = []
    to_remote: list[LocalAction] = []
    conflicts: list[str] = []
    for key in sorted(set(base.rows) | set(local.rows) | set(remote.rows)):
        b, l, r = base.rows.get(key), local.rows.get(key), remote.rows.get(key)
        if b is None:
            if l and r:
                conflicts.append(f"{key}: created independently on both sides")
            elif l:
                to_remote.append(LocalAction(key, "create", base.fields, True))
            elif r:
                to_local.append(LocalAction(key, "create", base.fields, True))
            continue
        if l is None or r is None:
            if not allow_deletes:
                conflicts.append(f"{key}: deleted/missing on one side; deletion blocked")
                continue
            if l is None and r is None:
                continue
            if l is None and r is not None:
                remote_changed = (
                    any(r.attrs[f] != b.attrs[f] for f in base.fields)
                    or r.canonical_geometry != b.canonical_geometry
                )
                if remote_changed:
                    conflicts.append(f"{key}: deleted on QGIS while edited on Web")
                else:
                    to_remote.append(LocalAction(key, "delete", (), False))
                continue
            if r is None and l is not None:
                local_changed = (
                    any(l.attrs[f] != b.attrs[f] for f in base.fields)
                    or l.canonical_geometry != b.canonical_geometry
                )
                if local_changed:
                    conflicts.append(f"{key}: deleted on Web while edited on QGIS")
                else:
                    to_local.append(LocalAction(key, "delete", (), False))
                continue
        changed_local = {f for f in base.fields if l.attrs[f] != b.attrs[f]}
        changed_remote = {f for f in base.fields if r.attrs[f] != b.attrs[f]}
        clashes = sorted(f for f in changed_local & changed_remote if l.attrs[f] != r.attrs[f])
        lg = l.canonical_geometry != b.canonical_geometry
        rg = r.canonical_geometry != b.canonical_geometry
        if clashes or (lg and rg and l.canonical_geometry != r.canonical_geometry):
            conflicts.append(f"{key}: concurrent edits to {','.join(clashes) or 'geometry'}")
            continue
        fields_to_local = tuple(sorted(f for f in changed_remote if l.attrs[f] != r.attrs[f]))
        fields_to_remote = tuple(sorted(f for f in changed_local if l.attrs[f] != r.attrs[f]))
        geom_to_local = rg and l.canonical_geometry != r.canonical_geometry
        geom_to_remote = lg and l.canonical_geometry != r.canonical_geometry
        if fields_to_local or geom_to_local:
            to_local.append(LocalAction(key, "update", fields_to_local, geom_to_local))
        if fields_to_remote or geom_to_remote:
            to_remote.append(LocalAction(key, "update", fields_to_remote, geom_to_remote))
    if len(to_local) + len(to_remote) > MAX_ACTIONS:
        raise WorkspaceConflict("Too many updates in one sync batch.")
    return tuple(to_local), tuple(to_remote), tuple(conflicts)


def _manifest(root: Path, config: OpenVietGisConfig) -> dict[str, Any]:
    try:
        data = json.loads((root / STATE_FILE).read_text(encoding="utf-8"))
    except (OSError, ValueError) as exc:
        raise WorkspaceConflict("Missing/corrupted workspace manifest.") from exc
    if data.get("version") != 1 or any(
        data.get(name) != getattr(config, name)
        for name in ("base_url", "organization_id", "project_id")
    ):
        raise WorkspaceConflict("Workspace belongs to another server/organization/project.")
    if data.get("pending"):
        raise WorkspaceConflict("Previous sync outcome uncertain; recover pending work first.")
    if _sha(root / "baseline.gpkg") != data.get("baseline_sha256"):
        raise WorkspaceConflict("Baseline snapshot tampered; sync disabled.")
    return data


def _write_json(path: Path, data: dict[str, Any]) -> None:
    tmp = path.with_suffix(".tmp")
    with tmp.open("w", encoding="utf-8") as stream:
        json.dump(data, stream, indent=2, sort_keys=True)
        stream.flush()
        os.fsync(stream.fileno())
    os.replace(tmp, path)


def initialize_workspace(
    config: OpenVietGisConfig,
    layer_id: str,
    destination_dir: str | Path,
) -> Path:
    uid = str(UUID(layer_id))
    dest = Path(destination_dir).expanduser().resolve()
    dest.mkdir(parents=True, exist_ok=True)
    root = dest / f"ovg_qgis_{uid}"
    if root.exists():
        raise WorkspaceConflict("Workspace already exists. Use it, do not overwrite it.")
    td = tempfile.mkdtemp(dir=dest, prefix=".ovg-ws-")
    try:
        downloaded = download_layer_geopackage(config, uid, td)
        if not downloaded.content_revision or not downloaded.schema_revision:
            raise IntegrityError("Missing source revision; cannot create a workspace.")
        if len(downloaded.content_revision) != 64 or len(downloaded.schema_revision) != 64:
            raise IntegrityError("Invalid source revision.")
        dataset = read_dataset(downloaded.path)
        if downloaded.crs != f"EPSG:{dataset.crs}":
            raise WorkspaceConflict("Explicit CRS header differs from GeoPackage.")
        staging = Path(td)
        shutil.copy2(downloaded.path, staging / "baseline.gpkg")
        shutil.copy2(downloaded.path, staging / "working.gpkg")
        _write_json(staging / STATE_FILE, {
            "version": 1, "base_url": config.base_url,
            "organization_id": config.organization_id, "project_id": config.project_id,
            "layer_id": uid, "baseline_sha256": downloaded.sha256,
            "content_revision": downloaded.content_revision,
            "schema_revision": downloaded.schema_revision,
            "pending": None,
        })
        # The temporary directory is moved as a unit before cleanup.
        os.replace(staging, root)
    finally:
        if Path(td).exists():
            shutil.rmtree(td)
    return root


def preview_workspace(
    config: OpenVietGisConfig,
    root: str | Path,
    *,
    allow_deletes: bool = False,
) -> SyncPreview:
    folder = Path(root).expanduser().resolve()
    data = _manifest(folder, config)
    layer_id = str(UUID(data["layer_id"]))
    baseline_path, working_path = folder / "baseline.gpkg", folder / "working.gpkg"
    base, work = read_dataset(baseline_path), read_dataset(working_path)
    remote_dir = folder / ".remote-preview"
    remote_dir.mkdir(exist_ok=True)
    for old in remote_dir.iterdir():
        if old.is_file():
            old.unlink()
    downloaded = download_layer_geopackage(config, layer_id, remote_dir)
    remote = read_dataset(downloaded.path)
    if downloaded.schema_revision != data["schema_revision"]:
        raise WorkspaceConflict("Remote schema revision changed, manual remapping required.")
    if downloaded.crs != f"EPSG:{base.crs}":
        raise WorkspaceConflict("Remote CRS changed.")
    _compatible(base, work, remote)
    pull, push, conflicts = _plan(base, work, remote, allow_deletes=allow_deletes)
    digest_payload: dict[str, Any] = {
        "baseline": data["baseline_sha256"],
        "working": _sha(working_path),
        "remote": downloaded.sha256,
        "remote_revision": downloaded.content_revision,
        "schema": downloaded.schema_revision,
        "pull": [a.__dict__ for a in pull],
        "push": [a.__dict__ for a in push],
        "conflicts": conflicts,
    }
    if allow_deletes:
        digest_payload["allow_deletes"] = True
    digest = hashlib.sha256(
        json.dumps(digest_payload, sort_keys=True, default=lambda obj: list(obj)).encode()
    ).hexdigest()
    return SyncPreview(
        folder, downloaded.path, _sha(working_path), data["baseline_sha256"],
        downloaded.sha256, downloaded.content_revision, downloaded.schema_revision,
        pull, push, conflicts, digest
    )


def _apply_pull(working: Path, remote: Dataset, tasks: tuple[LocalAction, ...]) -> None:
    if not tasks:
        return
    local = read_dataset(working)
    _compatible(local, local, remote)
    with closing(sqlite3.connect(working)) as conn:
        conn.execute("BEGIN IMMEDIATE")
        for task in tasks:
            if task.action == "delete":
                conn.execute(
                    f'DELETE FROM {_quote(local.table)} WHERE "ovg_feature_id"=?',
                    (task.feature_id,),
                )
                continue
            source = remote.rows[task.feature_id]
            if task.action == "create":
                cols = ("ovg_feature_id", local.geom_col, *local.fields)
                values = (source.feature_id, source.geom, *(source.attrs[f] for f in local.fields))
                conn.execute(
                    f"INSERT INTO {_quote(local.table)} ({','.join(map(_quote, cols))})"
                    f" VALUES ({','.join('?' for _ in cols)})", values
                )
            else:
                patch = {f: source.attrs[f] for f in task.changed_fields}
                if task.geometry_changed:
                    patch[local.geom_col] = source.geom
                if not patch:
                    continue
                conn.execute(
                    f"UPDATE {_quote(local.table)} SET "
                    + ",".join(f"{_quote(k)}=?" for k in patch)
                    + ' WHERE "ovg_feature_id"=?',
                    (*patch.values(), task.feature_id),
                )
        conn.execute(
            "UPDATE gpkg_contents SET last_change=strftime('%Y-%m-%dT%H:%M:%fZ','now') "
            "WHERE table_name=?", (local.table,),
        )
        conn.commit()
    updated = read_dataset(working)
    # Keep GeoPackage bounding metadata consistent for QGIS viewport/zoom.
    all_xy = []
    def collect(coords):
        if isinstance(coords, tuple) and len(coords) == 2 and all(
            isinstance(n, (int, float)) for n in coords
        ):
            all_xy.append(coords)
        elif isinstance(coords, tuple):
            for part in coords:
                collect(part)
    for row in updated.rows.values():
        collect(row.canonical_geometry[1])
    with closing(sqlite3.connect(working)) as conn:
        bounds = (
            (min(x for x, _ in all_xy), min(y for _, y in all_xy),
             max(x for x, _ in all_xy), max(y for _, y in all_xy))
            if all_xy else (None, None, None, None)
        )
        conn.execute(
            "UPDATE gpkg_contents SET min_x=?, min_y=?, max_x=?, max_y=? "
            "WHERE table_name=?", (*bounds, local.table),
        )
        conn.commit()
    read_dataset(working)


def _finish(root: Path, merged: Path, new_content_rev: str, schema_rev: str) -> None:
    # Keep pending recovery until all files and the manifest are durable.
    baseline = root / "baseline.gpkg"
    working = root / "working.gpkg"
    btmp = root / ".baseline-new.gpkg"
    wtmp = root / ".working-new.gpkg"
    shutil.copy2(merged, btmp)
    shutil.copy2(merged, wtmp)
    os.replace(wtmp, working)
    os.replace(btmp, baseline)
    data = json.loads((root / STATE_FILE).read_text("utf-8"))
    data["baseline_sha256"] = _sha(baseline)
    data["content_revision"] = new_content_rev
    data["schema_revision"] = schema_rev
    data["pending"] = None
    _write_json(root / STATE_FILE, data)


def commit_workspace(
    config: OpenVietGisConfig,
    preview: SyncPreview,
    *,
    confirmed_digest: str,
    allow_deletes: bool = False,
) -> dict[str, Any]:
    """Human-confirmed manual sync. Close/QGIS-commit local layer before calling."""
    root = preview.workspace
    data = _manifest(root, config)
    if confirmed_digest != preview.digest:
        raise WorkspaceConflict("The human confirmation does not match the preview.")
    if preview.conflicts:
        raise WorkspaceConflict("Conflicts must be resolved manually: " + "; ".join(preview.conflicts))
    if _sha(root / "working.gpkg") != preview.local_digest:
        raise WorkspaceConflict("QGIS working file changed since preview.")
    if _sha(preview.remote_file) != preview.remote_digest:
        raise WorkspaceConflict("Remote snapshot was replaced since preview.")
    if data["baseline_sha256"] != preview.baseline_digest:
        raise WorkspaceConflict("Common baseline changed since preview.")
    if preview.remote_schema_revision != data["schema_revision"]:
        raise WorkspaceConflict("Remote schema changed.")
    remote = read_dataset(preview.remote_file)
    merged = root / ".pending-merged.gpkg"
    shutil.copy2(root / "working.gpkg", merged)
    _apply_pull(merged, remote, preview.to_local)
    # Compare the merged working copy against the exact remote revision using
    # R026's existing authenticated, non-mutating server diff contract.
    remote_count = len(preview.to_server)
    if remote_count:
        result = preview_layer_geopackage_sync(
            config, layer_id=data["layer_id"], gpkg_path=merged,
            base_content_revision=preview.remote_content_revision,
            base_schema_revision=data["schema_revision"],
            allow_deletes=allow_deletes,
        )
        if result.stale_revision or not result.can_commit:
            raise WorkspaceConflict("Remote changed since download; preview again.")
        if (
            result.create_count + result.update_count + getattr(result, "delete_count", 0)
        ) != remote_count:
            raise WorkspaceConflict("Server diff differs from reviewed edits; abort.")
        if not allow_deletes and result.omitted_server_feature_count:
            raise WorkspaceConflict("Server reports missing features; abort.")
        operation_id = str(uuid4())
        data["pending"] = {
            "operation_id": operation_id, "merged_sha256": _sha(merged),
            "base_remote_revision": preview.remote_content_revision,
            "expected_diff_sha256": result.diff_sha256
        }
        _write_json(root / STATE_FILE, data)
        # Network error after HTTP commit is indeterminate. The journal is
        # deliberately kept until recovery confirms the exact remote dataset.
        try:
            receipt = push_layer_geopackage_sync(
                config, layer_id=data["layer_id"], gpkg_path=merged,
                base_content_revision=preview.remote_content_revision,
                base_schema_revision=data["schema_revision"],
                expected_diff_sha256=result.diff_sha256,
                operation_id=operation_id,
                summary="R027 bidirectional QGIS workspace commit",
                allow_deletes=allow_deletes,
            )
        except ApiError as exc:
            # A synchronous 403 definitively refuses this request before the
            # R006 changeset, unlike a network timeout whose result is unknown.
            # Persist the distinction; never delete or overwrite local edits.
            if exc.status_code == 403:
                data["pending"]["definitive_denial"] = True
                _write_json(root / STATE_FILE, data)
                raise WorkspaceConflict(
                    "Server refused sync (HTTP 403). Check project.edit, GIS ACL "
                    "and the project's pairing key; use Khôi phục giao dịch "
                    "after permissions are corrected. Local edits are intact."
                ) from exc
            raise
        if receipt.operation_count != remote_count:
            raise WorkspaceConflict("Unexpected server receipt count; recover pending operation.")
        content_rev = receipt.resulting_content_revision
        changeset_id = receipt.changeset_id
        # A success receipt alone is not proof of external convergence.
        # Read back the exact server revision before advancing our baseline.
        with tempfile.TemporaryDirectory(dir=root) as td:
            acknowledged = download_layer_geopackage(config, data["layer_id"], td)
            remote_after = read_dataset(acknowledged.path)
            merged_data = read_dataset(merged)
            _compatible(merged_data, merged_data, remote_after)
            if (
                acknowledged.content_revision != content_rev
                or set(remote_after.rows) != set(merged_data.rows)
                or any(
                    remote_after.rows[k].attrs != merged_data.rows[k].attrs
                    or remote_after.rows[k].canonical_geometry != merged_data.rows[k].canonical_geometry
                    for k in merged_data.rows
                )
            ):
                raise WorkspaceConflict(
                    "Server acknowledged changeset but read-back differs; recover pending."
                )
    else:
        # Pull-only path: remote may have changed after downloaded snapshot;
        # require a second verified download and revision/data identity check.
        with tempfile.TemporaryDirectory(dir=root) as td:
            latest = download_layer_geopackage(config, data["layer_id"], td)
            if latest.content_revision != preview.remote_content_revision:
                raise WorkspaceConflict("Remote moved during local pull; preview again.")
            latest_data = read_dataset(latest.path)
            _compatible(remote, remote, latest_data)
            if set(remote.rows) != set(latest_data.rows) or any(
                remote.rows[k].attrs != latest_data.rows[k].attrs
                or remote.rows[k].canonical_geometry != latest_data.rows[k].canonical_geometry
                for k in remote.rows
            ):
                raise WorkspaceConflict("Remote data changed during local pull.")
        data["pending"] = {
            "operation_id": None, "merged_sha256": _sha(merged),
            "base_remote_revision": preview.remote_content_revision
        }
        _write_json(root / STATE_FILE, data)
        content_rev, changeset_id = preview.remote_content_revision, None
    _finish(root, merged, content_rev, data["schema_revision"])
    merged.unlink(missing_ok=True)
    return {
        "changeset_id": changeset_id,
        "new_content_revision": content_rev,
        "pulled": len(preview.to_local),
        "pushed": remote_count,
        "deletions_blocked": not allow_deletes,
    }


def recover_workspace(config: OpenVietGisConfig, root: str | Path) -> bool:
    """Recover an uncertain commit only if remote equals exact staged data."""
    folder = Path(root).expanduser().resolve()
    try:
        data = json.loads((folder / STATE_FILE).read_text("utf-8"))
    except (OSError, ValueError) as exc:
        raise WorkspaceConflict("Missing/corrupt workspace manifest.") from exc
    if data.get("version") != 1 or any(
        data.get(name) != getattr(config, name)
        for name in ("base_url", "organization_id", "project_id")
    ):
        raise WorkspaceConflict("Pending workspace scope mismatch; recovery denied.")
    _ = UUID(data["layer_id"])
    pending = data.get("pending")
    if not pending:
        return False
    merged = folder / ".pending-merged.gpkg"
    if _sha(merged) != pending["merged_sha256"]:
        raise WorkspaceConflict("Pending payload is missing or tampered.")
    with tempfile.TemporaryDirectory(dir=folder) as td:
        latest = download_layer_geopackage(config, data["layer_id"], td)
        merged_dataset, remote_dataset = read_dataset(merged), read_dataset(latest.path)
        _compatible(merged_dataset, merged_dataset, remote_dataset)
        if pending.get("definitive_denial"):
            # Explicitly denied HTTP 403: do not advance the local baseline.
            # Only unlock after confirming the server still matches the exact
            # PRE-PUSH remote download (revision, schema, and every feature).
            preview_dir = folder / ".remote-preview"
            snapshots = list(preview_dir.glob("*.gpkg")) if preview_dir.is_dir() else []
            if len(snapshots) != 1 or latest.content_revision != pending["base_remote_revision"]:
                raise WorkspaceConflict("Denied sync cannot be verified; keep pending and investigate.")
            original = read_dataset(snapshots[0])
            _compatible(original, original, remote_dataset)
            if (
                set(original.rows) != set(remote_dataset.rows)
                or any(
                    original.rows[k].attrs != remote_dataset.rows[k].attrs
                    or original.rows[k].canonical_geometry
                    != remote_dataset.rows[k].canonical_geometry
                    for k in original.rows
                )
            ):
                raise WorkspaceConflict("Remote changed after denied push; manual review required.")
            if _sha(folder / "baseline.gpkg") != data["baseline_sha256"]:
                raise WorkspaceConflict("Local baseline changed; keep pending for investigation.")
            # Leave baseline and edited working.gpkg as-is; a fresh preview
            # will perform the required three-way merge once ACL is fixed.
            data["pending"] = None
            _write_json(folder / STATE_FILE, data)
        else:
            if set(merged_dataset.rows) != set(remote_dataset.rows) or any(
                merged_dataset.rows[k].attrs != remote_dataset.rows[k].attrs or
                merged_dataset.rows[k].canonical_geometry != remote_dataset.rows[k].canonical_geometry
                for k in merged_dataset.rows
            ):
                raise WorkspaceConflict(
                    "Remote data does not exactly match staged payload; manual investigation required."
                )
            _finish(folder, merged, latest.content_revision, latest.schema_revision)
    merged.unlink(missing_ok=True)
    return True