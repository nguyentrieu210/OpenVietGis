from tools.product_inventory import is_test, matches, python_candidates, ui_candidates


def test_routes():
    rows = python_candidates(
        '@router.get(BASE + "/items")\nasync def list_items(): pass\n',
        "apps/api/example.py",
    )
    assert len(rows) == 1
    assert rows[0]["name"] == "list_items"
    assert rows[0]["line"] == 1


def test_mcp_decorator_and_wrapper():
    rows = python_candidates(
        '@mcp.tool()\ndef gis_one(): pass\ntool("gis_two", Model)\n',
        "apps/mcp/workspace_tools.py",
    )
    assert [row["name"] for row in rows] == ["gis_one", "gis_two"]


def test_mcp_literal_registry():
    rows = python_candidates(
        'TOOLS = {"gis_example": (Model, "example")}\n', "apps/mcp/advanced_tools.py"
    )
    assert rows[0]["name"] == "gis_example"


def test_ui_candidates():
    rows = ui_candidates(
        "<button onClick={run}>Run</button>\n", "apps/web-next/src/Test.tsx"
    )
    assert len(rows) == 2
    assert all(row["line"] == 1 for row in rows)


def test_classification():
    assert is_test("apps/api/tests/integration/test_case.py")
    assert is_test("apps/web/src/Thing.test.tsx")
    assert not is_test("tests/e2e/visual.spec.ts-snapshots/fixture.png")
    assert matches("apps/api/x.py", ["apps/api/*"])
