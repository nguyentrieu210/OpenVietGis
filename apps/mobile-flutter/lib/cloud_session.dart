import 'dart:async';
import 'dart:convert';
import 'package:material_ui/material_ui.dart';
import 'package:http/http.dart' as http;
import 'package:openvietgis_sdk/openvietgis_sdk.dart';

/// A public health response is connectivity evidence, never an identity grant.
final class CloudHealth {
  final Uri origin;
  final String version;
  final bool databaseReady;
  const CloudHealth(this.origin,this.version,this.databaseReady);
}

Uri cloudOrigin(String input) {
  final uri=Uri.parse(input.trim());
  if(uri.scheme!='https'||uri.host.isEmpty||uri.userInfo.isNotEmpty||
    uri.hasQuery||uri.hasFragment||uri.path.isNotEmpty&&uri.path!='/'||
    uri.port!=443) throw ArgumentError('Expected canonical HTTPS origin');
  return Uri.parse(uri.origin);
}

Future<CloudHealth> probeCloud(Uri origin,http.Client client) async {
  final canonical=cloudOrigin(origin.toString());
  final request=http.Request('GET',canonical.replace(path:'/api/v1/health'))
    ..followRedirects=false..headers['accept']='application/json';
  final result=await client.send(request).timeout(const Duration(seconds:10));
  if(result.statusCode!=200||
     !(result.headers['content-type']??'').toLowerCase().startsWith('application/json')){
    await result.stream.listen((_){}).cancel();
    throw StateError('HEALTH_UNAVAILABLE');
  }
  final bytes=<int>[];
  await for(final part in result.stream.timeout(const Duration(seconds:10))){
    if(bytes.length+part.length>65536)throw StateError('HEALTH_LIMIT');
    bytes.addAll(part);
  }
  final data=jsonDecode(utf8.decode(bytes));
  if(data is! Map<String,dynamic>||data['service']!='openvietgis-api-rust'||
      data['runtime']!='rust'||data['status']!='ok'||data['version'] is! String||
      (data['version'] as String).length>128||data['database'] is! Map){
    throw StateError('UNRECOGNIZED_SERVER');
  }
  return CloudHealth(canonical,data['version'] as String,data['database']['status']=='ready');
}

/// Owned by the app, not by a screen or plugin. Public probing never unlocks
/// managed data. Only a future reviewed native identity host may attach a
/// dedicated human-mobile credential. No credentials are saved by this class.
final class CloudSession extends ChangeNotifier {
  final http.Client Function() _client;
  CloudSession({http.Client Function()? clientFactory}):_client=clientFactory??http.Client.new;
  CloudHealth? health;
  bool busy=false;
  String? error;
  ManagedClient? _managed;
  http.Client? _probe;
  int _epoch=0;
  bool _disposed=false;

  bool get authenticated=>_managed!=null;
  ManagedClient get managed=>_managed??(throw StateError('AUTH_REQUIRED'));

  Future<void> check(String input) async {
    if(_disposed)return;
    disconnect();
    final epoch=++_epoch;
    busy=true;error=null;notifyListeners();
    http.Client? client;
    try{
      final origin=cloudOrigin(input);
      client=_client();_probe=client;
      final value=await probeCloud(origin,client).timeout(const Duration(seconds:15));
      if(!_disposed&&epoch==_epoch)health=value;
    }catch(_){
      if(!_disposed&&epoch==_epoch)error='CONNECTION_FAILED';
    }finally{
      client?.close();
      if(!_disposed&&epoch==_epoch){
        _probe=null;busy=false;notifyListeners();
      }
    }
  }

  /// Not exposed as a login button until the browser/keystore/server gates pass.
  void attachReviewedIdentity(MobileCredentials credentials,
      Future<MobileCredentials> Function(String) rotate){
    if(_disposed||busy||health==null||!health!.databaseReady||
       health!.origin!=credentials.scope.origin)throw StateError('SERVER_NOT_READY');
    _managed?.close();_managed=null;
    final client=_client();
    try{
      _managed=ManagedClient(credentials.scope,ScopedMobileTransport(
        scope:credentials.scope,credentials:credentials,rotate:rotate,client:client));
    }catch(_){client.close();rethrow;}
    notifyListeners();
  }

  void disconnect(){
    ++_epoch;_probe?.close();_probe=null;
    _managed?.close();_managed=null;health=null;error=null;busy=false;
    if(!_disposed)notifyListeners();
  }
  @override void dispose(){
    _disposed=true;disconnect();super.dispose();
  }
}
