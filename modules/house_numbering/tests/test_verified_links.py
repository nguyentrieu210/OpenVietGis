from modules.house_numbering.verified_links import link_status

PATH=("Đường A","91")
def record(**kw):
    return {"decision":"belongs","state":"active","eligibility":"eligible","route_id":None,"locked":False,"evidence_token":"abc","review_revision":1,"verified_road":"Đường A","verified_prefix":"91",**kw}
def current(**kw):
    return {"status":"verified","decision":"belongs","can_belong":True,"evidence_token":"abc","revision":1,**kw}

def test_verified_current_is_ready():
    assert link_status(record(),current(),"target",PATH)=="ready"

def test_other_address_never_linked():
    assert link_status(record(decision="other_address"),current(),"target",PATH)=="other_address"

def test_existing_route_and_protected_number():
    assert link_status(record(route_id="target"),None,"target",PATH)=="linked"
    assert link_status(record(route_id="another"),current(),"target",PATH)=="blocked"
    assert link_status(record(locked=True),current(),"target",PATH)=="blocked"
    assert link_status(record(eligibility="ineligible"),current(),"target",PATH)=="blocked"

def test_stale_evidence_and_mismatched_address():
    for r,c in [(record(),None),(record(),current(can_belong=False)),(record(),current(revision=2)),(record(),current(evidence_token="different")),(record(verified_prefix="91/14"),current())]:
        assert link_status(r,c,"target",PATH)=="stale"
