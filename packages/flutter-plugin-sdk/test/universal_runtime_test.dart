import 'dart:convert';
import 'dart:io';
import 'package:test/test.dart';
import 'package:openvietgis_plugin_sdk/src/universal_runtime.dart';
import 'package:openvietgis_plugin_sdk/src/universal_session.dart';
void main(){
 final vectors=jsonDecode(File('../../fixtures/spec075/runtime-wire-vectors.json').readAsStringSync()) as Map<String,dynamic>;
 for(final c in vectors['valid'] as List){
  test('wire '+c['name'],(){
   final r=UniversalRuntime.parse(c['runtime'],expectedManifest:c['manifest']);
   expect(r.invocation(c['action'],Map<String,dynamic>.from(c['input']),c['request_id']),c['expected']);
   expect(()=>r.invocation('unknown',{},c['request_id']),throwsStateError);
   expect(()=>r.invocation(c['action'],{'extra':'x'},c['request_id']),throwsStateError);
  });
 }
 for(final c in vectors['invalid'] as List){
  test('reject '+c['name'],()=>expect(()=>UniversalRuntime.parse(c['runtime'],expectedManifest:c['manifest']),throwsStateError));
 }
 test('session binds immutable authorization snapshot',()async{
  final c=(vectors['valid'] as List).first;
  final r=UniversalRuntime.parse(c['runtime'],expectedManifest:c['manifest']);
  UniversalScope make(String grant)=>UniversalScope(issuer:'fixture',organization:'o',project:'p',subject:'u',session:'s',
    manifest:r.manifest,release:r.release,lock:r.lock,activationRevision:r.activationRevision,catalogEpoch:r.catalogEpoch,
    grantRevision:grant,qualified:true,online:true,granted:true);
  var scope=make(r.grantRevision);
  final session=UniversalSession.fromRuntime(runtime:r,current:()=>scope,
    send:(s,body)async{expect(body,c['expected']);scope=make('13');return {'ok':true};},newRequestId:()=>c['request_id']);
  await expectLater(session.invoke(c['action'],{}),throwsStateError);
  expect(()=>UniversalSession.fromRuntime(runtime:r,current:()=>scope,send:(s,b)async=>{},newRequestId:()=>c['request_id']),throwsStateError);
 });
 test('processing document is parseable but native execution remains denied',()async{
  final c=(vectors['valid'] as List).last;
  final r=UniversalRuntime.parse(c['runtime'],expectedManifest:c['manifest']);
  final scope=UniversalScope(issuer:'fixture',organization:'o',project:'p',subject:'u',session:'s',
    manifest:r.manifest,release:r.release,lock:r.lock,activationRevision:r.activationRevision,catalogEpoch:r.catalogEpoch,
    grantRevision:r.grantRevision,qualified:true,online:true,granted:true);
  final session=UniversalSession.fromRuntime(runtime:r,current:()=>scope,
    send:(s,b)async{fail('native processing must not reach transport');},newRequestId:()=>c['request_id']);
  await expectLater(session.invoke(c['action'],Map<String,String>.from(c['input'])),throwsStateError);
 });

}
