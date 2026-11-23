from __future__ import annotations

import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RUNTIME_ROOTS = (ROOT / "apps", ROOT / "packages")
SKIP_DIRS = {"node_modules", ".venv", "dist", "build", "coverage", "__pycache__", "generated"}
TEXT_EXTENSIONS = {".py", ".ts", ".tsx", ".js", ".jsx", ".json", ".toml"}
FORBIDDEN_MARKERS = {
    "maplibre": "map rendering engine",
    "terra-draw": "GIS editing engine",
    "terradraw": "GIS editing engine",
    "geoserver": "OGC server",
    "martin": "tile server",
    "house-number": "house-numbering vertical",
    "house_number": "house-numbering vertical",
    "binh-hung": "legacy vertical",
    "customrolebuilder": "custom-role builder",
    "publicsharelink": "public/anonymous sharing",
}
MAP_UI_ROOT = "apps/web/src/features/map/"
MAP_ENGINE_ADAPTER = MAP_UI_ROOT + "maplibre-engine.ts"
# Explicit renderer ownership paths. The Studio compatibility adapter and the
# independent 3D adapter do not grant renderer access to their parent folders.
RENDERER_OWNERS = {
    MAP_ENGINE_ADAPTER,
    "apps/web-next/src/map-surface.ts",
    "packages/spatial-workspace/src/maplibre3d.ts",
    # Isolated browser fixture bootstraps a renderer worker; never production.
    "packages/spatial-workspace/tests/browser/3d.ts",
}
RENDERER_MANIFESTS = {
    "apps/web/package.json",
    "apps/web-next/package.json",
    "apps/web-next/tsconfig.json",
    "packages/spatial-workspace/package.json",
}
RENDERER_IMPORT = re.compile(
    r"""(?:from\s*|import\s*(?:\(\s*)?)["']maplibre-gl(?:/[^"']*)?["']"""
)


def find_scope_violations(root: Path = ROOT) -> list[str]:
    violations: list[str] = []
    for runtime_root in (root / "apps", root / "packages"):
        if not runtime_root.exists():
            continue
        for path in runtime_root.rglob("*"):
            if not path.is_file() or path.suffix.lower() not in TEXT_EXTENSIONS:
                continue
            if any(part in SKIP_DIRS for part in path.relative_to(runtime_root).parts):
                continue
            content = path.read_text(encoding="utf-8", errors="ignore").lower()
            relative = path.relative_to(root).as_posix()
            for marker, label in FORBIDDEN_MARKERS.items():
                if marker == "maplibre":
                    if relative in RENDERER_OWNERS or relative in RENDERER_MANIFESTS:
                        continue
                    # The legacy R004 UI may mention the engine by name, but it
                    # may not import renderer APIs, types or CSS.
                    if relative.startswith(MAP_UI_ROOT):
                        if RENDERER_IMPORT.search(content):
                            violations.append(
                                f"{relative} imports renderer APIs outside the MapEngine adapter"
                            )
                        continue
                if marker in content:
                    violations.append(
                        f"{relative} contains out-of-scope {label} marker {marker!r}"
                    )
    return violations


def main() -> int:
    violations = find_scope_violations()
    if violations:
        for violation in violations:
            print(f"SCOPE VIOLATION: {violation}")
        return 1
    print("Accepted runtime scope and MapEngine boundary: OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
