import 'dart:convert';
import 'package:cryptography/cryptography.dart';
import 'release.dart';
/// Monotonic trusted host clock; no offline trust restoration after reboot.
final class EntitlementClock{
 final String bootId;final int monotonicMs,wallMs;
 const EntitlementClock(this.bootId,this.monotonicMs,this.wallMs);
}
final class EntitlementScope{
 final String issuer,organization,project,subject,device,session,manifestId,version,releaseDigest;
 final int epoch;
 const EntitlementScope({required this.issuer,required this.organization,required this.project,required this.subject,required this.device,required this.session,required this.manifestId,required this.version,required this.releaseDigest,required this.epoch});
 Json wire()=>{'issuer':issuer,'organization':organization,'project':project,'subject':subject,'device':device,'session':session,'manifest_id':manifestId,'version':version,'release_digest':releaseDigest,'epoch':epoch};
}
final class OfflineEntitlement{
 final Json _payload;final EntitlementClock _anchor;final int _serverTime;
 bool _revoked=false;
 OfflineEntitlement._(this._payload,this._anchor,this._serverTime);
 String get id=>_payload['entitlement_id'];
 int get revision=>_payload['revision'];
 static Future<OfflineEntitlement> verify({required Json envelope,required String trustedKey,required String keyId,required EntitlementScope scope,required EntitlementClock anchor,required int serverTime,required int revisionFloor,required Set<String> revoked})async{
   exact(envelope,{'payload','signature'});
   final p=jsonDecode(jsonEncode(envelope['payload'])) as Json;
   exact(p,{'schema_version','issuer','entitlement_id','organization','project','subject','device','session','manifest_id','version','release_digest','epoch','revision','issued_at','expires_at','key_id'});
   final signature=base64Decode(envelope['signature'] as String),key=base64Decode(trustedKey);
   if(signature.length!=64||key.length!=32||base64Encode(signature)!=envelope['signature']||base64Encode(key)!=trustedKey||!await Ed25519().verify(canonical(p),signature:Signature(signature,publicKey:SimplePublicKey(key,type:KeyPairType.ed25519))))deny('ENTITLEMENT_SIGNATURE');
   if(p['schema_version']!=1||p['key_id']!=keyId||!token(p['entitlement_id'])||revoked.contains(p['entitlement_id'])||(p['revision'] as int)<revisionFloor||(p['revision'] as int)<1||p['issued_at']>serverTime||p['expires_at']<=serverTime||p['expires_at']-p['issued_at']>86400||anchor.bootId.isEmpty||anchor.monotonicMs<0||scope.wire().entries.any((e)=>p[e.key]!=e.value))deny('ENTITLEMENT_DENIED');
   return OfflineEntitlement._(p,anchor,serverTime);
 }
 void revoke(){_revoked=true;}
 /// A valid entitlement alone never authorizes project data or export.
 bool allows(EntitlementScope scope,EntitlementClock now,{required bool currentPermission,required bool leaseValid}){
   if(_revoked||now.bootId!=_anchor.bootId||now.monotonicMs<_anchor.monotonicMs||now.wallMs<_anchor.wallMs||_serverTime+((now.monotonicMs-_anchor.monotonicMs)~/1000)>=_payload['expires_at']||scope.wire().entries.any((e)=>_payload[e.key]!=e.value)){_revoked=true;return false;}
   return currentPermission&&leaseValid;
 }
}
/// Core export depends on current data permission and data policy, not paid
/// plugin availability. This predicate does not export or reveal any data.
bool coreExportAllowed({required bool dataPermission,required bool leaseValid,required bool exportPolicy})=>dataPermission&&leaseValid&&exportPolicy;
