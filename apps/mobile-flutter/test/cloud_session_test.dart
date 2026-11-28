import 'dart:async';
import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:openvietgis_sdk/openvietgis_sdk.dart';
import 'package:openvietgis_mobile/cloud_session.dart';

const uid='12345678-1234-1234-1234-123456789abc';
const valid='{"status":"ok","runtime":"rust","service":"openvietgis-api-rust","version":"fixture","database":{"status":"ready"}}';
http.Response health()=>http.Response(valid,200,headers:{'content-type':'application/json'});
MobileCredentials credential(String origin)=>MobileCredentials(
  scope:Scope(origin:origin,issuer:'fixture',subject:uid,organization:uid,project:uid,session:uid),
  access:'ovg_ma_${List.filled(43,'a').join()}',refresh:'ovg_mr_${List.filled(43,'b').join()}',
  lifetime:const Duration(minutes:10));
void main(){
 test('public probe cannot authenticate, reviewed identity binds exact origin',()async{
   final s=CloudSession(clientFactory:()=>MockClient((r)async{
     expect(r.followRedirects,false);
     if(r.url.path=='/api/v1/health'){expect(r.headers.containsKey('authorization'),false);return health();}
     expect(r.headers['authorization'],startsWith('Bearer ovg_ma_'));
     return http.Response('{}',200,headers:{'content-type':'application/json'});
   }));
   await s.check('https://gis.example.invalid/');
   expect(s.health?.databaseReady,true);expect(s.authenticated,false);
   expect(()=>s.managed,throwsStateError);
   expect(()=>s.attachReviewedIdentity(credential('https://other.example.invalid'),(_)async=>throw StateError('no')),throwsStateError);
   s.attachReviewedIdentity(credential('https://gis.example.invalid'),(_)async=>throw StateError('no'));
   expect(await s.managed.delta(0),{});s.disconnect();expect(s.authenticated,false);s.dispose();
 });
 test('invalid origin and redirected or malformed health fail closed',()async{
   for(final input in ['http://gis.example.invalid','https://user@gis.example.invalid','https://gis.example.invalid/path','https://gis.example.invalid?x=1']){
     expect(()=>cloudOrigin(input),throwsArgumentError);
   }
   for(final response in [http.Response('',302),http.Response('{}',200,headers:{'content-type':'application/json'})]){
     final s=CloudSession(clientFactory:()=>MockClient((_)async=>response));
     await s.check('https://gis.example.invalid');expect(s.health,isNull);expect(s.error,'CONNECTION_FAILED');s.dispose();
   }
 });
 test('disconnect invalidates in-flight health',()async{
   final pending=Completer<http.Response>();
   final s=CloudSession(clientFactory:()=>MockClient((_)=>pending.future));
   final task=s.check('https://gis.example.invalid');
   s.disconnect();pending.complete(health());await task;expect(s.health,isNull);expect(s.authenticated,false);s.dispose();
 });
}
