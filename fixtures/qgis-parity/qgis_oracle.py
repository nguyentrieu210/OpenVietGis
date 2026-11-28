"""Independent QGIS 3.44 reference outputs; never connects to OVG or production."""
from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import sys
from pathlib import Path

FIXTURES = (
    "attributes.csv", "clip.geojson", "clip-wgs84.geojson", "dem.tif", "expected.json", "invalid.geojson",
    "line.dxf", "nir.tif", "parcels.geojson", "point.geojson", "point-wgs84.geojson", "red.tif",
)


def verified_fixture(root: Path) -> str:
    manifest = root / "SHA256SUMS.txt"
    entries = {}
    for line in manifest.read_text(encoding="ascii").splitlines():
        digest, name = line.split("  ", 1)
        if name not in FIXTURES or name in entries:
            raise ValueError("Unexpected or duplicate fixture name")
        entries[name] = digest
    if set(entries) != set(FIXTURES):
        raise ValueError("Fixture inventory mismatch")
    for name, digest in entries.items():
        path = root / name
        if path.is_symlink() or not path.is_file():
            raise ValueError(f"Invalid fixture file: {name}")
        if hashlib.sha256(path.read_bytes()).hexdigest() != digest:
            raise ValueError(f"Fixture checksum mismatch: {name}")
    return hashlib.sha256(manifest.read_bytes()).hexdigest()


