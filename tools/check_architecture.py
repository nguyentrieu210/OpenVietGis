from __future__ import annotations

import ast
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

# Core is allowed to know that official extensions exist only through these
# generic discovery gateways. They discover by manifest/directory convention and
# never import a named business module.
MODULE_GATEWAYS = {
    "apps/api/openvietgis_api/adapters/http/official_modules.py",
    "apps/web/src/features/extensions/registry.ts",
    "apps/web-next/src/official-module-registry.ts",
}

PYTHON_MODULE_IMPORT = re.compile(r"(?m)^\s*(?:from|import)\s+modules(?:\.|\s|$)")
JS_MODULE_IMPORT = re.compile(
    r"""(?m)\b(?:from\s+|import\s*(?:\(\s*)?)["'][^"']*modules/[^"']*["']"""
)
MODULE_PACKAGE_IMPORT = re.compile(r"""["']@openvietgis/module-[^"']+["']""")
MODULE_GLOB = re.compile(
    r"""import\.meta\.glob(?:\s*<[^>]+>)?\s*\(\s*["'][^"']*modules/[^"']*["']"""
)


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


# Runtime packages must not import app internals. Browser fixtures may exercise apps.
# Canonical frontend must not import retired application source.
JS_IMPORT = re.compile(
    r"""(?:\bfrom\s*|\bimport\s*(?:\(\s*)?|\brequire\s*\(\s*)["']([^"']+)["']"""
)
DOMAIN_FRAMEWORKS = {"fastapi", "starlette", "sqlalchemy", "psycopg", "psycopg2"}


def runtime_source(relative: str) -> bool:
    parts = relative.split("/")
    return (
        "src" in parts
        and "tests" not in parts
        and not any(
            part.endswith((".test.ts", ".test.tsx", ".spec.ts", ".spec.tsx"))
            for part in parts
        )
    )


def frontend_boundary_violations(root: Path, path: Path, content: str) -> list[str]:
    relative = path.relative_to(root).as_posix()
    module_frontend = relative.startswith("modules/") and "/frontend/" in relative and "/tests/" not in relative
    if not (runtime_source(relative) or module_frontend) or path.suffix not in {
        ".ts",
        ".tsx",
        ".js",
        ".jsx",
    }:
        return []
    errors = []
    for match in JS_IMPORT.finditer(content):
        imported = match.group(1)
        target = ""
        if imported.startswith("."):
            resolved = (path.parent / imported).resolve()
            if resolved.is_relative_to(root.resolve()):
                target = resolved.relative_to(root.resolve()).as_posix()
        elif imported.startswith("@openvietgis/web-next"):
            target = "apps/web-next/"
        elif imported.startswith("@openvietgis/web"):
            target = "apps/web/"
        if (relative.startswith("packages/") or module_frontend) and target.startswith("apps/"):
            errors.append(f"{relative} imports app internals: {imported}")
        if relative.startswith("apps/web/src/") and target.startswith("apps/web-next/"):
            errors.append(f"{relative} imports canonical app internals: {imported}")
        if (
            relative.startswith("apps/web-next/src/")
            and target.startswith("apps/web/")
        ):
            errors.append(f"{relative} imports retired frontend source: {imported}")
    return errors


def domain_framework_violations(relative: str, content: str) -> list[str]:
    try:
        tree = ast.parse(content)
    except SyntaxError:
        return [f"{relative} cannot be parsed for domain dependency validation"]
    errors = []
    for node in ast.walk(tree):
        names = (
            [alias.name for alias in node.names]
            if isinstance(node, ast.Import)
            else [node.module or ""]
            if isinstance(node, ast.ImportFrom)
            else []
        )
        for name in names:
            if name.split(".")[0] in DOMAIN_FRAMEWORKS:
                errors.append(
                    f"{relative}:{node.lineno} domain imports framework {name}"
                )
    return errors


RUST_VERTICAL_IMPORT = re.compile(r"\bovg_module_(?!(?:sdk|http)\b)[a-zA-Z0-9_]+\b")
RUST_VERTICAL_DEP = re.compile(r"(?m)^\s*ovg-module-(?!(?:sdk|http)\s*=)[a-zA-Z0-9_-]+\s*=")
RUST_APP_IMPORT = re.compile(r"\bopenvietgis_api_rust\b|(?:path\s*=\s*\x22[^\x22]*apps/api-rust)")
RUST_COMPOSITION = {"apps/api-rust/src/state.rs", "apps/api-rust/src/routes.rs", "apps/api-rust/src/lib.rs", "apps/api-rust/src/module_http_host.rs"}


def rust_boundary_violations(root: Path) -> list[str]:
    errors = []
    for base in ("crates", "modules", "apps/api-rust"):
        for path in (root / base).rglob("*"):
            if not path.is_file() or any(part in SKIP_DIRS | {"target", "target-build"} for part in path.relative_to(root).parts):
                continue
            if path.suffix != ".rs" and path.name != "Cargo.toml":
                continue
            rel = path.relative_to(root).as_posix()
            content = path.read_text(encoding="utf-8", errors="ignore")
            # Do not mistake a quoted example of an extension key for a code import.
            if rel.startswith("crates/") and (RUST_VERTICAL_IMPORT.search(content) or RUST_VERTICAL_DEP.search(content)):
                errors.append(f"{rel} Core Rust depends on vertical module")
            if rel.startswith("modules/") and RUST_APP_IMPORT.search(content):
                errors.append(f"{rel} Rust module depends on HTTP application internals")
            if rel.startswith("apps/api-rust/src/") and rel not in RUST_COMPOSITION and RUST_VERTICAL_IMPORT.search(content):
                errors.append(f"{rel} Rust vertical import outside composition")
    if (root / "crates/ovg-storage/src/house_numbering").exists():
        errors.append("crates/ovg-storage/src/house_numbering is module-owned persistence in Core")
    if (root / "apps/api-rust/src/house_numbering").exists():
        errors.append("apps/api-rust/src/house_numbering is module-owned HTTP adapter in Core")
    return errors


def find_violations(root: Path = ROOT) -> list[str]:
    violations: list[str] = rust_boundary_violations(root)
    for core_root in (root / "apps", root / "packages", root / "modules"):
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
            violations.extend(frontend_boundary_violations(root, path, content))
            if (
                relative_to_root.startswith("apps/api/openvietgis_api/domains/")
                and path.suffix == ".py"
            ):
                violations.extend(
                    domain_framework_violations(relative_to_root, content)
                )
            if (
                not relative_to_root.startswith("modules/")
                and relative_to_root not in MODULE_GATEWAYS
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
