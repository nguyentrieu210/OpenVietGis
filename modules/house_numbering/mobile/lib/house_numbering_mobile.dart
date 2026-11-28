/// Compiled-in official reference module candidate. All transport is provided
/// by the trusted app host. No API credentials, direct database or Core imports.
/// No activation without matching server manifest and project permission.
library;
import 'dart:async';
import 'dart:convert';
import 'package:openvietgis_plugin_sdk/openvietgis_plugin_sdk.dart';

enum HnPermission{read,edit,manage}
enum HnCommand{routes,areas,buildings,rules,assignments,surveys,proposals,proposal,createProposal,transition,syncSurvey,reviewSurvey,photo}
typedef HnTransport=Future<Json> Function(HnCommand command,Map<String,String> query,Json? body,String? resourceId);
typedef HnPhotoTransport=Future<Json> Function(List<int> bytes,String mediaType);
final class HnScope{
 final String issuer,organization,project,subject,session,manifestId,version;
 final bool active,online,leaseValid;
 final Set<HnPermission> permissions;
 HnScope({required this.issuer,required this.organization,required this.project,required this.subject,required this.session,required this.manifestId,required this.version,required this.active,required this.online,required this.leaseValid,required Set<HnPermission> permissions}):permissions=Set.unmodifiable(permissions);
 bool same(HnScope s)=>issuer==s.issuer&&organization==s.organization&&project==s.project&&subject==s.subject&&session==s.session&&manifestId==s.manifestId&&version==s.version;
}
bool _uuid(Object? s)=>s is String&&RegExp(r'^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$').hasMatch(s)&&s!='00000000-0000-0000-0000-000000000000';
void _id(String s){if(!_uuid(s))deny('INVALID_ID');}
/// The host routes this fixed command enum through the existing official
/// house-numbering API, never through a plugin-supplied URL.
final class HnRoute{
 final String method,path;
 const HnRoute(this.method,this.path);
 static HnRoute resolve(HnCommand command,HnScope s,{String? id}){
  _id(s.organization);_id(s.project);
  final base='/api/v1/organizations/${s.organization}/projects/${s.project}/modules/house-numbering';
  String resource(){if(id==null)deny('RESOURCE_REQUIRED');_id(id);return id;}
  return switch(command){
   HnCommand.routes=>HnRoute('GET','$base/routes'),
   HnCommand.areas=>HnRoute('GET','$base/areas'),
   HnCommand.buildings=>HnRoute('GET','$base/buildings'),
   HnCommand.rules=>HnRoute('GET','$base/rules'),
   HnCommand.assignments=>HnRoute('GET','$base/assignments'),
   HnCommand.surveys=>HnRoute('GET','$base/surveys'),
   HnCommand.proposals=>HnRoute('GET','$base/proposals'),
   HnCommand.proposal=>HnRoute('GET','$base/proposals/${resource()}'),
   HnCommand.createProposal=>HnRoute('POST','$base/proposals'),
   HnCommand.transition=>HnRoute('POST','$base/proposals/${resource()}/transition'),
   HnCommand.syncSurvey=>HnRoute('POST','$base/survey/sync'),
   HnCommand.reviewSurvey=>HnRoute('POST','$base/surveys/${resource()}/review'),
   HnCommand.photo=>HnRoute('POST','$base/survey/photos'),
  };
 }
}
final class SurveyDraft{
 final String clientUuid,buildingId,condition;
 final double? longitude,latitude,accuracy;
 final List<String> photos;
 SurveyDraft({required this.clientUuid,required this.buildingId,required this.condition,this.longitude,this.latitude,this.accuracy,List<String> photos=const[]}):photos=List.unmodifiable(photos){
  _id(clientUuid);_id(buildingId);
  if(!['house','vacant','removed','other'].contains(condition)||photos.length>12||!photos.every(_uuid)||photos.toSet().length!=photos.length)deny('INVALID_SURVEY');
  if((longitude==null)!=(latitude==null)||longitude!=null&&(!longitude!.isFinite||longitude!.abs()>180||!latitude!.isFinite||latitude!.abs()>90)||accuracy!=null&&(longitude==null||!accuracy!.isFinite||accuracy!<=0))deny('INVALID_GPS');
 }
 /// Exact immutable logical payload; caller must persist with scoped encrypted
 /// host outbox before submission, never change ID or fields on retry.
 Json toWire()=>{'client_uuid':clientUuid,'building_id':buildingId,'condition':condition,
  if(longitude!=null)...{'x':longitude,'y':latitude,'crs_code':4326},
  if(accuracy!=null)'gps_accuracy_m':accuracy,'photo_artifact_ids':photos};
}
final class HouseNumbering{
 final HnScope Function() current;
 final HnTransport _transport;
 final HnPhotoTransport _photo;
 final Future<bool> Function(String action) confirm;
 final HnScope _initial;
 bool _closed=false,_busy=false;
 HouseNumbering({required this.current,required HnTransport transport,required HnPhotoTransport photo,required this.confirm}):_initial=current(),_transport=transport,_photo=photo{
  _guard(HnPermission.read);
 }
 HnScope _guard(HnPermission p){
  final c=current();
  if(_closed||!c.same(_initial)||!c.active||!c.online||!c.leaseValid||!c.permissions.contains(p))deny('HN_SCOPE_DENIED');
  return c;
 }
 void close(){_closed=true;}
 Future<Json> _call(HnCommand command,HnPermission permission,{Map<String,String> query=const{},Json? body,String? id,bool sensitive=false})async{
  if(_busy)deny('HN_BUSY');final c=_guard(permission);HnRoute.resolve(command,c,id:id);
  _busy=true;
  try{
   if(sensitive&&!await confirm(command.name))deny('CONSENT_DENIED');
   _guard(permission);
   final result=await _transport(command,Map.unmodifiable(query),body==null?null:jsonDecode(jsonEncode(body)) as Json,id).timeout(const Duration(seconds:30));
   _guard(permission);
   if(utf8.encode(jsonEncode(result)).length>4*1024*1024)deny('HN_RESPONSE_LIMIT');
   return result;
  }on TimeoutException{close();deny('HN_UNKNOWN_OUTCOME');}finally{_busy=false;}
 }
 Future<Json> routes()=>_call(HnCommand.routes,HnPermission.read);
 Future<Json> areas()=>_call(HnCommand.areas,HnPermission.read);
 Future<Json> rules()=>_call(HnCommand.rules,HnPermission.read);
 Future<Json> assignments()=>_call(HnCommand.assignments,HnPermission.read);
 Future<Json> buildings(String routeId){_id(routeId);return _call(HnCommand.buildings,HnPermission.read,query:{'route_id':routeId});}
 Future<Json> surveys()=>_call(HnCommand.surveys,HnPermission.read);
 Future<Json> proposals({int offset=0}){if(offset<0)deny('OFFSET');return _call(HnCommand.proposals,HnPermission.read,query:{'offset':'$offset','limit':'100'});}
 Future<Json> proposal(String id)=>_call(HnCommand.proposal,HnPermission.read,id:id);
 Future<Json> createProposal({required String scopeKind,required String scopeId,required String ruleId,required List<String> buildingIds}){
  _id(scopeId);_id(ruleId);
  if(!['area','route'].contains(scopeKind)||buildingIds.isEmpty||buildingIds.length>5000||!buildingIds.every(_uuid)||buildingIds.toSet().length!=buildingIds.length)deny('INVALID_PROPOSAL');
  return _call(HnCommand.createProposal,HnPermission.edit,sensitive:true,body:{'scope_kind':scopeKind,'scope_id':scopeId,'rule_id':ruleId,'building_ids':List.of(buildingIds)});
 }
 Future<Json> transition(String proposalId,int revision,String action,{String? comment}){
  _id(proposalId);if(revision<=0||!['submit','withdraw','approve','reject'].contains(action)||(comment?.length??0)>1000)deny('INVALID_TRANSITION');
  final permission=['approve','reject'].contains(action)?HnPermission.manage:HnPermission.edit;
  return _call(HnCommand.transition,permission,id:proposalId,sensitive:true,body:{'expected_revision':revision,'action':action,if(comment!=null)'comment':comment});
 }
 Future<Json> reviewSurvey(String id,int revision,String decision){
  _id(id);if(revision<=0||!['accepted','rejected'].contains(decision))deny('INVALID_REVIEW');
  return _call(HnCommand.reviewSurvey,HnPermission.manage,id:id,sensitive:true,body:{'expected_revision':revision,'decision':decision});
 }
 Future<Json> submitSurvey(SurveyDraft draft)=>_call(HnCommand.syncSurvey,HnPermission.edit,sensitive:true,body:{'operations':[draft.toWire()]});
 /// Host captures photo after OS permission and passes bytes; no file paths in plugin.
 Future<String> uploadPhoto(List<int> bytes,String mediaType)async{
  if(_busy||bytes.isEmpty||bytes.length>15*1024*1024||!['image/jpeg','image/png'].contains(mediaType)||bytes.any((b)=>b<0||b>255))deny('PHOTO_DENIED');
  _guard(HnPermission.edit);_busy=true;final copy=List<int>.of(bytes);
  try{
   if(!await confirm('uploadSurveyPhoto'))deny('CONSENT_DENIED');_guard(HnPermission.edit);
   final result=await _photo(copy,mediaType).timeout(const Duration(seconds:30));
   _guard(HnPermission.edit);final id=result['id'];if(!_uuid(id))deny('INVALID_ARTIFACT');return id as String;
  }on TimeoutException{close();deny('HN_UNKNOWN_OUTCOME');}finally{copy.fillRange(0,copy.length,0);_busy=false;}
 }
}
