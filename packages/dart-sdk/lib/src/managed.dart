import 'dart:async';
import 'dart:convert';
import 'package:http/http.dart' as http;
import 'scope.dart';

typedef Json=Map<String,dynamic>;
/// Transport must be injected by trusted host AFTER dedicated mobile credential qualification.
/// This library never turns a service token or web cookie into human authority.
abstract interface class HumanTransport {
  Scope get scope;
  Future<http.StreamedResponse> send(http.Request request);
  void close();
}
final class ProtocolError implements Exception {
  final int status;
  final String code;
  const ProtocolError(this.status,this.code);
  @override String toString()=> 'ProtocolError($status,$code)';
}
final class ManagedClient {
  final HumanTransport _transport;
  final Scope scope;
  bool _closed=false;
  ManagedClient(this.scope,this._transport){
    if(!scope.same(_transport.scope))throw ArgumentError('Scope mismatch');
  }
  void close(){_closed=true;_transport.close();}
  void _guard(){if(_closed||!scope.same(_transport.scope))throw StateError('SCOPE_CLOSED');}
  Future<Json> _request(String method,Uri uri,[List<int>? payload])async{
    _guard();
    if(uri.origin!=scope.origin.origin)throw StateError('ORIGIN_MISMATCH');
    final req=http.Request(method,uri)..followRedirects=false..headers['accept']='application/json';
    if(payload!=null){if(payload.length>16*1024*1024)throw StateError('PAYLOAD_LIMIT');req.headers['content-type']='application/json';req.bodyBytes=payload;}
    final res=await _transport.send(req).timeout(const Duration(seconds:30));
    _guard();
    if(res.statusCode>=300&&res.statusCode<400){await res.stream.listen((_){}).cancel();throw const ProtocolError(403,'REDIRECT_DENIED');}
    if(res.statusCode<200||res.statusCode>=300){await res.stream.listen((_){}).cancel();if(res.statusCode==403)close();throw ProtocolError(res.statusCode,'REQUEST_REJECTED');}
    if(!(res.headers['content-type']??'').toLowerCase().startsWith('application/json')){await res.stream.listen((_){}).cancel();throw const ProtocolError(502,'INVALID_CONTENT_TYPE');}
    final data=<int>[];
    await for(final chunk in res.stream.timeout(const Duration(seconds:30))){
      _guard();if(data.length+chunk.length>16*1024*1024)throw StateError('RESPONSE_LIMIT');data.addAll(chunk);
    }
    _guard();final result=jsonDecode(utf8.decode(data));
    if(result is! Map<String,dynamic>)throw const ProtocolError(502,'INVALID_RESPONSE');return result;
  }
  Future<Json> bootstrap(String layer,{int offset=0,int limit=1000}){
    if(!Scope.isUuid(layer)||offset<0||limit<1||limit>5000)throw ArgumentError('Invalid bootstrap');
    return _request('GET',scope.managed(['bootstrap'],{'layer_id':layer,'session_id':scope.session,'offset':'$offset','limit':'$limit'}));
  }
  Future<Json> delta(int cursor,{int limit=200}){
    if(cursor<0||limit<1||limit>500)throw ArgumentError('Invalid delta');
    return _request('GET',scope.managed(['delta'],{'session_id':scope.session,'cursor':'$cursor','limit':'$limit'}));
  }
  Future<Json> receipt(String operationId){
    if(!RegExp(r'^[A-Za-z0-9_.:-]{1,128}$').hasMatch(operationId))throw ArgumentError('Invalid operation ID');
    return _request('GET',scope.managed(['commits',operationId]));
  }
  /// The caller must persist exact bytes and sending state BEFORE invoking this.
  /// Never retries automatically, including HTTP 401/429/5xx.
  Future<Json> commit(List<int> persistedBytes){
    final body=jsonDecode(utf8.decode(persistedBytes));
    if(body is! Map||body['session_id']!=scope.session)throw ArgumentError('Scope mismatch');
    return _request('POST',scope.managed(['commits']),persistedBytes);
  }
  // Registry facade uses the existing Web catalog and activations. Host-only
  // methods: never pass this client to a declarative plugin.
  Uri _registry(List<String> segments)=>scope.origin.replace(pathSegments:['api','v1',...segments]);
  List<String> get _org=>['organizations',scope.organization];
  List<String> get _extensions=>[..._org,'projects',scope.project,'extensions'];
  Future<Json> marketplaceCatalog()=>_request('GET',_registry(['marketplace','catalog']));
  Future<Json> installedManifests()=>_request('GET',_registry([..._org,'extensions','manifests']));
  Future<Json> projectActivations()=>_request('GET',_registry([..._extensions,'activations']));
  Future<Json> installReviewedRelease(String publisher,String extension,String version,{required bool consent}){
    if(!consent||!RegExp(r'^[a-z][a-z0-9_]{1,31}
    _guard();final req=http.Request('GET',scope.managed(['events'],{'session_id':scope.session}))..followRedirects=false..headers['accept']='text/event-stream';
    final response=await _transport.send(req).timeout(const Duration(seconds:30));_guard();
    if(response.statusCode!=200||!(response.headers['content-type']??'').startsWith('text/event-stream')){
      await response.stream.listen((_){}).cancel();if(response.statusCode==403)close();throw ProtocolError(response.statusCode,'EVENTS_REJECTED');
    }
    return response;
  }
}
).hasMatch(publisher)||!RegExp(r'^[a-z][a-z0-9_]{1,127}
    _guard();final req=http.Request('GET',scope.managed(['events'],{'session_id':scope.session}))..followRedirects=false..headers['accept']='text/event-stream';
    final response=await _transport.send(req).timeout(const Duration(seconds:30));_guard();
    if(response.statusCode!=200||!(response.headers['content-type']??'').startsWith('text/event-stream')){
      await response.stream.listen((_){}).cancel();if(response.statusCode==403)close();throw ProtocolError(response.statusCode,'EVENTS_REJECTED');
    }
    return response;
  }
}
).hasMatch(extension)||!extension.startsWith('partner_${publisher}_')||!RegExp(r'^\d+\.\d+\.\d+
    _guard();final req=http.Request('GET',scope.managed(['events'],{'session_id':scope.session}))..followRedirects=false..headers['accept']='text/event-stream';
    final response=await _transport.send(req).timeout(const Duration(seconds:30));_guard();
    if(response.statusCode!=200||!(response.headers['content-type']??'').startsWith('text/event-stream')){
      await response.stream.listen((_){}).cancel();if(response.statusCode==403)close();throw ProtocolError(response.statusCode,'EVENTS_REJECTED');
    }
    return response;
  }
}
).hasMatch(version))throw ArgumentError('Invalid installation');
    return _request('POST',_registry([..._org,'marketplace','install',publisher,extension,version]));
  }
  Future<Json> activateManifest(String manifestId,{required bool consent}){
    if(!consent||!Scope.isUuid(manifestId))throw ArgumentError('Invalid activation');
    return _request('POST',_registry([..._extensions,'activations']),utf8.encode(jsonEncode({'manifest_id':manifestId})));
  }
  Future<Json> disableExtension(String extension,{required bool consent}){
    if(!consent||!RegExp(r'^[a-z][a-z0-9_]{1,127}
    _guard();final req=http.Request('GET',scope.managed(['events'],{'session_id':scope.session}))..followRedirects=false..headers['accept']='text/event-stream';
    final response=await _transport.send(req).timeout(const Duration(seconds:30));_guard();
    if(response.statusCode!=200||!(response.headers['content-type']??'').startsWith('text/event-stream')){
      await response.stream.listen((_){}).cancel();if(response.statusCode==403)close();throw ProtocolError(response.statusCode,'EVENTS_REJECTED');
    }
    return response;
  }
}
).hasMatch(extension))throw ArgumentError('Invalid disable');
    return _request('POST',_registry([..._extensions,extension,'disable']));
  }
  Future<http.StreamedResponse> events()async{
    _guard();final req=http.Request('GET',scope.managed(['events'],{'session_id':scope.session}))..followRedirects=false..headers['accept']='text/event-stream';
    final response=await _transport.send(req).timeout(const Duration(seconds:30));_guard();
    if(response.statusCode!=200||!(response.headers['content-type']??'').startsWith('text/event-stream')){
      await response.stream.listen((_){}).cancel();if(response.statusCode==403)close();throw ProtocolError(response.statusCode,'EVENTS_REJECTED');
    }
    return response;
  }
}
