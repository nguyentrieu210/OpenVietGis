"""R009 Cartography styles, layouts, auto legend, and immutable print export domain."""

from __future__ import annotations

import hashlib
import json
import re
from dataclasses import dataclass
from datetime import datetime
from enum import StrEnum
from typing import Any
from uuid import UUID

from openvietgis_api.domains.platform_errors import PlatformValidationError

_HEX_COLOR = re.compile(r"^#[0-9a-fA-F]{6}$")


class StyleMode(StrEnum):
    SINGLE = "single"
    CATEGORICAL = "categorical"
    RANGE = "range"


class PaperSize(StrEnum):
    A4 = "A4"
    A3 = "A3"
    A2 = "A2"
    A1 = "A1"
    A0 = "A0"


class Orientation(StrEnum):
    PORTRAIT = "portrait"
    LANDSCAPE = "landscape"


class PrintFormat(StrEnum):
    PNG = "png"
    PDF = "pdf"


@dataclass(frozen=True, slots=True)
class LayerStyleRecord:
    id: UUID
    organization_id: UUID
    project_id: UUID
    layer_id: UUID
    mode: StyleMode
    style_spec: dict[str, Any]
    label_field: str | None
    legend_items: list[dict[str, Any]]
    revision: int
    updated_by_actor_id: UUID
    created_at: datetime
    updated_at: datetime


@dataclass(frozen=True, slots=True)
class PrintLayoutRecord:
    id: UUID
    organization_id: UUID
    project_id: UUID
    name: str
    title: str
    paper_size: PaperSize
    orientation: Orientation
    crs_code: int
    attribution: str
    show_north_arrow: bool
    show_scale_bar: bool
    show_legend: bool
    layer_ids: list[str]
    extent: dict[str, float] | None
    created_by_actor_id: UUID
    created_at: datetime
    updated_at: datetime


@dataclass(frozen=True, slots=True)
class PrintExportRecord:
    id: UUID
    organization_id: UUID
    project_id: UUID
    layout_id: UUID
    output_format: PrintFormat
    dpi: int
    scale_denominator: int
    scale_accuracy_note: str
    revision_manifest: dict[str, Any]
    storage_key: str
    sha256: str
    byte_size: int
    created_by_actor_id: UUID
    created_at: datetime


def print_revision_fingerprint(manifest: dict[str, Any]) -> str:
    """Canonical content fingerprint, independent of object key order."""
    return hashlib.sha256(
        json.dumps(
            manifest, sort_keys=True, separators=(",", ":"), ensure_ascii=False, allow_nan=False
        ).encode("utf-8")
    ).hexdigest()


def _check_color(val: Any, default: str = "#1f78b4") -> str:
    if val is None:
        return default
    s = str(val)
    if not _HEX_COLOR.match(s):
        raise PlatformValidationError(f"invalid hex color {s!r}", code="INVALID_STYLE")
    return s


def validate_and_build_legend(
    mode: StyleMode,
    style_spec: dict[str, Any],
    layer_name: str,
) -> tuple[dict[str, Any], list[dict[str, Any]]]:
    """Validate renderer-neutral style spec and auto-generate legend items."""
    if mode == StyleMode.SINGLE:
        fill = _check_color(style_spec.get("fill_color"), "#2563eb")
        stroke = _check_color(style_spec.get("stroke_color"), "#1e3a8a")
        width = float(style_spec.get("stroke_width", 1.5))
        opacity = float(style_spec.get("opacity", 0.75))
        if not (0.0 <= opacity <= 1.0) or not (0.5 <= width <= 12.0):
            raise PlatformValidationError("invalid opacity or stroke_width", code="INVALID_STYLE")
        clean = {
            "fill_color": fill,
            "stroke_color": stroke,
            "stroke_width": width,
            "opacity": opacity,
        }
        legend = [{"label": layer_name, "fill_color": fill, "stroke_color": stroke}]
        return clean, legend

    if mode == StyleMode.CATEGORICAL:
        field = str(style_spec.get("field") or "").strip()
        categories = style_spec.get("categories")
        if not field or not isinstance(categories, list) or not categories:
            raise PlatformValidationError(
                "categorical style requires field and non-empty categories",
                code="INVALID_STYLE",
            )
        clean_cats: list[dict[str, Any]] = []
        legend = []
        for idx, item in enumerate(categories):
            if not isinstance(item, dict) or "value" not in item:
                raise PlatformValidationError(
                    f"category #{idx} requires value", code="INVALID_STYLE"
                )
            color = _check_color(item.get("color"))
            label = str(item.get("label") or item["value"])
            clean_cats.append({"value": item["value"], "color": color, "label": label})
            legend.append({"label": label, "fill_color": color, "stroke_color": "#111827"})
        fallback = _check_color(style_spec.get("default_color"), "#9ca3af")
        return {
            "field": field,
            "categories": clean_cats,
            "default_color": fallback,
        }, legend

    if mode == StyleMode.RANGE:
        field = str(style_spec.get("field") or "").strip()
        stops = style_spec.get("breaks")
        if not field or not isinstance(stops, list) or not stops:
            raise PlatformValidationError(
                "range style requires field and non-empty breaks", code="INVALID_STYLE"
            )
        clean_breaks: list[dict[str, Any]] = []
        legend = []
        for idx, b in enumerate(stops):
            if not isinstance(b, dict) or "min" not in b or "max" not in b:
                raise PlatformValidationError(
                    f"range break #{idx} requires min and max", code="INVALID_STYLE"
                )
            lo, hi = float(b["min"]), float(b["max"])
            if lo > hi:
                raise PlatformValidationError("range break min > max", code="INVALID_STYLE")
            color = _check_color(b.get("color"))
            label = str(b.get("label") or f"{lo:g} – {hi:g}")
            clean_breaks.append({"min": lo, "max": hi, "color": color, "label": label})
            legend.append({"label": label, "fill_color": color, "stroke_color": "#111827"})
        return {"field": field, "breaks": clean_breaks}, legend

    raise PlatformValidationError(f"unsupported style mode {mode!r}", code="INVALID_STYLE")


def paper_dimensions_px(
    paper_size: PaperSize, orientation: Orientation, dpi: int
) -> tuple[int, int]:
    """Return physical print canvas size in pixels (bounded to keep exports responsive)."""
    # Physical mm: A4 = 210x297, A3 = 297x420; compute pixel dimensions for 150 or 300 DPI
    mm_w, mm_h = {PaperSize.A4:(210.,297.),PaperSize.A3:(297.,420.),PaperSize.A2:(420.,594.),PaperSize.A1:(594.,841.),PaperSize.A0:(841.,1189.)}[paper_size]
    if paper_size in (PaperSize.A0,PaperSize.A1,PaperSize.A2) and dpi != 150:
        raise PlatformValidationError("Large paper formats require 150 dpi", code="INVALID_DPI")
    if orientation == Orientation.LANDSCAPE:
        mm_w, mm_h = mm_h, mm_w
    # The finite size/DPI profile is bounded to at most 4961 × 3508 pixels.
    # Clamping an A3 300-DPI dimension changes physical page size.
    if dpi not in (150, 300):
        raise PlatformValidationError("dpi must be 150 or 300", code="INVALID_DPI")
    px_w = int(round((mm_w / 25.4) * dpi))
    px_h = int(round((mm_h / 25.4) * dpi))
    return px_w, px_h
