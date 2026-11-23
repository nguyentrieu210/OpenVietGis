"""Draft decision, numbered register and authenticated QR plates (no PII)."""
from __future__ import annotations
import io, zipfile
from datetime import date
from html import escape
from typing import Any
from urllib.parse import urlencode
from uuid import UUID
import qrcode
from PIL import Image, ImageDraw
from sqlalchemy import text
from sqlalchemy.orm import Session
from openvietgis_api.domains.authorization.context import AuthorizationContext
from openvietgis_api.domains.platform_errors import PlatformConflictError, PlatformValidationError
from modules.house_numbering.documents import _font
from modules.house_numbering.persistence import HouseNumberingRepository
from modules.house_numbering.progress import _cell, _col_name

WIDTH,HEIGHT=1240,1754

def list_proposals(repo:HouseNumberingRepository,status:str|None=None,limit:int=100,offset:int=0)->dict[str,Any]:
    repo.prepare('read')
    if status not in (None,'draft','submitted','approved') or not 1<=limit<=100 or offset<0:
        raise PlatformValidationError('Invalid filter',code='INVALID_PROPOSAL_FILTER')
    rows=repo.session.execute(text('''
      SELECT p.id,p.scope_kind,p.scope_id,p.status,p.revision,p.rule_id,p.created_at,p.updated_at,
             p.submitted_at,p.decided_at,p.decision_comment,r.version AS rule_version,r.status AS rule_status,
             r.authority_ref,(SELECT count(*) FROM house_numbering.proposal_items i
                 WHERE i.organization_id=p.organization_id AND i.project_id=p.project_id AND i.proposal_id=p.id) item_count
      FROM house_numbering.proposals p JOIN house_numbering.rules r
        ON (r.organization_id,r.project_id,r.id)=(p.organization_id,p.project_id,p.rule_id)
      WHERE p.organization_id=:org AND p.project_id=:project AND (CAST(:status AS text) IS NULL OR p.status=CAST(:status AS text))
      ORDER BY p.updated_at DESC,p.id LIMIT :limit OFFSET :offset
    '''),{'org':repo.org_id,'project':repo.project_id,'status':status,'limit':limit+1,'offset':offset}).mappings().all()
    return {'items':[dict(x) for x in rows[:limit]],'next_offset':offset+limit if len(rows)>limit else None}

def packet_rows(repo:HouseNumberingRepository,pid:UUID)->tuple[dict[str,Any],list[dict[str,Any]]]:
    repo.prepare('read')
    proposal,_=repo.proposal(pid)
    rows=repo.session.execute(text('''
      SELECT i.building_id,i.proposed_number,n.number_text,n.kind AS number_kind,n.retired_at,
             b.route_id,b.parcel_ref,b.state AS building_state,b.linked_source_id,b.linked_feature_id,
             r.name AS route_name,r.prefix,a.name AS area_name
      FROM house_numbering.proposal_items i
      JOIN house_numbering.buildings b ON (b.organization_id,b.project_id,b.id)=(i.organization_id,i.project_id,i.building_id)
      LEFT JOIN house_numbering.routes r ON (r.organization_id,r.project_id,r.id)=(b.organization_id,b.project_id,b.route_id)
      LEFT JOIN house_numbering.areas a ON (a.organization_id,a.project_id,a.id)=(b.organization_id,b.project_id,b.area_id)
      LEFT JOIN house_numbering.address_numbers n ON (n.organization_id,n.project_id,n.id)=(i.organization_id,i.project_id,i.source_number_id)
      WHERE i.organization_id=:org AND i.project_id=:project AND i.proposal_id=:pid
      ORDER BY r.name,i.proposed_number,i.id
    '''),{'org':repo.org_id,'project':repo.project_id,'pid':pid}).mappings().all()
    return dict(proposal),[dict(r) for r in rows]


def put(draw:ImageDraw.ImageDraw,value:Any,x:int,y:int,max_width:int,size:int=24,bold:bool=False):
    font=_font(size,bold=bold);s=str(value)
    while s and draw.textbbox((0,0),s,font=font)[2]>max_width:s=s[:-1]
    if s!=str(value):s=(s[:-1]+'…') if len(s)>1 else '…'
    draw.text((x,y),s,fill='black',font=font)

def page(title:str,note:str)->tuple[Image.Image,ImageDraw.ImageDraw]:
    im=Image.new('RGB',(WIDTH,HEIGHT),'white');draw=ImageDraw.Draw(im)
    draw.rectangle((55,55,1185,1700),outline='#94A3B8',width=2)
    put(draw,note,80,82,1070,22,True);put(draw,title,80,132,1070,32,True)
    draw.line((80,186,1160,186),fill='black',width=2)
    return im,draw

