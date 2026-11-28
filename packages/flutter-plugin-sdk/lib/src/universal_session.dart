/// Transport-neutral host candidate. The trusted native application must
/// supply a qualified transport, scope and server-verified release identity.
/// Not a replacement for SPEC073 cryptographic verifier or entitlement broker.
import 'dart:async';
import 'dart:convert';
import 'universal_document.dart';
import 'universal_runtime.dart';

final class UniversalScope {
  final String issuer,organization,project,subject,session,manifest,release,lock,grantRevision;
  final int activationRevision,catalogEpoch;
  final bool qualified,online,granted;
  const UniversalScope({required this.issuer,required this.organization,required this.project,required this.subject,
    required this.session,required this.manifest,required this.release,required this.lock,required this.activationRevision,
    required this.catalogEpoch,required this.grantRevision,required this.qualified,required this.online,required this.granted});
  bool same(UniversalScope other)=>issuer==other.issuer&&organization==other.organization&&project==other.project&&
    subject==other.subject&&session==other.session&&manifest==other.manifest&&release==other.release&&lock==other.lock&&
    activationRevision==other.activationRevision&&catalogEpoch==other.catalogEpoch&&grantRevision==other.grantRevision;
}
/// Host callback uses only an existing authenticated same-origin broker.
/// The plugin has no access to URL, credentials, native bridge or callback.
typedef UniversalInvoke=Future<UniversalJson> Function(UniversalScope scope,String actionId,UniversalJson arguments,String requestId);
final class UniversalSession {
  final UniversalDocument document;
  final UniversalScope Function() _current;
  final UniversalInvoke _invoke;
  final String Function() _newRequestId;
  final UniversalScope _initial;
  bool _closed=false,_busy=false;
  UniversalSession({required this.document,required UniversalScope Function() current,required UniversalInvoke invoke,
    required String Function() newRequestId}):_current=current,_invoke=invoke,_newRequestId=newRequestId,_initial=current(){_guard();}
  /// Host-only adapter. Callback receives the exact common invocation body,
  /// never a plugin-controlled URL, token, path, or native command.
  factory UniversalSession.fromRuntime({required UniversalRuntime runtime,required UniversalScope Function() current,
    required Future<UniversalJson> Function(UniversalScope,UniversalJson) send,required String Function() newRequestId}){
    UniversalScope checked(){
    final bound=current();
    if(!runtime.granted||bound.manifest!=runtime.manifest||bound.release!=runtime.release||bound.lock!=runtime.lock||
      bound.grantRevision!=runtime.grantRevision||bound.activationRevision!=runtime.activationRevision||
      bound.catalogEpoch!=runtime.catalogEpoch)throw StateError('UNIVERSAL_RUNTIME_MISMATCH');
    return bound;
    }
    return UniversalSession(document:runtime.document,current:checked,newRequestId:newRequestId,
      invoke:(scope,action,input,request)=>send(scope,runtime.invocation(action,input,request)));
  }
  UniversalScope _guard(){
    final now=_current();
    if(_closed||!now.same(_initial)||!now.qualified||!now.online||!now.granted)throw StateError('UNIVERSAL_RUNTIME_DENIED');
    return now;
  }
  void close(){_closed=true;}
  Future<UniversalJson> invoke(String actionId,Map<String,String> values) async {
    final scope=_guard();
    if(_busy)throw StateError('UNIVERSAL_RUNTIME_BUSY');
    final action=document.actions[actionId];
    if(action==null)throw StateError('UNKNOWN_ACTION');
    if(action.command=='gis_processing_plan')throw StateError('NATIVE_PROCESSING_CONFIRMATION_NOT_QUALIFIED');
    final input=action.arguments(values);
    final requestId=_newRequestId();
    if(!RegExp(r'^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$').hasMatch(requestId)||requestId=='00000000-0000-0000-0000-000000000000')throw StateError('INVALID_REQUEST_ID');
    _busy=true;
    try{
      final result=await _invoke(scope,actionId,input,requestId).timeout(Duration(milliseconds:action.timeoutMs));
      _guard();
      final bytes=utf8.encode(jsonEncode(result));
      if(bytes.length>256*1024)throw StateError('RESPONSE_LIMIT');
      return jsonDecode(utf8.decode(bytes)) as UniversalJson;
    } on TimeoutException {close();throw StateError('UNIVERSAL_TIMEOUT');}
    finally{_busy=false;}
  }
}