def run(root: Path, out: Path) -> dict:
    os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
    os.environ["PROJ_NETWORK"] = "OFF"
    from qgis.core import (  # type: ignore[import-not-found]
        Qgis,
        QgsApplication,
        QgsCoordinateReferenceSystem,
        QgsFillSymbol,
        QgsMapRendererParallelJob,
        QgsMapSettings,
        QgsProcessingContext,
        QgsProject,
        QgsRectangle,
        QgsVectorLayer,
    )
    from qgis.PyQt.QtCore import QSize  # type: ignore[import-not-found]
    from qgis.PyQt.QtGui import QColor  # type: ignore[import-not-found]

    if not 34400 <= Qgis.QGIS_VERSION_INT < 34500:
        raise RuntimeError(f"QGIS 3.44 required, found {Qgis.QGIS_VERSION}")
    app = QgsApplication([], False)
    app.initQgis()
    try:
        import processing  # type: ignore[import-not-found]
        from processing.core.Processing import Processing  # type: ignore[import-not-found]

        Processing.initialize()
        project = QgsProject.instance()
        crs = QgsCoordinateReferenceSystem("EPSG:3857")
        project.setCrs(crs)
        context = QgsProcessingContext()
        context.setProject(project)

        def load(name):
            layer = QgsVectorLayer(str(root / (name + ".geojson")), name, "ogr")
            if not layer.isValid() or layer.crs().authid() != crs.authid():
                raise ValueError(f"Invalid source or CRS: {name}")
            return layer

        def process(algorithm, params):
            if QgsApplication.processingRegistry().algorithmById(algorithm) is None:
                raise RuntimeError(f"Missing QGIS algorithm: {algorithm}")
            result = processing.run(
                algorithm, {**params, "OUTPUT": "memory:"}, context=context,
            )["OUTPUT"]
            if not result.isValid() or result.crs().authid() != crs.authid():
                raise RuntimeError(f"Invalid output: {algorithm}")
            return result

        parcels, overlay, point, invalid = [load(n) for n in ("parcels", "clip", "point", "invalid")]
        clipped = process("native:clip", {"INPUT": parcels, "OVERLAY": overlay})
        dissolved = process("native:dissolve", {"INPUT": parcels, "FIELD": ["zone"]})
        buffered = process("native:buffer", {
            "INPUT": point, "DISTANCE": 10.0, "SEGMENTS": 8,
            "END_CAP_STYLE": 0, "JOIN_STYLE": 0, "MITER_LIMIT": 2.0, "DISSOLVE": False,
        })
        # Cross-CRS algorithm evidence runs through native QGIS Processing on
        # the same checksum-pinned fixtures used by the Rust/PostGIS harness.
        wgs_clip = QgsVectorLayer(str(root / "clip-wgs84.geojson"), "clip_wgs84", "ogr")
        wgs_point = QgsVectorLayer(str(root / "point-wgs84.geojson"), "point_wgs84", "ogr")
        if (not wgs_clip.isValid() or wgs_clip.crs().authid() != "EPSG:4326"
                or not wgs_point.isValid() or wgs_point.crs().authid() != "EPSG:4326"):
            raise ValueError("Invalid WGS84 processing fixtures")

        mixed_union = process("native:union", {"INPUT": parcels, "OVERLAY": wgs_clip})
        mixed_join = process("native:joinattributesbylocation", {
            "INPUT": point, "JOIN": wgs_point, "PREDICATE": [0],
            "JOIN_FIELDS": [], "METHOD": 1, "DISCARD_NONMATCHING": True,
            "PREFIX": "join_",
        })
        reverse_mixed_join = process("native:joinattributesbylocation", {
            "INPUT": wgs_point, "JOIN": point, "PREDICATE": [0],
            "JOIN_FIELDS": [], "METHOD": 1, "DISCARD_NONMATCHING": True,
            "PREFIX": "join_",
        })
        projected = processing.run(
            "native:reprojectlayer",
            {"INPUT": wgs_point, "TARGET_CRS": crs, "OUTPUT": "memory:"},
            context=context,
        )["OUTPUT"]
        if not projected.isValid() or projected.crs().authid() != "EPSG:3857":
            raise RuntimeError("Invalid QGIS reproject output")

        mixed_actual = {}

        def feature_rows(layer):
            names = [field.name() for field in layer.fields()]
            rows = []
            for feature in layer.getFeatures():
                geometry = feature.geometry()
                attributes = {name: feature[name] for name in names}
                row = {
                    "source_feature_id": attributes.get("source_feature_id")
                        or attributes.get("join_source_feature_id"),
                    "overlay_feature_id": attributes.get("overlay_feature_id")
                        or attributes.get("join_overlay_feature_id")
                        or attributes.get("join_source_feature_id"),
                    "geometry_type": int(geometry.type()),
                    "attributes": attributes,
                }
                if "code" in attributes:
                    row["source_code"] = attributes["code"]
                overlay_code = attributes.get("join_code")
                if overlay_code is None:
                    overlay_code = next(
                        (value for name, value in attributes.items()
                         if name.startswith("code_") and value is not None),
                        None,
                    )
                if overlay_code is not None:
                    row["overlay_code"] = overlay_code
                if geometry.type() == 2:
                    row["area_m2"] = geometry.area()
                elif geometry.type() == 0 and not geometry.isEmpty():
                    point_geometry = geometry.asPoint()
                    row["x"] = point_geometry.x()
                    row["y"] = point_geometry.y()
                rows.append(row)
            return rows

        mixed_actual["mixed_crs_union"] = {
            "features": mixed_union.featureCount(),
            "area_m2": sum(row["area_m2"] for row in feature_rows(mixed_union)),
            "rows": feature_rows(mixed_union),
        }
        mixed_actual["mixed_crs_spatial_join"] = {
            "features": mixed_join.featureCount(),
            "rows": feature_rows(mixed_join),
        }
        mixed_actual["reverse_mixed_crs_spatial_join"] = {
            "features": reverse_mixed_join.featureCount(),
            "rows": feature_rows(reverse_mixed_join),
        }
        mixed_actual["mixed_crs_reproject"] = {
            "crs": projected.crs().authid(),
            "features": projected.featureCount(),
            "rows": feature_rows(projected),
        }

        expected = json.loads((root / "expected.json").read_text(encoding="utf-8"))
        actual = {
            "clip": {
                "features": clipped.featureCount(),
                "area_m2": sum(f.geometry().area() for f in clipped.getFeatures()),
            },
            "dissolve_zone": {
                "features": dissolved.featureCount(),
                "areas_m2": {str(f["zone"]): f.geometry().area() for f in dissolved.getFeatures()},
            },
            "buffer": {
                "features": buffered.featureCount(),
                "area_m2": sum(f.geometry().area() for f in buffered.getFeatures()),
            },
            "invalid_geometry_rejected": all(not f.geometry().isGeosValid() for f in invalid.getFeatures()),
        }
        actual.update(mixed_actual)

        def close(a, b):
            return math.isclose(a, b, rel_tol=0, abs_tol=1e-6)
        if (
            actual["clip"]["features"] != expected["clip"]["features"]
            or not close(actual["clip"]["area_m2"], expected["clip"]["area_m2"])
            or actual["dissolve_zone"]["features"] != expected["dissolve_zone"]["features"]
            or actual["dissolve_zone"]["areas_m2"].keys() != expected["dissolve_zone"]["areas_m2"].keys()
            or any(not close(v, expected["dissolve_zone"]["areas_m2"][k])
                   for k, v in actual["dissolve_zone"]["areas_m2"].items())
            or actual["buffer"]["features"] != 1
            or not close(actual["buffer"]["area_m2"], expected["buffer"]["area_m2"])
            or not actual["invalid_geometry_rejected"]
        ):
            raise RuntimeError(f"QGIS reference mismatch: {actual}")

        print(
            "QGIS mixed-CRS measurements: "
            + json.dumps({
                key: actual[key] for key in (
                    "mixed_crs_union", "mixed_crs_spatial_join",
                    "reverse_mixed_crs_spatial_join", "mixed_crs_reproject",
                )
            }, ensure_ascii=False, sort_keys=True),
            flush=True,
        )

        # Reference map images, not screenshots of the interactive QGIS application.
        parcels.renderer().setSymbol(QgsFillSymbol.createSimple({
            "color": "40,120,190,150", "outline_color": "20,60,100,255", "outline_width": "0.3",
        }))
        project.addMapLayer(parcels)
        images = []
        for width, height in ((1280, 800), (1440, 900), (1920, 1080)):
            settings = QgsMapSettings()
            settings.setLayers([parcels])
            settings.setDestinationCrs(crs)
            settings.setOutputSize(QSize(width, height))
            settings.setOutputDpi(96)
            settings.setBackgroundColor(QColor("white"))
            settings.setExtent(QgsRectangle(-20, -20, 320, 120))
            job = QgsMapRendererParallelJob(settings)
            job.start()
            job.waitForFinished()
            if job.errors():
                raise RuntimeError("QGIS reference map renderer reported errors")
            name = f"qgis-map-{width}x{height}.png"
            if not job.renderedImage().save(str(out / name)):
                raise RuntimeError("Could not save reference map")
            images.append({
                "path": name, "width": width, "height": height,
                "sha256": hashlib.sha256((out / name).read_bytes()).hexdigest(),
            })
        project.setFileName(str(out / "reference.qgz"))
        if not project.write():
            raise RuntimeError("Could not save QGIS reference project")
        return {
            "qgis_version": Qgis.QGIS_VERSION, "crs": crs.authid(),
            "fixture_sha256": verified_fixture(root),
            "actual": actual, "reference_map_images": images,
            "limitations": "Headless QGIS engine/map reference only; no desktop interaction or OVG comparison.",
        }
    finally:
        QgsProject.instance().clear()
        # Standalone processing wrappers may outlive local project objects.
        # The dedicated process exits after writing its evidence.


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fixtures", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    root, out = args.fixtures.resolve(), args.out.resolve()
    if root == out or out.is_relative_to(root):
        parser.error("Output must be separate from the immutable fixture directory")
    verified_fixture(root)
    out.mkdir(parents=True, exist_ok=False)
    # System QGIS package commonly places processing under this plugin directory.
    plugins = Path("/usr/share/qgis/python/plugins")
    if plugins.is_dir():
        sys.path.append(str(plugins))
    evidence = run(root, out)
    (out / "qgis-oracle.json").write_text(
        json.dumps(evidence, ensure_ascii=False, indent=2) + "\n", encoding="utf-8",
    )
    print("PASS QGIS engine reference: clip, dissolve, buffer, Union, Spatial Join, Reproject and map render")


if __name__ == "__main__":
    main()