def pdf(pages:list[Image.Image])->bytes:
    out=io.BytesIO();pages[0].save(out,'PDF',save_all=True,append_images=pages[1:],resolution=150)
    return out.getvalue()

def appendix_pages(p:dict[str,Any],rows:list[dict[str,Any]])->list[Image.Image]:
    pages=[]
    for start in range(0,len(rows),27):
        im,d=page('DANH SÁCH SỐ NHÀ KÈM PHƯƠNG ÁN','PHỤ LỤC – TRÍCH XUẤT DỮ LIỆU NỘI BỘ')
        put(d,'Phương án: '+str(p['id']),90,215,1060,19)
        for x,v in ((90,'STT'),(155,'Số nhà'),(365,'Đường/hẻm'),(800,'Thửa/Mã công trình')):put(d,v,x,273,340,21,True)
        d.line((85,315,1155,315),fill='#334155',width=2)
        for i,row in enumerate(rows[start:start+27]):
            y=334+i*47
            put(d,start+i+1,91,y,55,20)
            put(d,row.get('number_text') or row['proposed_number'],155,y,190,20,True)
            put(d,row.get('route_name') or 'Chưa có',365,y,423,20)
            put(d,row.get('parcel_ref') or row['building_id'],800,y,342,19)
        put(d,f'Trang phụ lục {len(pages)+1} · Tổng {len(rows)} hồ sơ',90,1650,1040,18)
        pages.append(im)
    return pages

def decision_draft(p:dict[str,Any],rows:list[dict[str,Any]])->bytes:
    im,d=page('DỰ THẢO QUYẾT ĐỊNH CẤP SỐ NHÀ', 'DỰ THẢO – CHƯA KÝ – KHÔNG CÓ GIÁ TRỊ PHÁP LÝ')
    lines=[
      'Cơ quan có thẩm quyền: [GHI TÊN CƠ QUAN BAN HÀNH]',
      'Số quyết định: [CHƯA CẤP SỐ VĂN BẢN]',
      'Ngày soạn mẫu: '+date.today().strftime('%d/%m/%Y'),
      'Căn cứ quy tắc: '+str(p.get('authority_ref') or '[CHƯA BỔ SUNG]'),
      'Căn cứ hồ sơ khảo sát, xác minh và đối chiếu số nhà.',
      'Điều 1. Dự thảo phê duyệt danh sách số nhà theo bảng kê kèm theo.',
      f'Số lượng công trình: {len(rows)}.',
      f'Trạng thái quy trình nội bộ: {p["status"].upper()}.',
      'Điều 2. Rà soát điều kiện pháp lý, hồ sơ và các bên liên quan.',
      'Điều 3. Hiệu lực theo quyết định được ký và ban hành hợp lệ.',
      'Nơi nhận: [BỔ SUNG]',
      'Người có thẩm quyền ký: [BỔ SUNG]',
      'Chữ ký, dấu: [CHƯA CÓ]',
      '',
      'Bảng kê chi tiết xuất Excel/PDF riêng từ phương án này.',
      'Đây là mẫu trình ký, KHÔNG phải quyết định hành chính đã ban hành.',
    ]
    for i,v in enumerate(lines):put(d,v,92,235+i*78,1020,23)
    return pdf([im])

def xlsx(rows:list[list[Any]])->bytes:
    sheet='<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>'
    for i,values in enumerate(rows,1):
        sheet+=f'<row r="{i}">'+''.join(_cell(f'{_col_name(j)}{i}',v) for j,v in enumerate(values,1))+'</row>'
    sheet+='</sheetData></worksheet>'
    content_types='''<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>'''
    rels='''<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>'''
    book='''<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Danh sach so nha" sheetId="1" r:id="rId1"/></sheets></workbook>'''
    bookrels='''<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>'''
    out=io.BytesIO()
    with zipfile.ZipFile(out,'w',zipfile.ZIP_DEFLATED) as z:
        for name,data in (('[Content_Types].xml',content_types),('_rels/.rels',rels),('xl/workbook.xml',book),('xl/_rels/workbook.xml.rels',bookrels),('xl/worksheets/sheet1.xml',sheet)):
            z.writestr(name,'<?xml version="1.0" encoding="UTF-8"?>'+data)
    return out.getvalue()

def register_xlsx(p:dict[str,Any],items:list[dict[str,Any]])->bytes:
    title='DANH SÁCH SỐ NHÀ ĐÃ DUYỆT NỘI BỘ' if p['status']=='approved' else 'DANH SÁCH SỐ NHÀ DỰ THẢO'
    matrix=[[title],['Mã phương án',str(p['id'])],['Trạng thái',p['status']],['Căn cứ',p.get('authority_ref') or 'Chưa xác định'],[],
            ['STT','Building ID','Số nhà','Tên tuyến/hẻm','Tiền tố','Thửa','Khu vực','Trạng thái số','Đã nghỉ hiệu lực?']]
    for i,r in enumerate(items,1):
        matrix.append([i,str(r['building_id']),r.get('number_text') or r['proposed_number'],r.get('route_name') or '',
          r.get('prefix') or '',r.get('parcel_ref') or '',r.get('area_name') or '',r.get('number_kind') or '',bool(r.get('retired_at'))])
    return xlsx(matrix)

