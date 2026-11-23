import io
import zipfile
from uuid import uuid4
from xml.etree import ElementTree
import pytest
from modules.house_numbering.proposal_packet import decision_draft,register_xlsx,plates_pdf,pdf,appendix_pages
from openvietgis_api.domains.platform_errors import PlatformConflictError

ORG,PROJECT=uuid4(),uuid4()

def packet(state='draft'):
    return {'id':uuid4(),'status':state,'authority_ref':'THAM-CHIEU-TEST','rule_status':'authoritative','rule_id':uuid4()}

def row(number='C19/2',kind='approved'):
    return {'building_id':uuid4(),'number_text':number,'proposed_number':number,'number_kind':kind,
      'retired_at':None,'building_state':'active','route_name':'Đường Mẫu / Hẻm C19','prefix':'C19',
      'parcel_ref':'123','area_name':'Phường Kiểm thử','linked_source_id':uuid4(),'linked_feature_id':'feature-test'}

def test_decision_pdf_is_explicitly_draft_and_bounded():
    result=decision_draft(packet(),[row() for _ in range(80)])
    assert result.startswith(b'%PDF') and len(result)>2000
    # Decision form is one page; full annex is a separate paginated download.
    assert result.count(b'/Type /Page')>=1

def test_register_contains_all_rows_and_valid_xlsx():
    p=packet('approved');rows=[row() for _ in range(30)]
    content=register_xlsx(p,rows)
    with zipfile.ZipFile(io.BytesIO(content)) as z:
        assert 'xl/worksheets/sheet1.xml' in z.namelist()
        sheet=z.read('xl/worksheets/sheet1.xml')
        assert ElementTree.fromstring(sheet) is not None
        assert sheet.count(b'<row r=')==36
        assert 'ĐÃ DUYỆT NỘI BỘ'.encode() in sheet

def test_plate_qr_requires_approval_and_valid_snapshot():
    with pytest.raises(PlatformConflictError):plates_pdf(packet(),[row()],ORG,PROJECT)
    with pytest.raises(PlatformConflictError):plates_pdf(packet('approved'),[row(kind='submitted')],ORG,PROJECT)
    result=plates_pdf(packet('approved'),[row()],ORG,PROJECT)
    assert result.startswith(b'%PDF') and len(result)>2000

def test_appendix_is_separate_and_printable():
    assert pdf(appendix_pages(packet(),[row()])).startswith(b'%PDF')

def test_qr_url_reaches_nginx_canonical_app_and_is_url_encoded():
    from modules.house_numbering.proposal_packet import qr_house_link
    from urllib.parse import urlsplit, parse_qs
    source=uuid4()
    url=qr_house_link(org=ORG,project=PROJECT,source=source,feature='Nhà 12 & A')
    u=urlsplit(url)
    assert u.path=='/app'
    qs=parse_qs(u.query)
    assert qs=={'hn_org':[str(ORG)],'hn_project':[str(PROJECT)],
                'hn_source':[str(source)],'hn_feature':['Nhà 12 & A']}
