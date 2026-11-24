"""Bounded, credential-isolated SQLite cache of confirmed server snapshots.

Never contains bearer tokens or unsent edits. A fresh authorized bootstrap must
validate revision, scope, schema and permission epoch before a cache is reused.
"""
from __future__ import annotations

import hashlib
import json
import os
import sqlite3
import time
from contextlib import closing
from pathlib import Path

MAX_BYTES = 32 * 1024 * 1024
MAX_TOTAL = 128 * 1024 * 1024
MAX_AGE = 24 * 3600


def cache_path(root: Path, config, layer_id: str) -> Path:
    scope = json.dumps([
        config.base_url, config.organization_id, config.project_id,
        hashlib.sha256(config.bearer_token.encode()).hexdigest(), layer_id,
    ])
    return root / (hashlib.sha256(scope.encode()).hexdigest() + ".sqlite3")


def signature(page) -> str:
    return json.dumps([
        page.layer_id, page.source_id, page.content_revision, page.schema_revision,
        page.permission_epoch, page.crs_code, page.fields,
    ], sort_keys=True)


def read_cached_record(path: Path, page, *, allow_older=False):
    if not getattr(page, "cacheable", False):
        return None
    try:
        if not path.is_file() or path.stat().st_size > MAX_BYTES + 1024 * 1024:
            return None
        with closing(sqlite3.connect(path.as_uri() + "?mode=ro", uri=True, timeout=1)) as db:
            row = db.execute("SELECT signature, saved, features, cursor FROM snapshot").fetchone()
        if row is None or time.time() - row[1] >= MAX_AGE:
            return None
        previous = json.loads(row[0])
        current = json.loads(signature(page))
        if len(previous) != 7 or previous[:2] != current[:2] or previous[3:] != current[3:]:
            return None
        if not allow_older and previous[2] != current[2]:
            return None
        features = json.loads(row[2])
        if not isinstance(features, list):
            return None
        if previous[2] == current[2] and len(features) != getattr(page, "feature_count", None):
            return None
        return {"features": features, "revision": previous[2], "cursor": int(row[3])}
    except (OSError, sqlite3.Error, ValueError, TypeError):
        return None


def read_snapshot(path: Path, page):
    record = read_cached_record(path, page)
    return record["features"] if record is not None else None


def write_snapshot(path: Path, page, features) -> None:
    if not getattr(page, "cacheable", False) or len(features) != getattr(page, "feature_count", None):
        return
    tmp = path.with_suffix(".tmp")
    try:
        payload = json.dumps(features, ensure_ascii=False, allow_nan=False)
        if len(payload.encode()) > MAX_BYTES:
            return
        path.parent.mkdir(parents=True, mode=0o700, exist_ok=True)
        fd = os.open(tmp, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
        os.close(fd)
        try:
            with closing(sqlite3.connect(tmp, timeout=1)) as db:
                db.execute("CREATE TABLE snapshot(signature TEXT, saved REAL, features TEXT, cursor INTEGER)")
                db.execute("INSERT INTO snapshot VALUES (?, ?, ?, ?)",
                           (signature(page), time.time(), payload, int(getattr(page, "cursor", 0))))
                db.commit()
            os.replace(tmp, path)
        finally:
            tmp.unlink(missing_ok=True)
        total = 0
        for entry in sorted(path.parent.glob("*.sqlite3"), key=lambda p: p.stat().st_mtime, reverse=True):
            size = entry.stat().st_size
            if time.time() - entry.stat().st_mtime >= MAX_AGE or total + size > MAX_TOTAL:
                entry.unlink(missing_ok=True)
            else:
                total += size
    except (OSError, sqlite3.Error, ValueError, TypeError):
        # Cache is disposable; never block the authorized network data path.
        return


def clear_snapshots(root: Path) -> None:
    for path in root.glob("*.sqlite3"):
        try:
            path.unlink(missing_ok=True)
        except OSError:
            pass
