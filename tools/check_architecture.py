from __future__ import annotations

import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TEXT_EXTENSIONS = {".py", ".ts", ".tsx", ".js", ".jsx", ".json"}
ADAPTER_MARKERS = ("openvietgis_api.adapters", "openvietgis_api/adapters")
SKIP_DIRS = {
    "node_modules",
    ".venv",
    "venv",
    "dist",
    "build",
    "coverage",
    "__pycache__",
}

# Core is allowed to know that official extensions exist only through these two
# generic discovery gateways. They discover by manifest/directory convention and
# never import a named business module.
MODULE_GATEWAYS = {
    "apps/api/openvietgis_api/domains/extensions/runtime.py",
    "apps/web/src/features/extensions/registry.ts",
}

PYTHON_MODULE_IMPORT = re.compile(r"(?m)^\s*(?:from|import)\s+modules(?:\.|\s|$)")
JS_MODULE_IMPORT = re.compile(
    r"""(?m)\b(?:from\s+|import\s*(?:\(\s*)?)["'][^"']*modules/[^"']*["']"""
)
MODULE_PACKAGE_IMPORT = re.compile(r"""["']@openvietgis/module-[^"']+["']""")
MODULE_GLOB = re.compile(r"""import\.meta\.glob\(\s*["'][^"']*modules/[^"']*["']""")


def references_vertical_module_code(content: str) -> bool:
    return any(
        pattern.search(content)
        for pattern in (
            PYTHON_MODULE_IMPORT,
            JS_MODULE_IMPORT,
            MODULE_PACKAGE_IMPORT,
            MODULE_GLOB,
        )
    )


def find_violations(root: Path = ROOT) -> list[str]:
    violations: list[str] = []
    for core_root in (root / "apps", root / "packages"):
        if not core_root.exists():
            continue
        for path in core_root.rglob("*"):
            relative_parts = path.relative_to(core_root).parts
            if any(part in SKIP_DIRS for part in relative_parts):
                continue
            if not path.is_file() or path.suffix not in TEXT_EXTENSIONS:
                continue
            content = path.read_text(encoding="utf-8", errors="ignore")
            relative_to_root = path.relative_to(root).as_posix()
            if (
                relative_to_root not in MODULE_GATEWAYS
                and references_vertical_module_code(content)
            ):
                violations.append(f"{relative_to_root} references vertical module code")
            if relative_to_root.startswith("apps/api/openvietgis_api/domains/") and any(
                marker in content for marker in ADAPTER_MARKERS
            ):
                violations.append(
                    f"{relative_to_root} imports infrastructure adapter code from a domain"
                )
    return violations


def main() -> int:
    violations = find_violations()
    if violations:
        for violation in violations:
            print(f"ARCHITECTURE VIOLATION: {violation}")
        return 1
    print("Architecture boundaries: OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
