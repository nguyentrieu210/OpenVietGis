import 'dart:async';
import 'dart:convert';
import 'release.dart';

const capabilities={'data.read','data.edit','gps.read','camera.capture','module.read','module.edit','module.review'};
const kinds={'text','input_text','input_number','input_date','select','table','form','map_tool'};
const _types={'text','integer','number','boolean','date','uuid','uuid_list'};
final class Action{
 final String id,command,capability,permission,mode,consent;
 final int timeoutMs;
 final Map<String,String> input,output;
 Action._(Json j):id=j['id'],command=j['command_id'],capability=j['capability'],permission=j['permission'],mode=j['mode'],consent=j['consent'],timeoutMs=j['timeout_ms'],input=Map.unmodifiable(Map<String,String>.from(j['input'])),output=Map.unmodifiable(Map<String,String>.from(j['output']));
 static Action parse(Json j,VerifiedRelease release){
  exact(j,{'id','command_id','capability','permission','mode','consent','timeout_ms','input','output'});
  if(![j['id'],j['command_id'],j['capability'],j['permission']].every(token)||!capabilities.contains(j['capability'])||!release.capabilities.contains(j['capability'])||!release.permissions.contains(j['permission'])||!['online','offline'].contains(j['mode'])||!['none','explicit'].contains(j['consent'])||j['timeout_ms'] is! int||j['timeout_ms']<1||j['timeout_ms']>30000)deny('ACTION_DENIED');
  if(j['capability']!='data.read'&&j['capability']!='module.read'&&j['consent']!='explicit')deny('CONSENT_REQUIRED');
  for(final schema in [j['input'],j['output']]){
   if(schema is! Map||schema.length>32||schema.entries.any((e)=>!token(e.key)||!_types.contains(e.value)))deny('ACTION_SCHEMA');
  }
  return Action._(j);
 }
}
final class Node{
 final String id,kind,label;
 final List<Node> children;
 Node._(this.id,this.kind,this.label,this.children);
}
final class Runtime{
 final VerifiedRelease release;
 final String slot;
 final List<Node> nodes;
 final Map<String,Action> actions;
 Runtime._(this.release,this.slot,this.nodes,this.actions);
 factory Runtime.parse(VerifiedRelease release){
  final j=release.contribution;
  exact(j,{'slot','nodes','actions'});
  if(!['workspace','map_tool','form','dashboard'].contains(j['slot'])||j['nodes'] is! List||j['actions'] is! List||j['actions'].length>32||release.capabilities.any((c)=>!capabilities.contains(c)))deny('INVALID_CONTRIBUTION');
  var count=0;final ids=<String>{};
  Node parse(Json n,int depth){
   exact(n,{'id','kind','label','children'});
   if(++count>500||depth>16||!token(n['id'])||!ids.add(n['id'])||!kinds.contains(n['kind'])||n['label'] is! String||(n['label'] as String).length>500||n['children'] is! List)deny('INVALID_NODE');
   if(n['kind']!='form'&&n['children'].isNotEmpty)deny('INVALID_CHILDREN');
   return Node._(n['id'],n['kind'],n['label'],List.unmodifiable((n['children'] as List).map((x)=>parse(x as Json,depth+1))));
  }
  final nodes=List<Node>.unmodifiable((j['nodes'] as List).map((n)=>parse(n as Json,1)));
  final actions=<String,Action>{};
  for(final a in j['actions']){final action=Action.parse(a as Json,release);if(actions.containsKey(action.id))deny('DUPLICATE_ACTION');actions[action.id]=action;}
  return Runtime._(release,j['slot'],nodes,Map.unmodifiable(actions));
 }
}
bool _uuid(Object? v)=>v is String&&RegExp(r'^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$').hasMatch(v)&&v!='00000000-0000-0000-0000-000000000000';
void validateValues(Map<String,String> schema,Json values){
 if(values.length!=schema.length||values.keys.any((k)=>!schema.containsKey(k)))deny('INVALID_ARGUMENT_KEYS');
 for(final e in schema.entries){
  final v=values[e.key];
  final valid=switch(e.value){
   'text'=>v is String&&utf8.encode(v).length<=8192,
   'integer'=>v is int&&v.abs()<=9007199254740991,
   'number'=>v is num&&v.isFinite,
   'boolean'=>v is bool,
   'date'=>v is String&&RegExp(r'^\d{4}-\d{2}-\d{2}$').hasMatch(v)&&DateTime.tryParse(v)?.toIso8601String().substring(0,10)==v,
   'uuid'=>_uuid(v),
   'uuid_list'=>v is List&&v.length<=5000&&v.every(_uuid)&&v.toSet().length==v.length,
   _=>false,
  };
  if(!valid)deny('INVALID_ARGUMENT_TYPE');
 }
 if(utf8.encode(jsonEncode(values)).length>256*1024)deny('ARGUMENT_LIMIT');
}
/// Trusted host owns scope, trusted time, permissions, lease and command table.
/// No credential or database handle is represented in plugin input/output.
final class Activation{
 final String issuer,organization,project,subject,session,manifestId,releaseDigest;
 final Set<String> permissions,capabilities;
 final bool active,entitled,online,leaseValid;
 final int now,epoch;
 Activation({required this.issuer,required this.organization,required this.project,required this.subject,required this.session,required this.manifestId,required this.releaseDigest,required Set<String> permissions,required Set<String> capabilities,required this.active,required this.entitled,required this.online,required this.leaseValid,required this.now,required this.epoch}):permissions=Set.unmodifiable(permissions),capabilities=Set.unmodifiable(capabilities);
 bool same(Activation o)=>issuer==o.issuer&&organization==o.organization&&project==o.project&&subject==o.subject&&session==o.session&&manifestId==o.manifestId&&releaseDigest==o.releaseDigest&&epoch==o.epoch;
}
final class Cancellation{
 bool _cancelled=false;
 bool get cancelled=>_cancelled;
 void check(){if(cancelled)deny('CANCELLED');}
}
typedef CommandHandler=Future<Json> Function(Json input,Activation scope,Cancellation cancellation);
final class Command{
 final String capability,permission,mode,consent;
 final Map<String,String> input,output;
 final CommandHandler run;
 Command({required this.capability,required this.permission,required this.mode,required this.consent,required Map<String,String> input,required Map<String,String> output,required this.run}):input=Map.unmodifiable(input),output=Map.unmodifiable(output);
}
bool _schemaEqual(Map<String,String> a,Map<String,String> b)=>a.length==b.length&&a.entries.every((e)=>b[e.key]==e.value);
/// Serial bounded execution; timeout closes activation and cancels all queued work.
/// Handler must cooperatively cancel before side effects; unknown server outcomes
/// must be reconciled by host, never retried by plugin.
final class Broker{
 final Runtime runtime;
 final Activation Function() current;
 final Future<bool> Function(Action action,Activation scope) consent;
 final Map<String,Command> _commands;
 final Activation _initial;
 final List<Cancellation> _pending=[];
 Future<void> _tail=Future.value();
 bool _closed=false;
 Broker({required this.runtime,required this.current,required this.consent,required Map<String,Command> commands}):_initial=current(),_commands=Map.unmodifiable(commands){
  _guard();
 }
 Activation _guard(){
  final c=current(),r=runtime.release;
  if(_closed||!c.same(_initial)||!c.active||!c.entitled||!c.leaseValid||c.issuer!=r.issuer||c.manifestId!=r.manifestId||c.releaseDigest!=r.releaseDigest||c.epoch!=r.epoch||c.now>=r.expiresAt)deny('ACTIVATION_DENIED');
  return c;
 }
 void close(){_closed=true;for(final c in _pending){c._cancelled=true;}}
 Future<Json> invoke(String actionId,Json args){
  _guard();
  final a=runtime.actions[actionId];if(a==null)deny('UNKNOWN_ACTION');
  final command=_commands[a.command];if(command==null||command.capability!=a.capability||command.permission!=a.permission||command.mode!=a.mode||command.consent!=a.consent||!_schemaEqual(command.input,a.input)||!_schemaEqual(command.output,a.output))deny('COMMAND_DENIED');
  validateValues(a.input,args);final copy=jsonDecode(jsonEncode(args)) as Json;
  if(_pending.length>=8)deny('QUEUE_LIMIT');
  final cancel=Cancellation();_pending.add(cancel);final result=Completer<Json>();
  _tail=_tail.then((_)async{
   try{
    cancel.check();final c=_guard();
    if(!c.permissions.contains(a.permission)||!c.capabilities.contains(a.capability)||(a.mode=='online'&&!c.online))deny('PERMISSION_DENIED');
    if(a.consent=='explicit'&&!await consent(a,c).timeout(Duration(milliseconds:a.timeoutMs)))deny('CONSENT_DENIED');
    cancel.check();final after=_guard();
    if(!after.permissions.contains(a.permission)||!after.capabilities.contains(a.capability)||(a.mode=='online'&&!after.online))deny('PERMISSION_DENIED');
    final response=await command.run(copy,after,cancel).timeout(Duration(milliseconds:a.timeoutMs));
    cancel.check();final end=_guard();
    if(!end.permissions.contains(a.permission)||!end.capabilities.contains(a.capability)||(a.mode=='online'&&!end.online))deny('PERMISSION_DENIED');
    validateValues(a.output,response);result.complete(jsonDecode(jsonEncode(response)) as Json);
   }on TimeoutException{close();result.completeError(StateError('ACTION_TIMEOUT_UNKNOWN_OUTCOME'));}
   catch(_){result.completeError(StateError('ACTION_DENIED_OR_FAILED'));}
   finally{_pending.remove(cancel);}
  });
  return result.future;
 }
}
