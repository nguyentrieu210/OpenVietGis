import 'dart:async';
import 'dart:io';
import 'dart:math';
import 'dart:typed_data';

typedef TileRead = Future<Uint8List> Function(String layer,int z,int x,int y);

/// Explicit opt-in renderer bridge. Only IPv4 loopback; a fresh 256-bit
/// capability per route, no arbitrary paths, no CORS, no persistent HTTP cache.
class LocalTileServer {
 static const maxBytes=4*1024*1024;
 static const maxInFlight=4;
 final HttpServer _server;
 final Set<String> _layers;
 final TileRead _read;
 final String _token;
 final Set<Future<void>> _pending={};
 bool _closed=false;
 LocalTileServer._(this._server,this._layers,this._read,this._token);
 static Future<LocalTileServer> start(Set<String> layers,TileRead read)async{
  final uuid=RegExp(r'^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$');
  if(layers.isEmpty||layers.length>8||layers.any((l)=>!uuid.hasMatch(l)))throw ArgumentError('INVALID_LAYER_SET');
  final rng=Random.secure();
  final token=List.generate(32,(_)=>rng.nextInt(256).toRadixString(16).padLeft(2,'0')).join();
  final s=await HttpServer.bind(InternetAddress.loopbackIPv4,0,shared:false);
  s.idleTimeout=const Duration(seconds:5);
  final result=LocalTileServer._(s,Set.unmodifiable(layers),read,token);
  s.listen(result._accept,onError:(Object _){unawaited(result.close());});
  return result;
 }
 String template(String layer){
  if(_closed||!_layers.contains(layer))throw StateError('INVALID_TILE_LEASE');
  return 'http://127.0.0.1:${_server.port}/$_token/$layer/{z}/{x}/{y}.pbf';
 }
 void _accept(HttpRequest r){
  if(_closed||_pending.length>=maxInFlight){unawaited(_reject(r,503));return;}
  late Future<void> task;
  task=_serve(r).whenComplete(()=>_pending.remove(task));_pending.add(task);
 }
 Future<void> _reject(HttpRequest r,int status)async{
  try{r.response.statusCode=status;r.response.persistentConnection=false;r.response.headers.set('Cache-Control','no-store');await r.response.close();}catch(_){}
 }
 Future<void> _serve(HttpRequest r)async{
  try{
   final p=r.uri.pathSegments;
   final host=r.headers.value(HttpHeaders.hostHeader);
   if(_closed||r.connectionInfo?.remoteAddress.address!='127.0.0.1'||host!='127.0.0.1:${_server.port}'||r.method!='GET'||r.headers.value('origin')!=null||r.contentLength>0||r.headers.chunkedTransferEncoding||r.uri.hasQuery||p.length!=5||p[0]!=_token||!_layers.contains(p[1])||!p[4].endsWith('.pbf')){
    await _reject(r,404);return;
   }
   int? n(String s){if(s.length>8||!RegExp(r'^(0|[1-9][0-9]*)$').hasMatch(s))return null;return int.tryParse(s);}
   final z=n(p[2]),x=n(p[3]),y=n(p[4].substring(0,p[4].length-4));
   if(z==null||x==null||y==null||z>22||x>=(1<<z)||y>=(1<<z)){await _reject(r,400);return;}
   final data=await _read(p[1],z,x,y);
   if(_closed){await _reject(r,410);return;}
   if(data.isEmpty||data.length>maxBytes){await _reject(r,422);return;}
   final res=r.response;
   res.headers.contentType=ContentType('application','vnd.mapbox-vector-tile');
   res.headers.set('Cache-Control','private, no-store');
   res.headers.set('X-Content-Type-Options','nosniff');
   res.persistentConnection=false;res.contentLength=data.length;res.add(data);await res.close();
  }catch(_){await _reject(r,409);}
 }
 Future<void> close()async{
  if(_closed)return;_closed=true;
  await _server.close(force:true);
  // Do not pretend that an outstanding Rust call has been cancelled. Keep
  // the route owner alive until each admitted read has actually returned.
  await Future.wait(_pending.toList());
 }
}
