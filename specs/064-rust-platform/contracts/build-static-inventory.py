import ast, json, subprocess
from pathlib import Path
ROOT=Path(__file__).resolve().parents[3]
BASE='3e78ac142acfc050ec22d30da5c44ad0e6b8459a'
mapping={"auth":"E01","bootstrap":"E01","me":"E01","organizations":"E01","workspaces":"E01","projects":"E01","grants":"E01","gis_access":"E01","invitations":"E01","service_principals":"E01","audit":"E01","authorization":"E01","tenant_access":"E01","security":"E01","tenant_serialization":"E01","gis_access_guard":"E01","spatial":"E02","maps":"E02","map_viewport":"E02","editing":"E03","queries":"E04","tilesets":"E05","cartography":"E06","layout_designer":"E06","transfers":"E07","processing":"E08","processing_models":"E08","professional":"E08","raster":"E09","terrain":"E09","cad":"E10","cad_documents":"E10","model_vault":"E10","saved_scenes":"E10","v8i_bridge":"E10","collaboration":"E11","team_chat":"E11","org_chat":"E11","team_locks":"E11","team_presence":"E11","team_realtime":"E11","managed_sync":"E11","external_sources":"E12","integration_hub":"E12","integration_bindings":"E12","external_provider_transport":"E12","field":"E13","extensions":"E14","official_modules":"E14","marketplace":"E14","marketplace_developers":"E14","partner_platform":"E14","health":"E18","ops":"E18"}
paths=subprocess.check_output(['git','ls-tree','-r','--name-only',BASE],cwd=ROOT,text=True).splitlines()
def read(path):
 return subprocess.check_output(['git','show',f'{BASE}:{path}'],cwd=ROOT,text=True)
prefixes=('apps/api/','apps/web/','apps/web-next/','apps/mcp/','modules/','integrations/','packages/','infra/','tests/','tools/')
source_ext={'.py','.ts','.tsx','.js','.jsx','.rs','.sql','.json','.yaml','.yml','.toml','.ps1','.sh','.cs','.cpp','.h','.html','.css'}
records=[]; routes=[]; parse_errors=[]
for p in paths:
 if not p.startswith(prefixes): continue
 if Path(p).suffix.lower() not in source_ext and Path(p).name not in {'Dockerfile','Makefile'}: continue
 if '/fixtures/' in p or '/snapshots/' in p: continue
 e='UNCLASSIFIED'; role='source'
 if '/tests/' in p or p.startswith('tests/') or '.test.' in p or '.spec.' in p or Path(p).name.startswith('test_'): role='test'
 if '/migrations/' in p: role='migration'
 if p.startswith('modules/house_numbering/') or p.startswith('modules/land_asset/'): e='E15'
 elif p.startswith('apps/mcp/'): e='E16'
 elif p.startswith('integrations/microstation'): e='E10'
 elif p.startswith('integrations/qgis'): e='E12'
 elif p.startswith('apps/web'): e='E17'
 elif p.startswith('infra/') or p.startswith('tools/'): e='E18'
 elif p.startswith('packages/plugin-sdk'): e='E14'
 elif p.startswith('packages/client-sdk'): e='E17'
 elif p.startswith('packages/spatial-workspace'): e='E02'
 elif p.startswith('apps/api/'):
  if '/adapters/http/' in p: e=mapping.get(Path(p).stem,'UNCLASSIFIED')
  elif '/domains/' in p:
   domain=p.split('/domains/')[1].split('/')[0]
   e={'identity':'E01','tenancy':'E01','authorization':'E01','audit':'E01','bootstrap':'E01','spatial':'E02','maps':'E05','editing':'E03','queries':'E04','cartography':'E06','transfers':'E07','processing':'E08','cad':'E10','collaboration':'E11','integration_hub':'E12','external_sources':'E12','field':'E13','extensions':'E14','ops':'E18'}.get(domain,'UNCLASSIFIED')
 if e=='UNCLASSIFIED' and (role=='test' or role=='migration'): e='CROSS_CUTTING_REVIEW'
 if e=='UNCLASSIFIED' and '/adapters/cad/' in p: e='E10'
 if e=='UNCLASSIFIED' and '/adapters/oidc/' in p: e='E01'
 if e=='UNCLASSIFIED' and '/security/' in p: e='E01'
 if e=='UNCLASSIFIED' and '/adapters/rendering/' in p: e='E05'
 if e=='UNCLASSIFIED' and '/adapters/formats/' in p:
  e='E09' if any(x in p for x in ('raster','terrain')) else ('E13' if 'offline' in p else 'E07')
 if e=='UNCLASSIFIED' and '/adapters/persistence/' in p:
  key=Path(p).stem.removesuffix('_repository')
  e={'audit':'E01','authorization':'E01','identity':'E01','tenancy':'E01','rls':'E01','gis_access':'E01','scope_helper':'E01','cad':'E10','cartography':'E06','collaboration':'E11','managed_events':'E11','editing':'E03','extensions':'E14','external_sources':'E12','field':'E13','map':'E05','map_viewport':'E05','ops':'E18','processing':'E08','query':'E04','spatial':'E02','spatial_transform':'E02','spatial_fixtures':'E02','web_transform_guard':'E02','transfer':'E07'}.get(key,'CROSS_CUTTING_REVIEW')
 if e=='UNCLASSIFIED' and p.startswith('apps/api/'): e='CROSS_CUTTING_REVIEW'
 records.append({'path':p,'engine_candidate':e,'role':role,'status':'needs_review'})
 if Path(p).suffix=='.py' and role=='source' and (p.startswith('apps/api/openvietgis_api/') or p.startswith('modules/')):
  try: tree=ast.parse(read(p))
  except (SyntaxError, UnicodeError) as err:
   parse_errors.append({'path':p,'error':str(err)}); continue
  for node in ast.walk(tree):
   if not isinstance(node,(ast.FunctionDef,ast.AsyncFunctionDef)): continue
   for d in node.decorator_list:
    if not isinstance(d,ast.Call) or not isinstance(d.func,ast.Attribute): continue
    if d.func.attr not in {'get','post','put','patch','delete','head','options','websocket','api_route','route'}: continue
    path=ast.unparse(d.args[0]) if d.args else next((ast.unparse(k.value) for k in d.keywords if k.arg=='path'),'UNKNOWN')
    routes.append({'file':p,'line':node.lineno,'handler':node.name,'decorator':ast.unparse(d.func),'path_expression':path,'engine_candidate':e,'status':'needs_runtime_contract_verification'})
out={'baseline':BASE,'notes':'Static tracked-file and decorator candidates only, not mounted OpenAPI routes or feature parity. Dynamic registrations, prefixes, methods and runtime behavior require verification. UNCLASSIFIED and CROSS_CUTTING_REVIEW require explicit triage; file classifications are initial candidates.','files':records,'route_candidates':routes,'parse_errors':parse_errors}
dst=Path(__file__).with_name('static-inventory.json')
dst.write_text(json.dumps(out,ensure_ascii=False,indent=2)+'\n')
from collections import Counter
print('files',len(records),'route_candidates',len(routes),'parse_errors',len(parse_errors),'unclassified',sum(r['engine_candidate']=='UNCLASSIFIED' for r in records))
print('roles',dict(Counter(r['role'] for r in records)))
print('route engines',dict(Counter(r['engine_candidate'] for r in routes)))
