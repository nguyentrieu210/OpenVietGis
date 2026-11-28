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
  Uri _registry(List<String> segments,[Map<String,String>? query])=>scope.origin.replace(pathSegments:['api','v1',...segments],queryParameters:query);
  List<String> get _org=>['organizations',scope.organization];
  List<String> get _project=>[..._org,'projects',scope.project];
  List<String> get _extensions=>[..._project,'extensions'];
  List<String> get _team=>[..._project,'collaboration'];
  List<String> get _processing=>[..._project,'processing'];
  Future<Json> _post(List<String> p,Json body)=>_request('POST',_registry(p),utf8.encode(jsonEncode(body)));
  void _id(String id){if(!Scope.isUuid(id))throw ArgumentError('Invalid UUID');}
  Future<Json> marketplaceCatalog()=>_request('GET',_registry(['marketplace','catalog']));
  Future<Json> installedManifests()=>_request('GET',_registry([..._org,'extensions','manifests']));
  Future<Json> projectActivations()=>_request('GET',_registry([..._extensions,'activations']));
  Future<Json> installReviewedRelease(String publisher,String extension,String version,{required bool consent}){
    if(!consent||!RegExp(r'^[a-z][a-z0-9_]{1,31}$').hasMatch(publisher)||!RegExp(r'^[a-z][a-z0-9_]{1,127}$').hasMatch(extension)||!extension.startsWith('partner_'+publisher+'_')||!RegExp(r'^\d+\.\d+\.\d+$').hasMatch(version))throw ArgumentError('Invalid installation');
    return _request('POST',_registry([..._org,'marketplace','install',publisher,extension,version]));
  }
  Future<Json> activateManifest(String manifestId,{required bool consent}){
    _id(manifestId);if(!consent)throw ArgumentError('Consent required');
    return _post([..._extensions,'activations'],{'manifest_id':manifestId});
  }
  Future<Json> disableExtension(String extension,{required bool consent}){
    if(!consent||!RegExp(r'^[a-z][a-z0-9_]{1,127}$').hasMatch(extension))throw ArgumentError('Invalid disable');
    return _request('POST',_registry([..._extensions,extension,'disable']));
  }
  Future<Json> algorithms()=>_request('GET',_registry([..._processing,'algorithms']));
  Future<Json> jobs()=>_request('GET',_registry([..._processing,'jobs']));
  Future<Json> job(String id){_id(id);return _request('GET',_registry([..._processing,'jobs',id]));}
  Future<Json> cancelJob(String id){_id(id);return _request('POST',_registry([..._processing,'jobs',id,'cancel']));}
  Future<Json> submitJob({required String algorithm,required String inputLayer,required Json expectedInput,String? overlayLayer,Json? expectedOverlay,required Json parameters,required String outputName,required String operationId,required bool consent}){
    _id(inputLayer);if(overlayLayer!=null)_id(overlayLayer);
    if(!consent||!['buffer','clip','dissolve','difference','intersection','union','reproject','spatial_join'].contains(algorithm)||outputName.trim().isEmpty||outputName.length>200||!RegExp(r'^[A-Za-z0-9_.:-]{1,128}$').hasMatch(operationId)||(overlayLayer==null)!=(expectedOverlay==null))throw ArgumentError('Invalid job');
    void check(Json r){_id(r['source_id'] as String);for(final k in ['content_revision','schema_revision']){if(r[k] is! String||!RegExp(r'^[a-f0-9]{64}$').hasMatch(r[k]))throw ArgumentError('Invalid revision');}}
    check(expectedInput);if(expectedOverlay!=null)check(expectedOverlay);
    return _post([..._processing,'jobs'],{'algorithm_id':algorithm,'input_layer_id':inputLayer,'expected_input':expectedInput,if(overlayLayer!=null)'overlay_layer_id':overlayLayer,if(expectedOverlay!=null)'expected_overlay':expectedOverlay,'parameters':parameters,'output_name':outputName,'operation_id':operationId,'execute_now':true});
  }
  Future<Json> projectMembers()=>_request('GET',_registry([..._team,'members']));
  Future<Json> projectMessages({String? peer}){if(peer!=null)_id(peer);return _request('GET',_registry([..._team,'messages'],{'limit':'100',if(peer!=null)'peer':peer}));}
  Future<Json> sendProjectMessage(String body,{String? recipient}){if(recipient!=null)_id(recipient);if(body.trim().isEmpty||body.runes.length>4000)throw ArgumentError('Invalid message');return _post([..._team,'messages'],{'body':body,if(recipient!=null)'recipient_id':recipient});}
  Future<Json> rooms()=>_request('GET',_registry([..._org,'chat','rooms']));
  Future<Json> roomMessages(String room,{String? before}){_id(room);if(before!=null)_id(before);return _request('GET',_registry([..._org,'chat','rooms',room,'messages'],{'limit':'100',if(before!=null)'before':before}));}
  Future<Json> sendRoomMessage(String room,String body,{String? replyTo,List<String> mentions=const[]}){
    _id(room);if(replyTo!=null)_id(replyTo);for(final id in mentions){_id(id);}
    if(body.trim().isEmpty||body.runes.length>4000||mentions.length>20)throw ArgumentError('Invalid message');
    return _post([..._org,'chat','rooms',room,'messages'],{'body':body,if(replyTo!=null)'reply_to_id':replyTo,'mention_ids':mentions,'attachments':[]});
  }
  Future<Json> tasks({bool mine=true})=>_request('GET',_registry([..._team,'tasks'],{'mine':'$mine','limit':'200'}));
  Future<Json> createTask({required String title,required String assignee,String description='',String priority='normal',String? dueDate}){
    _id(assignee);if(title.trim().isEmpty||title.runes.length>200||description.runes.length>2000||!['low','normal','high'].contains(priority)||dueDate!=null&&(!RegExp(r'^\d{4}-\d{2}-\d{2}$').hasMatch(dueDate)||DateTime.tryParse(dueDate)?.toIso8601String().substring(0,10)!=dueDate))throw ArgumentError('Invalid task');
    return _post([..._team,'tasks'],{'title':title,'description':description,'assignee_id':assignee,'priority':priority,if(dueDate!=null)'due_date':dueDate});
  }
  Future<Json> changeTask(String id,int revision,String status){
    _id(id);if(revision<1||!['todo','in_progress','done'].contains(status))throw ArgumentError('Invalid task status');
    return _request('PATCH',_registry([..._team,'tasks',id]),utf8.encode(jsonEncode({'revision':revision,'status':status})));
  }
  Future<Json> notifications()=>_request('GET',_registry([..._team,'notifications']));
  Future<Json> readNotification(String id){_id(id);return _request('POST',_registry([..._team,'notifications',id,'read']));}
  bool _point(double lon,double lat)=>lon.isFinite&&lat.isFinite&&lon.abs()<=180&&lat.abs()<=90;
  Future<Json> geocode(String query,{required bool consent,double? lon,double? lat}){
    if(!consent||query.trim().runes.length<2||query.runes.length>200||(lon==null)!=(lat==null)||lon!=null&&!_point(lon,lat!))throw ArgumentError('Invalid search or consent');
    return _request('GET',_registry(['geocode'],{'q':query,if(lon!=null)'lon':'$lon',if(lat!=null)'lat':'$lat'}));
  }
  Future<Json> route(double fromLon,double fromLat,double toLon,double toLat,{required bool consent,String profile='auto'}){
    if(!consent||!_point(fromLon,fromLat)||!_point(toLon,toLat)||!['auto','motorcycle','motor_scooter','pedestrian','bicycle','truck'].contains(profile))throw ArgumentError('Invalid route or consent');
    return _request('GET',_registry(['routing','route'],{'from_lon':'$fromLon','from_lat':'$fromLat','to_lon':'$toLon','to_lat':'$toLat','profile':profile}));
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
