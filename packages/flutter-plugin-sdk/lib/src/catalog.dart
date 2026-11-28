import 'release.dart';
enum InstallState{notInstalled,disabled,incompatible,expired,revoked,permissionDenied,ready}
/// View of existing registry, not a new marketplace. Host must obtain the
/// actual server activation and trust state. Disable never deletes GIS data.
InstallState installState({required VerifiedRelease? release,required String issuer,required String? activeManifest,required int epoch,required int now,required bool compatible,required bool revoked,required bool entitled,required bool permitted,required bool enabled}){
 if(revoked)return InstallState.revoked;
 if(release==null)return InstallState.notInstalled;
 if(release.issuer!=issuer||release.epoch!=epoch||now>=release.expiresAt)return InstallState.expired;
 if(!compatible)return InstallState.incompatible;
 if(!entitled||!permitted)return InstallState.permissionDenied;
 if(!enabled||activeManifest!=release.manifestId)return InstallState.disabled;
 return InstallState.ready;
}
