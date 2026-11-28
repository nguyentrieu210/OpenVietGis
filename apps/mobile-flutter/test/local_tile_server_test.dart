import 'dart:async';
import 'dart:io';
import 'dart:typed_data';
import 'package:flutter_test/flutter_test.dart';
import '../lib/local_tile_server.dart';

void main(){
 const layer='11111111-1111-4111-8111-111111111111';
 const other='22222222-2222-4222-8222-222222222222';
 test('tile capability, scope, XYZ validation and shutdown',()async{
  var calls=0;
  final server=await LocalTileServer.start({layer},(l,z,x,y)async{
   calls++;expect(l,layer);expect([z,x,y],[1,0,1]);return Uint8List.fromList([1,2,3]);
  });
  final client=HttpClient();
  final uri=Uri.parse(server.template(layer).replaceAll('{z}','1').replaceAll('{x}','0').replaceAll('{y}','1'));
  Future<int> get(Uri u,{String? origin})async{
   final request=await client.getUrl(u);if(origin!=null)request.headers.set('Origin',origin);
   final res=await request.close();final bytes=await res.expand((x)=>x).toList();
   if(res.statusCode==200){expect(bytes,[1,2,3]);expect(res.headers.value('cache-control'),'private, no-store');}
   return res.statusCode;
  }
  try{
   expect(uri.host,'127.0.0.1');
   expect(await get(uri),200);
   expect(await get(uri.replace(path:uri.path.replaceFirst(uri.pathSegments[0],'invalid'))),404);
   expect(await get(uri.replace(path:uri.path.replaceFirst(layer,other))),404);
   expect(await get(uri.replace(path:uri.path.replaceFirst('/1/0/1.pbf','/1/2/1.pbf'))),400);
   expect(await get(uri,origin:'https://example.invalid'),404);
   expect(calls,1);
  }finally{client.close(force:true);await server.close();}
  expect(()=>server.template(layer),throwsStateError);
 });
 test('close waits for admitted read rather than pretending cancellation',()async{
  final entered=Completer<void>(),release=Completer<Uint8List>();
  final server=await LocalTileServer.start({layer},(l,z,x,y){entered.complete();return release.future;});
  final client=HttpClient();
  final uri=Uri.parse(server.template(layer).replaceAll('{z}','0').replaceAll('{x}','0').replaceAll('{y}','0'));
  final request=await client.getUrl(uri);
  final result=request.close().then<void>((r)=>r.drain<void>(),onError:(Object _){});
  await entered.future;
  var done=false;final closing=server.close().then((_)=>done=true);
  await Future<void>.delayed(const Duration(milliseconds:10));expect(done,false);
  release.complete(Uint8List.fromList([1]));
  await closing;await result;client.close(force:true);
 });
}
