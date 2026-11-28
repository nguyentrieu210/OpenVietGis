/// SPEC075 online universal document contract. Additive to SPEC073 release.dart.
/// No conversion of legacy mobile signed envelopes, no device permission grants,
/// no offline lease, no downloaded Dart, and no credential handling.
import 'dart:convert';

typedef UniversalJson = Map<String, dynamic>;
Never _deny() => throw StateError('INVALID_UNIVERSAL_DOCUMENT');
UniversalJson _object(Object? v) => v is Map<String,dynamic> ? v : _deny();
void _exact(UniversalJson v, Set<String> keys) {
  if (v.length != keys.length || !v.keys.every(keys.contains)) _deny();
}
String _string(Object? v,int max) => v is String && v.runes.length<=max ? v : _deny();
String _token(Object? v) {
  final s=_string(v,64);
  return RegExp(r'^[A-Za-z0-9_.-]+$').hasMatch(s) ? s : _deny();
}
List<dynamic> _list(Object? v,int max) => v is List && v.length<=max ? v : _deny();
int _integer(Object? v,int min,int max) => v is int && v>=min && v<=max ? v : _deny();
const _policy={'host_context':'host_context','host_notice':'host_notice','gis_layers':'map_view','gis_layer':'map_view'};

final class UniversalAction {
  final String id,command,permission;
  final int timeoutMs;
  final Map<String,bool> requiredFields;
  UniversalAction._(this.id,this.command,this.permission,this.timeoutMs,Map<String,bool> fields):requiredFields=Map.unmodifiable(fields);
  UniversalJson arguments(Map<String,String> values) {
    final result=<String,dynamic>{};
    for(final entry in requiredFields.entries){
      final v=values[entry.key];
      if(v==null||v.isEmpty){if(entry.value)_deny();continue;}
      if(v.runes.length>4096)_deny();
      result[entry.key]=v;
    }
    return result;
  }
}
final class UniversalNode {
  final String id,kind,label;
  final String? binding;
  final List<UniversalNode> children;
  UniversalNode._(this.id,this.kind,this.label,this.binding,List<UniversalNode> children):children=List.unmodifiable(children);
}
final class UniversalDocument {
  final String slot;
  final List<UniversalNode> widgets;
  final Map<String,UniversalAction> actions;
  UniversalDocument._(this.slot,List<UniversalNode> widgets,Map<String,UniversalAction> actions):widgets=List.unmodifiable(widgets),actions=Map.unmodifiable(actions);

  factory UniversalDocument.parse(Object? value,{required String extensionKey,required Set<String> permissions}){
    if(utf8.encode(jsonEncode(value)).length>256*1024)_deny();
    final d=_object(value);_exact(d,{'slot','widgets','actions'});
    final slot=_string(d['slot'],32);
    if(!{'workspace','map_tool','form','dashboard'}.contains(slot))_deny();
    final allowed=_policy.values.map((p)=>'ext.$extensionKey.$p').toSet();
    if(permissions.length>64||!permissions.every(allowed.contains))_deny();
    final actions=<String,UniversalAction>{};
    final fields=<String>{};
    for(final item in _list(d['actions'],32)){
      final a=_object(item);_exact(a,{'id','command_id','permission','consent','timeout_ms','input'});
      final id=_token(a['id']), command=_token(a['command_id']), permission=_string(a['permission'],128);
      if(actions.containsKey(id)||!_policy.containsKey(command)||a['consent']!='read'||permission!='ext.$extensionKey.${_policy[command]}'||!permissions.contains(permission))_deny();
      final timeout=_integer(a['timeout_ms'],1,30000);
      final input=_object(a['input']);
      final field=switch(command){'host_notice'=>'text','gis_layer'=>'layer_id','gis_layers'=>'after_id',_=>null};
      final required=command=='host_notice'||command=='gis_layer';
      if(input.isEmpty){if(required)_deny();}
      else {
        if(field==null||input.length!=1||!input.containsKey(field))_deny();
        final schema=_object(input[field]);_exact(schema,{'kind','required'});
        if(schema['kind']!='text'||schema['required']!=required)_deny();
        fields.add(field);
      }
      actions[id]=UniversalAction._(id,command,permission,timeout,{for(final k in input.keys)k:required});
    }
    final ids=<String>{};
    var count=0;
    List<UniversalNode> walk(Object? value,int depth){
      if(depth>16)_deny();
      final nodes=<UniversalNode>[];
      for(final item in _list(value,500)){
        final n=_object(item),id=_token(n['id']),kind=_string(n['kind'],32);
        if(++count>500||!ids.add(id))_deny();
        switch(kind){
          case 'text':
            _exact(n,{'kind','id','text'});
            nodes.add(UniversalNode._(id,kind,_string(n['text'],2000),null,[]));
            break;
          case 'input':
            _exact(n,{'kind','id','label','field'});
            final field=_token(n['field']);
            if(!fields.contains(field))_deny();
            nodes.add(UniversalNode._(id,kind,_string(n['label'],200),field,[]));
            break;
          case 'button':
            _exact(n,{'kind','id','label','action'});
            final action=_token(n['action']);
            if(!actions.containsKey(action))_deny();
            nodes.add(UniversalNode._(id,kind,_string(n['label'],200),action,[]));
            break;
          case 'group':
            _exact(n,{'kind','id','children'});
            nodes.add(UniversalNode._(id,kind,'',null,walk(n['children'],depth+1)));
            break;
          default:_deny();
        }
      }
      return nodes;
    }
    return UniversalDocument._(slot,walk(d['widgets'],1),actions);
  }
}
