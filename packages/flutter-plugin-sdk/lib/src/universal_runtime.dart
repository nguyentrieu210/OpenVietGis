/// Shared SPEC075 wire parser. Parsing does not qualify native execution,
/// authenticate a transport, verify release signatures or issue a lease.
import 'dart:convert';
import 'universal_document.dart';

Never _invalid()=>throw StateError('INVALID_PLUGIN_V2_CONTRACT');
Map<String,dynamic> _object(Object? v)=>v is Map<String,dynamic>?v:_invalid();
void _exact(Map<String,dynamic> v,Set<String> keys){
  if(v.length!=keys.length||!keys.every(v.containsKey))_invalid();
}
String _str(Object? v,int max)=>v is String&&v.runes.length<=max?v:_invalid();
int _positive(Object? v)=>v is int&&v>=1&&v<=9007199254740991?v:_invalid();
String _hex(Object? v){final s=_str(v,64);return RegExp(r'^[a-f0-9]{64}$').hasMatch(s)?s:_invalid();}
String _uuid(Object? v){
 final s=_str(v,36);
 return RegExp(r'^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$').hasMatch(s)&&s!='00000000-0000-0000-0000-000000000000'?s:_invalid();
}
String _grant(Object? v){
 final s=_str(v,19);
 return RegExp(r'^(0|[1-9][0-9]{0,18})$').hasMatch(s)&&BigInt.parse(s)<=BigInt.parse('9223372036854775807')?s:_invalid();
}
final class UniversalPin {
 final String key,version,release,manifest,activation;
 final int revision;
 const UniversalPin._(this.key,this.version,this.release,this.manifest,this.activation,this.revision);
 factory UniversalPin.parse(Object? value){
  final p=_object(value);_exact(p,{'key','version','release_digest','manifest_id','activation_id','activation_revision'});
  final key=_str(p['key'],128);
  if(!RegExp(r'^[A-Za-z0-9_.-]+$').hasMatch(key))_invalid();
  return UniversalPin._(key,_str(p['version'],64),_hex(p['release_digest']),_uuid(p['manifest_id']),_uuid(p['activation_id']),_positive(p['activation_revision']));
 }
}
final class UniversalRuntime {
 final UniversalDocument document;
 final String manifest,release,lock,grantRevision;
 final int activationRevision,catalogEpoch;
 final bool granted;
 final List<UniversalPin> pins;
 final Set<String> permissions;
 UniversalRuntime._(this.document,this.manifest,this.release,this.lock,this.grantRevision,this.activationRevision,this.catalogEpoch,this.granted,List<UniversalPin> pins,Set<String> permissions):
   pins=List.unmodifiable(pins),permissions=Set.unmodifiable(permissions);
 factory UniversalRuntime.parse(Object? value,{required String expectedManifest}){
  final r=_object(value);
  _exact(r,{'document','release_digest','activation_revision','manifest_id','profile','granted','grant_revision','permissions','dependency_lock','lock_digest','catalog_epoch'});
  final manifest=_uuid(r['manifest_id']),release=_hex(r['release_digest']),lock=_hex(r['lock_digest']),grant=_grant(r['grant_revision']);
  final revision=_positive(r['activation_revision']),epoch=_positive(r['catalog_epoch']);
  final granted=r['granted'];
  // Only the server's existing qualified wire profile is parsed. This
  // deliberately does not introduce a new native authentication profile.
  if(manifest!=expectedManifest||r['profile']!='web_read_only'||granted is! bool||(granted&&grant=='0'))_invalid();
  final rawPins=r['dependency_lock'];
  if(rawPins is! List||rawPins.isEmpty||rawPins.length>100)_invalid();
  final pins=rawPins.map(UniversalPin.parse).toList();
  if(pins.map((p)=>p.key).toSet().length!=pins.length)_invalid();
  final root=pins.last;
  if(root.manifest!=manifest||root.release!=release||root.revision!=revision)_invalid();
  final rawPerms=r['permissions'];
  if(rawPerms is! List||rawPerms.length>64)_invalid();
  final permissions=rawPerms.map((p)=>_str(p,128)).toSet();
  if(permissions.length!=rawPerms.length)_invalid();
  final doc=UniversalDocument.parse(r['document'],extensionKey:root.key,permissions:permissions);
  return UniversalRuntime._(doc,manifest,release,lock,grant,revision,epoch,granted,pins,permissions);
 }
 UniversalJson invocation(String actionId,UniversalJson input,String requestId){
  if(!granted)_invalid();
  final action=document.actions[actionId];
  if(action==null)_invalid();
  for(final e in input.entries){
   if(!action.requiredFields.containsKey(e.key)||e.value is! String||(e.value as String).runes.length>4096)_invalid();
  }
  for(final e in action.requiredFields.entries){if(e.value&&!input.containsKey(e.key))_invalid();}
  if(utf8.encode(jsonEncode(input)).length>256*1024)_invalid();
  return {'request_id':_uuid(requestId),'release_digest':release,'activation_revision':activationRevision,
    'grant_revision':grantRevision,'lock_digest':lock,'catalog_epoch':catalogEpoch,'action_id':actionId,'input':Map<String,dynamic>.of(input)};
 }
}
