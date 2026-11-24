from tools.local_candidate_gate import REQUIRED, local_failures
from tools.release_consolidation import hash_file


def test_local_gate_rejects_missing_changed_and_wrong_candidate(tmp_path):
    report = {
        "source_sha": "a" * 40,
        "clean_after": True,
        "production_approved": False,
        "steps": {},
    }
    assert local_failures(report, tmp_path, "a" * 40)
    for step in REQUIRED:
        path = tmp_path / (step + ".log")
        path.write_text("fixture")
        report["steps"][step] = {"exit_code": 0, "sha256": hash_file(path)}
    assert not local_failures(report, tmp_path, "a" * 40)
    assert local_failures(report, tmp_path, "b" * 40)
    (tmp_path / "browser.log").write_text("changed")
    assert local_failures(report, tmp_path, "a" * 40)
    report["production_approved"] = True
    assert "Local evidence cannot approve production." in local_failures(
        report, tmp_path, "a" * 40
    )
