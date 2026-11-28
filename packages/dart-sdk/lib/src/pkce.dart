/// Protocol primitives only. No invented server exchange or embedded login.
import 'dart:convert';
import 'dart:math';
import 'package:crypto/crypto.dart';

String _random(){final rng=Random.secure();return base64UrlEncode(List<int>.generate(32,(_)=>rng.nextInt(256))).replaceAll('=','');}
String s256(String verifier)=>base64UrlEncode(sha256.convert(ascii.encode(verifier)).bytes).replaceAll('=','');
bool _equal(String a,String b){if(a.length!=b.length)return false;var d=0;for(var i=0;i<a.length;i++){d|=a.codeUnitAt(i)^b.codeUnitAt(i);}return d==0;}

final class PkceAttempt {
  final Uri issuer,authorizeEndpoint,redirect;
  final String clientId,verifier,state,nonce;
  final Stopwatch _age=Stopwatch()..start();
  bool _used=false;
  PkceAttempt({required this.issuer,required this.authorizeEndpoint,required this.redirect,required this.clientId}):
      verifier=_random(),state=_random(),nonce=_random(){
    if(issuer.scheme!='https'||issuer.host.isEmpty||issuer.userInfo.isNotEmpty||issuer.hasQuery||issuer.hasFragment||
      authorizeEndpoint.scheme!='https'||authorizeEndpoint.origin!=issuer.origin||authorizeEndpoint.userInfo.isNotEmpty||
      authorizeEndpoint.hasFragment||authorizeEndpoint.hasQuery||
      redirect.scheme!='https'||redirect.host.isEmpty||redirect.userInfo.isNotEmpty||redirect.hasQuery||redirect.hasFragment||
      clientId.isEmpty){throw ArgumentError('Invalid reviewed OAuth configuration');}
  }
  Uri authorizationUri()=>authorizeEndpoint.replace(queryParameters:{
    'response_type':'code','client_id':clientId,'redirect_uri':redirect.toString(),
    'code_challenge':s256(verifier),'code_challenge_method':'S256','state':state,'nonce':nonce,'scope':'openid'
  });
  /// Exact redirect, unique parameters, issuer and state; consumed on any callback.
  String consume(Uri callback){
    if(_used)throw StateError('ATTEMPT_USED');_used=true;
    if(_age.elapsed>const Duration(minutes:5)||callback.scheme!=redirect.scheme||callback.authority!=redirect.authority||
      callback.path!=redirect.path||callback.hasFragment||callback.userInfo.isNotEmpty||
      callback.queryParametersAll.values.any((v)=>v.length!=1)||callback.queryParameters.containsKey('error')||
      !_equal(callback.queryParameters['state']??'',state)||callback.queryParameters['iss']!=issuer.toString()){
      throw StateError('INVALID_CALLBACK');
    }
    final code=callback.queryParameters['code'];
    if(code==null||code.isEmpty||code.length>4096)throw StateError('INVALID_CODE');
    return code;
  }
}
/// Refresh outcome unknown => reauthenticate; never replay an old refresh token.
final class RefreshGate<T>{
  Future<T>? _pending;
  bool _locked=false;
  Future<T> run(Future<T> Function() rotate){
    if(_locked)return Future.error(StateError('REAUTHENTICATE'));
    return _pending??=_execute(rotate);
  }
  Future<T> _execute(Future<T> Function() rotate)async{
    try{return await rotate();}catch(_){_locked=true;rethrow;}finally{_pending=null;}
  }
  void revoke(){_locked=true;}
}
