from modules.house_numbering.address_batch import group_key

STAGE={"snapshot":{"name":"Đường A hẻm 91","resolved_prefix":"91","code":"legacy"}}

def row(**kwargs):
    return {"status":"pending","eligible":True,"can_belong":True,"kind":"missing_prefix","road":"Đường A","alley":"","code":"legacy",**kwargs}

def test_normalized_similar_fields_share_group():
    assert group_key(STAGE,row())==group_key(STAGE,row(road="Duong A"))
    assert group_key(STAGE,row()) is not None

def test_different_source_code_cannot_share_group():
    assert group_key(STAGE,row())!=group_key(STAGE,row(code="other"))

def test_conflict_and_existing_decisions_cannot_batch():
    for x in [row(kind="different_road",road="B"),row(kind="descendant",alley="91/14"),row(status="verified"),row(eligible=False),row(can_belong=False),row(code="")]:
        assert group_key(STAGE,x) is None

def test_missing_road_requires_exact_prefix():
    assert group_key(STAGE,row(kind="missing_road",road="",alley="91"))
    assert group_key(STAGE,row(kind="missing_road",road="",alley="91/14")) is None
    assert group_key(STAGE,row(kind="missing_road",road="",alley="")) is None
