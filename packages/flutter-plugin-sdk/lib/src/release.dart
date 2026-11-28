import 'dart:convert';
import 'dart:collection';
import 'package:crypto/crypto.dart' as hash;
import 'package:cryptography/cryptography.dart';

typedef Json=Map<String,dynamic>;
Never deny(String code)=>throw StateError(code);
bool token(Object? s)=>s is String&&RegExp(r'^[A-Za-z0-9_.:-]{1,128}$').hasMatch(s);
void exact(Json m,Set<String> keys){if(m.length!=keys.length||!m.keys.every(keys.contains))deny('UNKNOWN_FIELDS');}
Object? _sorted(Object? v,int depth){
 if(depth>24)deny('DEPTH');
 if(v is Map){if(v.length>500||v.keys.any((k)=>!token(k)))deny('KEYS');return SplayTreeMap<String,Object?>.from(Map<String,Object?>.fromEntries(v.entries.map((e)=>MapEntry(e.key as String,_sorted(e.value,depth+1)))));}
 if(v is List){if(v.length>500)deny('ARRAY_LIMIT');return v.map((x)=>_sorted(x,depth+1)).toList();}
 if(v is int){if(v.abs()>9007199254740991)deny('INTEGER_LIMIT');return v;}
 if(v is String){if(utf8.encode(v).length>65536||v.runes.any((r)=>r<32||(r>=127&&r<=159)))deny('STRING_LIMIT');return v;}
 if(v==null||v is bool)return v;
 deny('CANONICAL_TYPE');
}
List<int> canonical(Json value){final b=utf8.encode(jsonEncode(_sorted(value,0)));if(b.length>256*1024)deny('ENVELOPE_LIMIT');return b;}
String digest(Json v)=>hash.sha256.convert(canonical(v)).toString();
List<int> _b64(String s,int n){final b=base64Decode(s);if(b.length!=n||base64Encode(b)!=s)deny('INVALID_SIGNATURE');return b;}
Future<void> _verify(String key,Json value,String sig)async{
 final valid=await Ed25519().verify(canonical(value),signature:Signature(_b64(sig,64),publicKey:SimplePublicKey(_b64(key,32),type:KeyPairType.ed25519)));
 if(!valid)deny('INVALID_SIGNATURE');
}
List<String> tokens(Object? v){if(v is! List||v.length>64||!v.every(token)||v.toSet().length!=v.length)deny('INVALID_TOKENS');return v.cast<String>();}
final class Trust{
 final String issuer,operatorKey,platform;
 final int epochFloor,now,hostApi;
 final Set<String> revokedKeys,revokedReleases;
 Trust({required this.issuer,required this.operatorKey,required this.platform,required this.epochFloor,required this.now,required this.hostApi,Set<String> revokedKeys=const{},Set<String> revokedReleases=const{}}):revokedKeys=Set.unmodifiable(revokedKeys),revokedReleases=Set.unmodifiable(revokedReleases);
}
final class VerifiedRelease{
 final String manifestId,extensionKey,version,releaseDigest,issuer,keyId;
 final int epoch,expiresAt;
 final Set<String> capabilities,permissions;
 final Json _contribution;
 Json get contribution=>jsonDecode(jsonEncode(_contribution)) as Json;
 VerifiedRelease._(this.manifestId,this.extensionKey,this.version,this.releaseDigest,this.issuer,this.keyId,this.epoch,this.expiresAt,this.capabilities,this.permissions,this._contribution);
}
Future<VerifiedRelease> verifyRelease({required Json envelope,required Json manifest,required String manifestId,required Json contribution,required Map<String,List<int>> assets,required Trust trust})async{
 exact(envelope,{'release','publisher_signature','review','operator_signature'});
 // Take immutable copies before async verification to avoid mutation races.
 final copy=jsonDecode(jsonEncode(envelope)) as Json;final m=jsonDecode(jsonEncode(manifest)) as Json;final c=jsonDecode(jsonEncode(contribution)) as Json;
 final release=copy['release'] as Json,review=copy['review'] as Json;
 exact(review,{'schema_version','issuer','catalog_epoch','issued_at','expires_at','publisher_key','publisher_key_id','release_digest','revoked'});
 exact(release,{'schema_version','publisher_id','extension_key','version','manifest_id','manifest_digest','host_api_min','host_api_max','platforms','capabilities','permissions','contribution_digest','assets','issued_at','key_id'});
 await _verify(trust.operatorKey,review,copy['operator_signature'] as String);
 final d=digest(release);
 if(review['schema_version']!=2||review['issuer']!=trust.issuer||review['revoked']!=false||review['release_digest']!=d||trust.revokedKeys.contains(review['publisher_key_id'])||trust.revokedReleases.contains(d)||(review['catalog_epoch'] as int)<trust.epochFloor||(review['issued_at'] as int)>trust.now||(review['expires_at'] as int)<=trust.now||(review['expires_at'] as int)-(review['issued_at'] as int)>86400)deny('REVIEW_DENIED');
 await _verify(review['publisher_key'] as String,release,copy['publisher_signature'] as String);
 final caps=tokens(release['capabilities']),perms=tokens(release['permissions']),platforms=tokens(release['platforms']);
 if(release['schema_version']!=2||release['key_id']!=review['publisher_key_id']||(release['issued_at'] as int)>(review['issued_at'] as int)||release['manifest_id']!=manifestId||!RegExp(r'^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$').hasMatch(manifestId)||manifestId=='00000000-0000-0000-0000-000000000000'||release['manifest_digest']!=digest(m)||release['contribution_digest']!=digest(c)||release['extension_key']!=m['extension_key']||release['version']!=m['version']||![release['publisher_id'],release['extension_key'],release['version'],release['key_id']].every(token)||!(release['extension_key'] as String).startsWith('partner_${release['publisher_id']}_')||trust.hostApi<(release['host_api_min'] as int)||trust.hostApi>(release['host_api_max'] as int)||!platforms.every((p)=>p=='android'||p=='ios')||!platforms.contains(trust.platform)||!perms.every((p)=>(m['permissions'] as List).contains(p)))deny('RELEASE_DENIED');
 final declarations=release['assets'] as List;if(declarations.length>64||declarations.length!=assets.length)deny('ASSET_COUNT');
 final seen=<String>{};var total=0;
 for(final a in declarations.cast<Json>()){exact(a,{'id','media_type','size','sha256'});final id=a['id'] as String;final bytes=assets[id];final size=a['size'] as int;
  if(!token(id)||!seen.add(id)||!['application/json','text/plain'].contains(a['media_type'])||bytes==null||bytes.length!=size||size<0)deny('ASSET_DENIED');
  total+=size;if(total>10*1024*1024||hash.sha256.convert(bytes).toString()!=a['sha256'])deny('ASSET_DIGEST');
 }
 return VerifiedRelease._(manifestId,release['extension_key'] as String,release['version'] as String,d,trust.issuer,release['key_id'] as String,review['catalog_epoch'] as int,review['expires_at'] as int,Set.unmodifiable(caps),Set.unmodifiable(perms),c);
}
