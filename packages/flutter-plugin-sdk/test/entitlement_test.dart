import 'dart:convert';
import 'package:test/test.dart';
import 'package:cryptography/cryptography.dart';
import 'package:openvietgis_plugin_sdk/openvietgis_plugin_sdk.dart';
void main(){
 test('signed lease expiry rollback permission and core export',()async{
  const s=EntitlementScope(issuer:'fixture',organization:'org',project:'p',subject:'u',device:'d',session:'s',manifestId:'m',version:'1.0.0',releaseDigest:'digest',epoch:3);
  final p={'schema_version':1,...s.wire(),'entitlement_id':'grant','revision':2,'issued_at':100,'expires_at':200,'key_id':'fixture'};
  final a=Ed25519(),key=await Ed25519().newKeyPairFromSeed(List.filled(32,9));
  final signature=await a.sign(canonical(p),keyPair:key);final pub=base64Encode((await key.extractPublicKey()).bytes);
  Future<OfflineEntitlement> open({Set<String> revoked=const{},int floor=2})=>OfflineEntitlement.verify(envelope:{'payload':p,'signature':base64Encode(signature.bytes)},trustedKey:pub,keyId:'fixture',scope:s,anchor:const EntitlementClock('boot',1000,100000),serverTime:110,revisionFloor:floor,revoked:revoked);
  final ent=await open();
  expect(ent.allows(s,const EntitlementClock('boot',2000,101000),currentPermission:false,leaseValid:true),false);
  expect(ent.allows(s,const EntitlementClock('boot',2000,101000),currentPermission:true,leaseValid:true),true);
  expect(ent.allows(s,const EntitlementClock('boot',0,99000),currentPermission:true,leaseValid:true),false);
  expect(ent.allows(s,const EntitlementClock('boot',2000,101000),currentPermission:true,leaseValid:true),false);
  await expectLater(open(revoked:{'grant'}),throwsStateError);await expectLater(open(floor:3),throwsStateError);
  expect(coreExportAllowed(dataPermission:true,leaseValid:true,exportPolicy:true),true);
  expect(coreExportAllowed(dataPermission:false,leaseValid:true,exportPolicy:true),false);
 });
}
