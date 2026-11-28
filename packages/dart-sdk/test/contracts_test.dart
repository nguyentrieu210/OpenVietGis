import 'dart:async';
import 'dart:convert';
import 'package:test/test.dart';
import 'package:http/http.dart' as http;
import 'package:openvietgis_sdk/openvietgis_sdk.dart';
const uid='12345678-1234-1234-1234-123456789abc';
Scope scope([String origin='https://gis.example.invalid'])=>Scope(origin:origin,issuer:'instance',subject:uid,organization:uid,project:uid,session:uid);
class FixtureTransport implements HumanTransport{
 @override final Scope scope;http.Request? request;int status=200;bool closed=false;
 FixtureTransport(this.scope);
 @override Future<http.StreamedResponse> send(http.Request req)async{
  request=req;return http.StreamedResponse(Stream.value(utf8.encode('{}')),status,headers:{'content-type':'application/json'});
 }
 @override void close(){closed=true;}
}
void main(){
 test('PKCE RFC7636 vector and exact callback',(){
  expect(s256('dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk'),'E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM');
  final a=PkceAttempt(issuer:Uri.parse('https://gis.example.invalid'),authorizeEndpoint:Uri.parse('https://gis.example.invalid/authorize'),redirect:Uri.parse('https://mobile.example.invalid/callback'),clientId:'fixture');
  final callback=a.redirect.replace(queryParameters:{'code':'fixture','state':a.state,'iss':a.issuer.toString()});
  expect(a.consume(callback),'fixture');expect(()=>a.consume(callback),throwsStateError);
 });
 test('scope, routes, no redirects or retry, revoke',()async{
  final s=scope(),t=FixtureTransport(scope());final c=ManagedClient(s,t);
  await c.delta(10);expect(t.request!.url.path,'/api/v1/organizations/$uid/projects/$uid/integration-hub/managed/delta');
  expect(t.request!.url.queryParameters['cursor'],'10');expect(t.request!.followRedirects,false);
  await c.receipt('op-1');expect(t.request!.url.path.endsWith('/commits/op-1'),true);
  expect(()=>ManagedClient(s,FixtureTransport(scope('https://other.example.invalid'))),throwsArgumentError);
  t.status=302;await expectLater(c.delta(10),throwsA(isA<ProtocolError>()));
  t.status=403;await expectLater(c.delta(10),throwsA(isA<ProtocolError>()));expect(t.closed,true);
  await expectLater(c.delta(10),throwsStateError);
 });
 test('reuse marketplace and project registry with explicit consent',()async{
  final s=scope(),t=FixtureTransport(scope());final c=ManagedClient(s,t);
  await c.marketplaceCatalog();expect(t.request!.url.path,'/api/v1/marketplace/catalog');
  await c.installedManifests();expect(t.request!.url.path,'/api/v1/organizations/$uid/extensions/manifests');
  await c.projectActivations();expect(t.request!.url.path,'/api/v1/organizations/$uid/projects/$uid/extensions/activations');
  expect(()=>c.installReviewedRelease('demo','partner_demo_test','1.0.0',consent:false),throwsArgumentError);
  await c.installReviewedRelease('demo','partner_demo_test','1.0.0',consent:true);
  expect(t.request!.url.path,'/api/v1/organizations/$uid/marketplace/install/demo/partner_demo_test/1.0.0');
  await c.activateManifest(uid,consent:true);expect(jsonDecode(t.request!.body),{'manifest_id':uid});
  await c.disableExtension('partner_demo_test',consent:true);expect(t.request!.url.path.endsWith('/partner_demo_test/disable'),true);
 });
 test('refresh single flight and fail closed',()async{
  final gate=RefreshGate<int>();var calls=0;final completer=Completer<int>();
  final a=gate.run((){calls++;return completer.future;});final b=gate.run(()async{calls++;return 2;});
  completer.complete(1);expect(await a,1);expect(await b,1);expect(calls,1);
  await expectLater(gate.run(()async=>throw StateError('unknown')),throwsStateError);
  await expectLater(gate.run(()async=>3),throwsStateError);
 });
 test('SSE revocation halts and bounded parser',()async{
  final chunks=Stream.value(utf8.encode('event: heartbeat\ndata: {}\n\nevent: session_revoked\ndata: {}\n\nevent: change\ndata: {}\n\n'));
  final events=await hints(chunks,active:()=>true).toList();
  expect(events.map((e)=>e.event),['heartbeat','session_revoked']);
  await expectLater(hints(Stream.value(List.filled(8193,65)),active:()=>true).toList(),throwsStateError);
 });
}
