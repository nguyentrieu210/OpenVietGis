from modules.house_numbering.batch_numbering import build_route_plan
from modules.house_numbering.domain import Rule


def point(bid,x,y,number=None,kind=None):
    return {"id":bid,"x":x,"y":y,"number_id":"n"+bid if number else None,"number_text":number,"kind":kind}
def target():
    return {"state":"active","eligibility":"eligible"}
def plan(points,numbers,ids):
    return build_route_plan(route={"id":"r","srid":3857,"prefix":"C19"},vertices=[(0,0),(100,0)],
        rule=Rule("test",max_distance=30),points=points,numbers=numbers,targets={b:target() for b in ids})
def test_reserved_before_anchor_and_tied_chainage():
    points=[point("anchor",40,-3,"C19/10","approved"),point("a",10,-3),point("b",20,-4),
            point("c",60,-3),point("d",30,3),point("e",30,4)]
    nums=[{"id":"nanchor","building_id":"anchor","number_text":"C19/10","kind":"approved","side":"right","measure_m":40},
          {"id":"reserved","building_id":None,"number_text":"C19/2","kind":"reserved","side":"right","measure_m":None}]
    rows,warnings=plan(points,nums,["e","anchor","c","b","d","a"])
    assert {r["building_id"]:r["number"] for r in rows}=={"a":"C19/4","b":"C19/6","c":"C19/12"}
    assert len(warnings)==3
def test_existing_evidence_conflicting_sequence_is_reviewed():
    points=[point("a",10,-4,"C19/8","survey"),point("b",20,-4,"C19/2","survey")]
    nums=[{"id":"n"+bid,"building_id":bid,"number_text":number,"kind":"survey","side":"right","measure_m":m}
          for bid,number,m in [("a","C19/8",10),("b","C19/2",20)]]
    rows,warnings=plan(points,nums,["a","b"])
    assert not rows and len(warnings)==2
