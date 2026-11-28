import 'dart:convert';
import 'dart:io';
import 'package:test/test.dart';
import 'package:openvietgis_plugin_sdk/src/universal_document.dart';
import 'package:openvietgis_plugin_sdk/src/universal_session.dart';

UniversalDocument fixture(String name,String cap){
  final json=jsonDecode(File('../plugin-sdk/examples/v2/host-$name.json').readAsStringSync());
  final key='partner_fixture_$name';
  return UniversalDocument.parse(json,extensionKey:key,permissions:{'ext.$key.$cap'});
}
UniversalScope scope({bool granted=true,bool qualified=true,String project='p',String grantRevision='1'})=>UniversalScope(
  issuer:'fixture',organization:'o',project:project,subject:'u',session:'s',manifest:'m',
  release:'r',lock:'l',activationRevision:1,catalogEpoch:2,grantRevision:grantRevision,qualified:qualified,online:true,granted:granted);
void main(){
  test('same reference documents as Rust and TypeScript',(){
    expect(fixture('context','host_context').actions.length,1);
    expect(fixture('notice','host_notice').actions.length,1);
    expect(fixture('layers','map_view').actions.length,2);
    expect(fixture('processing','processing_plan').actions.length,1);
  });
  test('reject unknown widget and ungranted permission',(){
    final raw=jsonDecode(File('../plugin-sdk/examples/v2/host-context.json').readAsStringSync()) as Map<String,dynamic>;
    expect(()=>UniversalDocument.parse(raw,extensionKey:'partner_fixture_context',permissions:{}),throwsStateError);
    raw['widgets']=[{'kind':'script','id':'x'}];
    expect(()=>UniversalDocument.parse(raw,extensionKey:'partner_fixture_context',permissions:{'ext.partner_fixture_context.host_context'}),throwsStateError);
  });
  test('rechecks scope and grant before and after call',()async{
    var current=scope();
    final document=fixture('context','host_context');
    final session=UniversalSession(document:document,current:()=>current,
      invoke:(s,id,args,request)async{current=scope(project:'other');return {'result':true};},
      newRequestId:()=> '10000000-0000-0000-0000-000000000001');
    await expectLater(session.invoke('context',{}),throwsStateError);
    current=scope(granted:false);
    await expectLater(session.invoke('context',{}),throwsStateError);
    session.close();
    expect(()=>UniversalSession(document:document,current:()=>scope(qualified:false),
      invoke:(s,id,args,request)async=>{},newRequestId:()=> 'unused'),throwsStateError);
  });
}
