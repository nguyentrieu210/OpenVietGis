import pytest
from modules.house_numbering.address_prefix import canonical_prefix
from modules.house_numbering.domain import Address, Rule, suggest_number
from modules.house_numbering.route_import import _prefix,group_routes,plan_routes

@pytest.mark.parametrize("raw,expected",[("30a","30A"),("c19/94/60","C19/94/60"),("580/A11","580/A11"),("155/3","155/3")])
def test_prefix_valid(raw,expected): assert canonical_prefix(raw)==expected

@pytest.mark.parametrize("bad",["0","A0","01","ABC","1 Test","1//2","1/","/1","A-1","É1"])
def test_prefix_invalid(bad):
    with pytest.raises(ValueError): canonical_prefix(bad)

def test_child_prefix_and_number():
    assert _prefix("C19","30a")=="C19/30A"
    assert _prefix("C19","c19/30a")=="C19/30A"
    result=suggest_number([Address("a","C19/30A/2",10,"right","approved"),Address("b","C19/30A/4",30,"right","approved")],measure=20,side="right",rule=Rule("test"),prefix="C19/30A")
    assert result.number=="C19/30A/2A"

def test_opt_in_grouping_holds_aliases_and_rewrites_parent():
    def row(code,name,logical="",parent="",kind="street",prefix=""):
        return {"key":"s:"+code,"source_id":"s","feature_id":code,"code":code,"name":name,"logical_code":logical,"parent_code":parent,"route_type":kind,"prefix":prefix,"direction_state":"forward","errors":[],"content_revision":"r"}
    rows=[row("a","Main","logical"),row("b","Main","logical"),row("c","Alley",parent="b",kind="alley",prefix="30A")]
    result=plan_routes(group_routes(rows),[])
    assert len(result)==2
    assert all(r["state"]=="ready" for r in result)
    assert result[1]["parent_key"]==result[0]["key"]
    assert [s["code"] for s in result[0]["segments"]]==["a","b"]
    rows=[row("a","Main","logical"),row("b","Different","logical")]
    assert "inconsistent_logical_route" in group_routes(rows)[0]["errors"]
