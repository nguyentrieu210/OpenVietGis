from pathlib import Path

from tools.check_architecture import find_violations

FIXTURES = Path(__file__).parent / "fixtures"


def test_rejects_core_to_vertical_module_dependency() -> None:
    violations = find_violations(FIXTURES / "invalid_core_to_module")

    assert violations
    assert any("apps/core/bad.ts" in violation for violation in violations)


def test_accepts_module_to_public_contract_dependency() -> None:
    assert find_violations(FIXTURES / "valid_module_to_contract") == []


def test_core_validation_succeeds_when_modules_directory_is_absent(
    tmp_path: Path,
) -> None:
    core = tmp_path / "apps" / "core"
    core.mkdir(parents=True)
    (core / "index.py").write_text("VALUE = 1\n", encoding="utf-8")

    assert find_violations(tmp_path) == []


def test_rejects_domain_to_adapter_dependency() -> None:
    violations = find_violations(FIXTURES / "invalid_domain_to_adapter")

    assert violations
    assert any(
        "imports infrastructure adapter code from a domain" in item
        for item in violations
    )


def test_does_not_treat_module_api_urls_or_prose_as_code_dependency(
    tmp_path: Path,
) -> None:
    core = tmp_path / "packages" / "client"
    core.mkdir(parents=True)
    (core / "url.ts").write_text(
        'export const endpoint = "/projects/p/modules/house-numbering";\\n',
        encoding="utf-8",
    )
    api = tmp_path / "apps" / "api"
    api.mkdir(parents=True)
    (api / "note.py").write_text(
        '"""Official modules. Stay generic."""\\n', encoding="utf-8"
    )

    assert find_violations(tmp_path) == []


def write_source(root: Path, relative: str, content: str) -> None:
    path = root / relative
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8")


def test_rejects_package_to_app_import(tmp_path: Path) -> None:
    write_source(
        tmp_path,
        "packages/spatial-workspace/src/bad.ts",
        'import {X} from "../../../apps/web-next/src/X";',
    )
    assert any("imports app internals" in v for v in find_violations(tmp_path))


def test_canonical_frontend_rejects_all_legacy_imports(tmp_path: Path) -> None:
    source = 'const X = import("../../web/src/features/X");'
    write_source(tmp_path, "apps/web-next/src/legacy-workbench-bridge.ts", source)
    assert any("imports retired frontend source" in v for v in find_violations(tmp_path))
    write_source(tmp_path, "apps/web-next/src/bad.ts", source)
    assert len([v for v in find_violations(tmp_path) if "imports retired frontend source" in v]) == 2


def test_reverse_frontend_dependency_rejected(tmp_path: Path) -> None:
    write_source(
        tmp_path, "apps/web/src/bad.ts", 'import "@openvietgis/web-next/src/X";'
    )
    assert any("canonical app internals" in v for v in find_violations(tmp_path))


def test_domain_framework_rejected_but_adapter_allowed(tmp_path: Path) -> None:
    source = "from fastapi import APIRouter\n"
    write_source(tmp_path, "apps/api/openvietgis_api/adapters/http/valid.py", source)
    assert find_violations(tmp_path) == []
    write_source(tmp_path, "apps/api/openvietgis_api/domains/bad.py", source)
    assert any("domain imports framework" in v for v in find_violations(tmp_path))


def test_public_sdk_and_test_fixture_allowed(tmp_path: Path) -> None:
    write_source(
        tmp_path,
        "apps/web-next/src/valid.ts",
        'import {ApiClient} from "@openvietgis/client-sdk";',
    )
    write_source(
        tmp_path,
        "packages/spatial-workspace/tests/browser/valid.ts",
        'import {X} from "../../../../apps/web-next/src/X";',
    )
    assert find_violations(tmp_path) == []


def test_generic_module_glob_requires_discovery_gateway(tmp_path: Path) -> None:
    source = 'const x = import.meta.glob<Manifest>("../../../modules/*/manifest.json");'
    write_source(tmp_path, "apps/web-next/src/official-module-registry.ts", source)
    assert find_violations(tmp_path) == []
    write_source(tmp_path, "apps/web-next/src/bad.ts", source)
    assert any(
        "references vertical module code" in v for v in find_violations(tmp_path)
    )


def test_module_frontend_cannot_import_app_internals(tmp_path: Path) -> None:
    write_source(tmp_path, "modules/example/frontend/Panel.tsx",
                 'import {X} from "../../../apps/web-next/src/X";')
    assert any("imports app internals" in v for v in find_violations(tmp_path))

def test_module_frontend_can_import_public_plugin_sdk(tmp_path: Path) -> None:
    write_source(tmp_path, "modules/example/frontend/Panel.tsx",
                 'import {Modal} from "@openvietgis/spatial-workspace/plugin/studio-ui";')
    assert find_violations(tmp_path) == []


def test_rust_core_cannot_import_or_depend_on_vertical_module(tmp_path: Path) -> None:
    write_source(tmp_path, "crates/ovg-storage/src/bad.rs", "use ovg_module_house_numbering_storage as store;")
    write_source(tmp_path, "crates/ovg-core/Cargo.toml", "ovg-module-house-numbering = { workspace = true }")
    assert len([v for v in find_violations(tmp_path) if "Core Rust depends on vertical module" in v]) == 2


def test_rust_module_can_use_core_ports_but_not_http_app(tmp_path: Path) -> None:
    write_source(tmp_path, "modules/example/storage/src/lib.rs", "use ovg_storage::catalog::ProjectRead;")
    write_source(tmp_path, "crates/ovg-storage/Cargo.toml", "ovg-module-sdk = { workspace = true }")
    assert find_violations(tmp_path) == []
    write_source(tmp_path, "modules/example/storage/Cargo.toml", 'host = { path = "../../../apps/api-rust" }')
    assert any("HTTP application internals" in v for v in find_violations(tmp_path))


def test_rust_legacy_storage_location_cannot_return(tmp_path: Path) -> None:
    write_source(tmp_path, "crates/ovg-storage/src/house_numbering/mod.rs", "// old code")
    assert any("module-owned persistence in Core" in v for v in find_violations(tmp_path))


def test_rust_http_sdk_allowed_but_legacy_handler_path_rejected(tmp_path: Path) -> None:
    write_source(tmp_path, "crates/ovg-core/Cargo.toml", "ovg-module-http = { workspace = true }")
    write_source(tmp_path, "modules/example/http/src/lib.rs", "use ovg_module_http::ModuleSession;")
    assert find_violations(tmp_path) == []
    write_source(tmp_path, "apps/api-rust/src/house_numbering/mod.rs", "// legacy")
    assert any("module-owned HTTP adapter in Core" in v for v in find_violations(tmp_path))