def qr_house_link(*, org:UUID, project:UUID, source:UUID, feature:str)->str:
    """Canonical /app URL; never use the unsupported /app/projects/... path."""
    if not feature or len(feature)>128:
        raise PlatformValidationError('Invalid GIS feature id',code='INVALID_FEATURE_ID')
    return 'https://openvietgis.cloud/app?' + urlencode({
        'hn_org':str(org),'hn_project':str(project),
        'hn_source':str(source),'hn_feature':feature,
    })

def plates_pdf(p:dict[str,Any],rows:list[dict[str,Any]],org:UUID,project:UUID,
               offset:int=0,limit:int=24)->bytes:
    if p['status']!='approved' or p['rule_status']!='authoritative' or not p.get('authority_ref'):
        raise PlatformConflictError('Approved proposal and authoritative rule are required',code='APPROVAL_REQUIRED')
    if offset<0 or not 1<=limit<=100:
        raise PlatformValidationError('Print at most 100 QR plates',code='INVALID_PLATE_BATCH')
    chosen=rows[offset:offset+limit]
    if not chosen:raise PlatformValidationError('No houses in this page',code='EMPTY_PLATE_BATCH')
    if any(x.get('number_kind')!='approved' or x.get('retired_at') is not None or x.get('building_state')!='active'
           or x.get('number_text')!=x.get('proposed_number') for x in chosen):
        raise PlatformConflictError('An approved number was changed or retired',code='STALE_APPROVAL')
    pages=[]
    for begin in range(0,len(chosen),6):
        im,d=page('MẪU BIỂN QR SỐ NHÀ', 'TRA CỨU NỘI BỘ – KHÔNG THAY THẾ BIỂN CHÍNH THỨC')
        for i,row in enumerate(chosen[begin:begin+6]):
            x=80+(i%2)*550;y=234+(i//2)*472
            d.rounded_rectangle((x,y,x+530,y+445),radius=12,outline='#334155',width=3)
            put(d,row['number_text'],x+28,y+20,450,70,True)
            put(d,row.get('route_name') or '',x+28,y+117,460,23,True)
            href=qr_house_link(org=org,project=project,source=row['linked_source_id'],
                               feature=str(row['linked_feature_id']))
            qr=qrcode.QRCode(error_correction=qrcode.constants.ERROR_CORRECT_M,box_size=6,border=2)
            qr.add_data(href);qr.make(fit=True)
            qr_img=qr.make_image(fill_color='black',back_color='white').convert('RGB')
            qr_img.thumbnail((245,245),Image.Resampling.NEAREST)
            im.paste(qr_img,(x+150,y+155))
            put(d,'Quét QR → đăng nhập để xem trên GIS',x+25,y+407,490,18)
        pages.append(im)
    return pdf(pages)

class HouseNumberingProposalPacket:
    def __init__(self,session:Session,context:AuthorizationContext):
        self.repo=HouseNumberingRepository(session,context)

    def list(self,status:str|None=None,limit:int=100,offset:int=0)->dict[str,Any]:
        return list_proposals(self.repo,status,limit,offset)

    def download(self,pid:UUID,kind:str,offset:int=0,limit:int=24)->tuple[str,str,bytes]:
        proposal,rows=packet_rows(self.repo,pid)
        if not rows:raise PlatformValidationError('Proposal has no items',code='EMPTY_PROPOSAL')
        if kind=='decision':return 'application/pdf','decision-draft.pdf',decision_draft(proposal,rows)
        if kind=='appendix':
            if offset<0 or not 1<=limit<=100:raise PlatformValidationError('Invalid appendix batch',code='INVALID_APPENDIX_BATCH')
            subset=rows[offset:offset+limit]
            if not subset:raise PlatformValidationError('No houses on this appendix page',code='EMPTY_APPENDIX_BATCH')
            return 'application/pdf','numbering-appendix.pdf',pdf(appendix_pages(proposal,subset))
        if kind=='register':return 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet','numbering-register.xlsx',register_xlsx(proposal,rows)
        if kind=='plates':return 'application/pdf','qr-plates.pdf',plates_pdf(proposal,rows,self.repo.org_id,self.repo.project_id,offset,limit)
        raise PlatformValidationError('Unsupported output',code='INVALID_PACKET_KIND')
