import 'dart:convert';
import 'scope.dart';
import 'managed.dart';

/// Foreground collaboration view-model, memory only. No plaintext offline
/// persistence, no automatic message resend, no push notification registration.
/// Network/session status is supplied by trusted host, not plugin.
final class CollaborationState {
 final ManagedClient _client;
 final Scope? Function() currentScope;
 final bool Function() online;
 final Map<String,Json> _snapshots={};
 int _generation=0;
 bool _closed=false;
 CollaborationState(this._client,{required this.currentScope,required this.online});
 bool _same()=>!_closed&&currentScope()?.same(_client.scope)==true;
 void _guard(){
  if(!_same()){revoke();throw StateError('SCOPE_REVOKED');}
  if(!online()){disconnect();throw StateError('OFFLINE');}
 }
 void disconnect(){_generation++;_snapshots.clear();}
 void revoke(){if(_closed)return;_closed=true;disconnect();_client.close();}
 Json? snapshot(String kind){
  _guard();final v=_snapshots[kind];return v==null?null:jsonDecode(jsonEncode(v)) as Json;
 }
 Future<Json> _read(String kind,Future<Json> Function() request)async{
  _guard();final generation=_generation;
  try{
   final value=await request();_guard();
   if(generation!=_generation)throw StateError('STALE_RESPONSE');
   if(utf8.encode(jsonEncode(value)).length>4*1024*1024)throw StateError('SNAPSHOT_LIMIT');
   final copy=jsonDecode(jsonEncode(value)) as Json;
   // Only one active room message snapshot; no unbounded room cache.
   _snapshots[kind]=copy;
   return jsonDecode(jsonEncode(copy)) as Json;
  }on ProtocolError catch(e){if(e.status==401||e.status==403){revoke();}else{disconnect();}rethrow;}
 }
 Future<Json> rooms()=>_read('rooms',_client.rooms);
 Future<Json> messages(String room)=>_read('messages',()=>_client.roomMessages(room));
 Future<Json> tasks()=>_read('tasks',()=>_client.tasks());
 Future<Json> notifications()=>_read('notifications',_client.notifications);
 Future<Json> _mutate(Future<Json> Function() request)async{
  _guard();final generation=_generation;
  try{
   final result=await request();_guard();if(generation!=_generation)throw StateError('STALE_RESPONSE');
   _snapshots.clear();return result;
  }on ProtocolError catch(e){if(e.status==401||e.status==403){revoke();}else{disconnect();}rethrow;}
  catch(_){disconnect();rethrow;}
 }
 Future<Json> send(String room,String body,{String? replyTo,List<String> mentions=const[]})=>_mutate(()=>_client.sendRoomMessage(room,body,replyTo:replyTo,mentions:mentions));
 Future<Json> setTask(String id,int revision,String status)=>_mutate(()=>_client.changeTask(id,revision,status));
 /// Reconnect must reload authoritative lists; no local cursor or speculative read receipts.
 Future<void> reconnect()async{disconnect();_guard();await rooms();await tasks();await notifications();}
}
