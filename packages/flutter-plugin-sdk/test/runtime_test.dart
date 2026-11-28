import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'package:test/test.dart';
import 'package:cryptography/cryptography.dart';
import 'package:openvietgis_plugin_sdk/openvietgis_plugin_sdk.dart';

final fixture=jsonDecode(File('test/fixtures/release-v2.json').readAsStringSync()) as Json;
Json clone(Json v)=>jsonDecode(jsonEncode(v)) as Json;
Trust trust({int floor=4,int now=1700000100,Set<String> revoked=const{}})=>Trust(issuer:'fixture-instance',operatorKey:fixture['operator_key'],platform:'android',epochFloor:floor,now:now,hostApi:2,revokedKeys:revoked);
Future<VerifiedRelease> verified({Json? value,Trust? t,Map<String,List<int>> assets=const{}}){
 final v=value??fixture;
 return verifyRelease(envelope:{for(final k in ['release','publisher_signature','review','operator_signature'])k:v[k]},manifest:v['manifest'],manifestId:fixture['release']['manifest_id'],contribution:v['contribution'],assets:assets,trust:t??trust());
}
Future<Json> resign(Json contribution,{Json? release,Map<String,List<int>> assets=const{}})async{
 final v=clone(fixture);v['contribution']=contribution;if(release!=null)v['release']=release;
 v['release']['contribution_digest']=digest(contribution);v['review']['release_digest']=digest(v['release']);
 final algorithm=Ed25519();
 final pub=await algorithm.newKeyPairFromSeed(List.filled(32,7));
 final op=await algorithm.newKeyPairFromSeed(List.filled(32,8));
 v['publisher_signature']=base64Encode((await algorithm.sign(canonical(v['release']),keyPair:pub)).bytes);
 v['operator_signature']=base64Encode((await algorithm.sign(canonical(v['review']),keyPair:op)).bytes);
 return v;
}
Activation activation(VerifiedRelease r,{String project='project-a',bool active=true,bool permitted=true,bool online=true,bool lease=true,int now=1700000100})=>Activation(issuer:r.issuer,organization:'org',project:project,subject:'actor',session:'session',manifestId:r.manifestId,releaseDigest:r.releaseDigest,permissions:permitted?r.permissions:{},capabilities:r.capabilities,active:active,entitled:true,online:online,leaseValid:lease,now:now,epoch:r.epoch);
Command command(CommandHandler handler,{String consent='none'})=>Command(capability:'data.read',permission:'ext.demo.read',mode:'online',consent:consent,input:{'id':'text'},output:{'name':'text'},run:handler);
void main(){
 test('independent golden v2 and separate trust',()async{
  final r=await verified();expect(r.extensionKey,'partner_demo_field');expect(r.releaseDigest,fixture['review']['release_digest']);expect(Runtime.parse(r).nodes.length,2);
  for(final t in [trust(floor:5),trust(now:1700003600),trust(revoked:{'pub1'})]){await expectLater(verified(t:t),throwsStateError);}
  final bad=clone(fixture);bad['contribution']['nodes'][0]['label']='tampered';await expectLater(verified(value:bad),throwsStateError);
  final changed=clone(fixture);changed['release']['version']='1.0.1';await expectLater(verified(value:changed),throwsStateError);
 });
 test('hostile signed graph, unknown capability and asset tamper',()async{
  final c=clone(fixture['contribution']);c['nodes'][0]['kind']='html';final r=await verified(value:await resign(c));expect(()=>Runtime.parse(r),throwsStateError);
  final d=clone(fixture['contribution']);d['nodes'].add(d['nodes'][0]);final r2=await verified(value:await resign(d));expect(()=>Runtime.parse(r2),throwsStateError);
  final e=clone(fixture['release']);e['capabilities'].add('arbitrary.network');final r3=await verified(value:await resign(clone(fixture['contribution']),release:e));expect(()=>Runtime.parse(r3),throwsStateError);
  final asset=clone(fixture['release']);asset['assets']=[{'id':'help','media_type':'text/plain','size':1,'sha256':'ca978112ca1bbdcafac231b39a23dc4da786eff8147c4e72b9807785afee48bb'}];
  final signed=await resign(clone(fixture['contribution']),release:asset);
  expect((await verified(value:signed,assets:{'help':[97]})).extensionKey,isNotEmpty);
  await expectLater(verified(value:signed,assets:{'help':[98]}),throwsStateError);
 });
 test('broker scope, permission, typed arguments, no arbitrary command',()async{
  final r=await verified();var a=activation(r);var calls=0;
  final b=Broker(runtime:Runtime.parse(r),current:()=>a,consent:(_,_)async=>true,commands:{'core.read':command((input,scope,cancel)async{calls++;cancel.check();return {'name':input['id']};})});
  expect(await b.invoke('read',{'id':'A'}),{'name':'A'});expect(calls,1);
  expect(()=>b.invoke('network',{}),throwsStateError);expect(()=>b.invoke('read',{'id':4}),throwsStateError);
  a=activation(r,permitted:false);await expectLater(b.invoke('read',{'id':'B'}),throwsStateError);expect(calls,1);
  a=activation(r,project:'project-b');expect(()=>b.invoke('read',{'id':'B'}),throwsStateError);
  b.close();expect(()=>b.invoke('read',{'id':'A'}),throwsStateError);
 });
 test('bounded queue, revoke, timeout and consent denial',()async{
  final r=await verified();var a=activation(r);final gate=Completer<Json>();
  final b=Broker(runtime:Runtime.parse(r),current:()=>a,consent:(_,_)async=>true,commands:{'core.read':command((_,_,_)=>gate.future)});
  final pending=List.generate(8,(_)=>b.invoke('read',{'id':'A'}));
  final results=pending.map((p)=>expectLater(p,throwsStateError)).toList();
  expect(()=>b.invoke('read',{'id':'A'}),throwsStateError);a=activation(r,active:false);b.close();gate.complete({'name':'A'});await Future.wait(results);
  final c=clone(fixture['contribution']);c['actions'][0]['timeout_ms']=5;
  final t=await verified(value:await resign(c));final slow=Broker(runtime:Runtime.parse(t),current:()=>activation(t),consent:(_,_)async=>true,commands:{'core.read':command((_,_,_)=>Completer<Json>().future)});
  await expectLater(slow.invoke('read',{'id':'A'}),throwsStateError);expect(()=>slow.invoke('read',{'id':'A'}),throwsStateError);
  c['actions'][0]['consent']='explicit';final u=await verified(value:await resign(c));var called=false;
  final denied=Broker(runtime:Runtime.parse(u),current:()=>activation(u),consent:(_,_)async=>false,commands:{'core.read':command((_,_,_)async{called=true;return {'name':'x'};},consent:'explicit')});
  await expectLater(denied.invoke('read',{'id':'A'}),throwsStateError);expect(called,false);
 });
 test('catalog disable does not imply data removal',()async{
  final r=await verified();
  expect(installState(release:r,issuer:r.issuer,activeManifest:r.manifestId,epoch:r.epoch,now:1700000100,compatible:true,revoked:false,entitled:true,permitted:true,enabled:false),InstallState.disabled);
 });
}
