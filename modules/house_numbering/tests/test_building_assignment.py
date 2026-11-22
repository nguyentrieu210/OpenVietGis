from modules.house_numbering.building_assignment import RouteIndex,score_candidates


def route(id,name,parent=None,prefix=None,code=None):
    return {"id":id,"name":name,"route_type":"alley" if parent else "street","parent_route_id":parent,
            "prefix":prefix,"source_label":"route-tree-import:"+code if code else "manual",
            "status":"confirmed","revision":1,"content_revision":"a"*64}


def test_address_beats_nearest_and_normalizes_vietnamese():
    ix=RouteIndex([route("main","Đường Nguyễn Văn Linh"),route("alley","Hẻm 155","main","155")])
    ids,reason=ix.match("Nguyen Van Linh","Hẻm 155","",True)
    assert ids=={"alley"} and reason=="exact_address"
    result=score_candidates(ix,{"locked":False,"geometry_usable":True},ids,reason,{"main":1.0,"alley":9.0},80)
    assert result["confidence"]=="high" and result["suggested_route_id"]=="alley"


def test_missing_alley_is_not_equivalent_to_explicit_main_road():
    ix=RouteIndex([route("main","Nguyễn Văn Linh"),route("alley","Hẻm 155","main","155")])
    ids,reason=ix.match("Nguyễn Văn Linh","","",False)
    result=score_candidates(ix,{"locked":False,"geometry_usable":True},ids,reason,{"main":1.0,"alley":2.0},80)
    assert result["confidence"]=="low" and result["suggested_route_id"] is None


def test_duplicate_road_names_and_code_conflict_require_review():
    ix=RouteIndex([route("a","Đường A",code="T01"),route("b","Đường A"),route("c","Đường C",code="T02")])
    ids,reason=ix.match("Đường A","","",True)
    assert ids=={"a","b"}
    result=score_candidates(ix,{"locked":False,"geometry_usable":True},ids,reason,{"a":1.0,"b":5.0},80)
    assert result["confidence"]=="conflict"
    ids,reason=ix.match("Đường A","","T02",True)
    assert reason=="attribute_conflict"
    assert score_candidates(ix,{"locked":False,"geometry_usable":True},ids,reason,{"a":1.0,"c":2.0},80)["suggested_route_id"] is None


def test_geometry_only_not_high_confidence_and_locked_building_blocked():
    ix=RouteIndex([route("a","Đường A")])
    ids,reason=ix.match("","","",False)
    assert score_candidates(ix,{"locked":False,"geometry_usable":True},ids,reason,{"a":0.5},80)["confidence"]=="low"
    assert score_candidates(ix,{"locked":True,"geometry_usable":True},ids,reason,{"a":0.5},80)["confidence"]=="blocked"


def test_deep_alley_prefix_keeps_hierarchy():
    ix=RouteIndex([route("a","Đường A"),route("b","Hẻm 155","a","155"),route("c","Hẻm 155/3","b","155/3")])
    ids,reason=ix.match("Đường A","155 / 3","",True)
    assert ids=={"c"} and reason=="exact_address"
