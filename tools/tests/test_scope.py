from pathlib import Path

from tools.check_scope import find_scope_violations


def test_scope_checker_accepts_product_neutral_runtime(tmp_path: Path) -> None:
    source = tmp_path / "apps" / "api" / "neutral.py"
    source.parent.mkdir(parents=True)
    source.write_text("class Organization: pass\n", encoding="utf-8")

    assert find_scope_violations(tmp_path) == []


def test_scope_checker_rejects_gis_or_vertical_runtime_markers(tmp_path: Path) -> None:
    source = tmp_path / "apps" / "web" / "src" / "bad.ts"
    source.parent.mkdir(parents=True)
    source.write_text('import "maplibre-gl"; // house-number\n', encoding="utf-8")

    violations = find_scope_violations(tmp_path)

    assert len(violations) == 2
    assert any("map rendering engine" in item for item in violations)
    assert any("house-numbering vertical" in item for item in violations)


def test_accepted_renderer_stays_inside_map_engine_adapter(tmp_path: Path) -> None:
    map_root = tmp_path / "apps/web/src/features/map"
    map_root.mkdir(parents=True)
    (map_root / "maplibre-engine.ts").write_text(
        'import { Map } from "maplibre-gl";\\n', encoding="utf-8"
    )
    (map_root / "MapWorkspace.tsx").write_text(
        'import { createEngine } from "./maplibre-engine";\\n', encoding="utf-8"
    )
    assert find_scope_violations(tmp_path) == []


def test_business_map_ui_cannot_import_renderer_types_or_dynamic_api(
    tmp_path: Path,
) -> None:
    source = tmp_path / "apps/web/src/features/map/MapWorkspace.tsx"
    source.parent.mkdir(parents=True)
    for content in (
        'import type { Map } from "maplibre-gl";',
        'const renderer = await import("maplibre-gl");',
        'import "maplibre-gl/dist/maplibre-gl.css";',
    ):
        source.write_text(content, encoding="utf-8")
        violations = find_scope_violations(tmp_path)
        assert len(violations) == 1
        assert "outside the MapEngine adapter" in violations[0]


def test_studio_renderer_adapter_is_narrowly_scoped(tmp_path: Path) -> None:
    root = tmp_path / "apps/web-next/src"
    root.mkdir(parents=True)
    (root / "map-surface.ts").write_text(
        'export { Map } from "maplibre-gl";', encoding="utf-8"
    )
    (root / "MapWorkspace.tsx").write_text(
        'import { Map } from "./map-surface";', encoding="utf-8"
    )
    assert find_scope_violations(tmp_path) == []

    (root / "MapWorkspace.tsx").write_text(
        'import { Map } from "maplibre-gl";', encoding="utf-8"
    )
    assert len(find_scope_violations(tmp_path)) == 1


def test_3d_adapter_does_not_grant_renderer_access_to_coordinator(
    tmp_path: Path,
) -> None:
    root = tmp_path / "packages/spatial-workspace/src"
    root.mkdir(parents=True)
    adapter = root / "maplibre3d.ts"
    adapter.write_text('const engine = import("maplibre-gl");', encoding="utf-8")
    assert find_scope_violations(tmp_path) == []

    (root / "coordinator.ts").write_text(
        'import type { Map } from "maplibre-gl";', encoding="utf-8"
    )
    assert len(find_scope_violations(tmp_path)) == 1


def test_renderer_owner_still_rejects_vertical_dependencies(tmp_path: Path) -> None:
    root = tmp_path / "apps/web-next/src"
    root.mkdir(parents=True)
    (root / "map-surface.ts").write_text(
        'import "maplibre-gl"; import "house-number";', encoding="utf-8"
    )
    violations = find_scope_violations(tmp_path)
    assert len(violations) == 1
    assert "house-numbering vertical" in violations[0]


def test_r031_adapter_allowlist_is_narrow(tmp_path: Path) -> None:
    root = tmp_path / "apps/web-next"
    (root / "src").mkdir(parents=True)
    (root / "package.json").write_text('{"dependencies":{"terra-draw":"1.40.0"}}')
    (root / "src/TerraDraftValidation.ts").write_text(
        'import {ValidateNotSelfIntersecting} from "terra-draw";'
    )
    workspace = root / "src/SpatialWorkspace.tsx"
    workspace.write_text('const label = "3D Tiles · MapLibre (tương thích)";', encoding="utf-8")
    cesium = tmp_path / "packages/spatial-workspace/src/cesium3d.ts"
    cesium.parent.mkdir(parents=True)
    cesium.write_text(
        'import type {GeographicCamera,NamedModel,SelectableFootprintEngine} '
        'from "./maplibre3d";'
    )
    assert find_scope_violations(tmp_path) == []
    workspace.write_text('import {TerraDraw} from "terra-draw";')
    assert len(find_scope_violations(tmp_path)) == 2
    workspace.write_text('import "maplibre-gl";')
    cesium.write_text('import "maplibre-gl";')
    assert len(find_scope_violations(tmp_path)) == 2
    (root / "src/TerraDraftValidation.ts").write_text('import "house-number";')
    assert len(find_scope_violations(tmp_path)) == 3


def test_hybrid_retained_marker_allowlist_is_path_specific(tmp_path: Path) -> None:
    api = tmp_path / "apps/api-rust/Cargo.toml"
    api.parent.mkdir(parents=True)
    api.write_text('house-numbering = []', encoding="utf-8")

    web = tmp_path / "apps/web-next/src"
    web.mkdir(parents=True)
    (web / "AppMarket.tsx").write_text('const module = "house_numbering";', encoding="utf-8")
    (web / "DsmPanes.tsx").write_text('const example = "/geoserver/wfs";', encoding="utf-8")
    (web / "LiveApp.tsx").write_text('import { Map } from "maplibre-gl";', encoding="utf-8")

    assert find_scope_violations(tmp_path) == []

    (web / "Unexpected.tsx").write_text(
        'const module = "house_numbering"; const server = "geoserver"; import "maplibre-gl";',
        encoding="utf-8",
    )
    violations = find_scope_violations(tmp_path)
    assert len(violations) == 3
