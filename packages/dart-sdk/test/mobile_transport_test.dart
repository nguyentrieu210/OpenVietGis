import 'dart:async';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:test/test.dart';
import 'package:openvietgis_sdk/openvietgis_sdk.dart';

const uid='12345678-1234-1234-1234-123456789abc';
Scope scope({String project=uid})=>Scope(origin:'https://gis.example.invalid',issuer:'instance',subject:uid,organization:uid,project:project,session:uid);
MobileCredentials credentials(Scope s,{String suffix='a',int seconds=600})=>MobileCredentials(scope:s,access:'ovg_ma_${List.filled(43,suffix).join()}',refresh:'ovg_mr_${List.filled(43,suffix).join()}',lifetime:Duration(seconds:seconds));

void main(){
 test('same scope only, no cookies or redirects, no automatic retries',()async{
  final s=scope();var sent=0;var status=200;
  final t=ScopedMobileTransport(scope:s,credentials:credentials(s),rotate:(_)async=>throw StateError('unexpected'),client:MockClient((r)async{
   sent++;expect(r.headers['authorization'],credentials(s).access.replaceFirst('ovg_ma_','Bearer ovg_ma_'));expect(r.followRedirects,false);
   return http.Response('{}',status);
  }));
  final path=s.managed(['delta']);
  await t.send(http.Request('GET',path));expect(sent,1);
  await expectLater(t.send(http.Request('GET',Uri.parse('https://evil.example.invalid/api/v1/geocode'))),throwsStateError);
  await expectLater(t.send(http.Request('GET',path.replace(path:path.path.replaceFirst(s.project,'another')))),throwsStateError);
  await expectLater(t.send(http.Request('GET',path)..headers['cookie']='bad'),throwsStateError);
  expect(sent,1);
  status=302;await expectLater(t.send(http.Request('GET',path)),throwsA(isA<ProtocolError>()));expect(sent,2);
  status=401;await expectLater(t.send(http.Request('POST',path)),throwsStateError);expect(sent,3);
  await expectLater(t.send(http.Request('GET',path)),throwsStateError);expect(sent,3);
 });
 test('expired credentials rotate single flight',()async{
  final s=scope();var rotations=0;var sent=0;final gate=Completer<MobileCredentials>();
  final t=ScopedMobileTransport(scope:s,credentials:credentials(s,seconds:1),rotate:(_) {rotations++;return gate.future;},client:MockClient((r)async{
   sent++;expect(r.headers['authorization'],'Bearer ${credentials(s,suffix:'b').access}');return http.Response('{}',200);
  }));
  final a=t.send(http.Request('GET',s.managed(['delta']))),b=t.send(http.Request('GET',s.managed(['delta'])));
  gate.complete(credentials(s,suffix:'b'));await Future.wait([a,b]);expect(rotations,1);expect(sent,2);t.close();
 });
 test('revocation during refresh prevents request',()async{
  final s=scope();final gate=Completer<MobileCredentials>();
  final t=ScopedMobileTransport(scope:s,credentials:credentials(s,seconds:1),rotate:(_)=>gate.future,client:MockClient((_)async=>throw StateError('must not send')));
  final pending=t.send(http.Request('GET',s.managed(['delta'])));
  final assertion=expectLater(pending,throwsStateError);t.close();gate.complete(credentials(s,suffix:'b'));await assertion;
 });
 test('invalid credential namespace and rotation scope rejected',()async{
  final s=scope();expect(()=>MobileCredentials(scope:s,access:'service',refresh:'cookie',lifetime:Duration(minutes:1)),throwsArgumentError);
  final t=ScopedMobileTransport(scope:s,credentials:credentials(s,seconds:1),rotate:(_)async=>credentials(scope(project:'22345678-1234-1234-1234-123456789abc'),suffix:'b'),client:MockClient((_)async=>throw StateError('must not send')));
  await expectLater(t.send(http.Request('GET',s.managed(['delta']))),throwsStateError);
  await expectLater(t.send(http.Request('GET',s.managed(['delta']))),throwsStateError);
 });
}
