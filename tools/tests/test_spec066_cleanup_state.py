import subprocess
from pathlib import Path

import yaml

ROOT=Path(__file__).resolve().parents[2]

def tracked():
    return subprocess.check_output(['git','ls-files'],cwd=ROOT,text=True).splitlines()

def test_single_frontend_workspace_and_release_consumers():
    workspace=yaml.safe_load((ROOT/'pnpm-workspace.yaml').read_text())
    assert 'apps/web' not in workspace['packages']
    assert 'apps/web-next' in workspace['packages']
    assert not (ROOT/'apps/web').exists()
    lock=(ROOT/'pnpm-lock.yaml').read_text()
    assert '\n  apps/web:\n' not in lock
    assert '\n  apps/web-next:\n' in lock
    docker=(ROOT/'infra/prod/Dockerfile.web').read_text()
    assert 'apps/web/package.json' not in docker
    assert 'apps/web-next/package.json' in docker

def test_no_committed_generated_or_vendor_outputs():
    bad=[]
    exact={'node_modules','dist','build','coverage','__pycache__','.pytest_cache','.vite','target'}
    for path in tracked():
        parts=Path(path).parts
        if any(part in exact for part in parts) or path.endswith(('.pyc','.map')):
            bad.append(path)
    assert bad==[]

def test_ignore_contract_covers_generated_outputs():
    gitignore=(ROOT/'.gitignore').read_text()
    dockerignore=(ROOT/'.dockerignore').read_text()
    for token in ('node_modules/','dist/','build/','coverage/','__pycache__/'):
        assert token in gitignore
        assert token in dockerignore
    assert '/target/' in gitignore
    assert '/apps/web-next/public/cesium/' in gitignore

def test_current_architecture_does_not_present_legacy_frontend_as_active():
    architecture=(ROOT/'docs/architecture.md').read_text()
    assert 'single canonical public/live frontend' in architecture
    assert 'retired by Spec 066' in architecture
    assert 'legacy/compatibility shell' not in architecture
