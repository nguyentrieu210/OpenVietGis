/// Explicit HTTPS origin and server/account/project scope.
final class Scope {
  final Uri origin;
  final String issuer, subject, organization, project, session;
  Scope({required String origin,required this.issuer,required this.subject,
    required this.organization,required this.project,required this.session})
      : origin=Uri.parse(origin) {
    if(this.origin.scheme!='https'||this.origin.host.isEmpty||
      this.origin.userInfo.isNotEmpty||this.origin.hasQuery||
      this.origin.hasFragment||this.origin.origin!=origin||
      issuer.isEmpty||issuer.length>256||
      ![subject,organization,project,session].every(isUuid)){
      throw ArgumentError('Invalid scope');
    }
  }
  static bool isUuid(String s)=>RegExp(r'^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$').hasMatch(s)&&s!='00000000-0000-0000-0000-000000000000';
  Uri managed(List<String> path,[Map<String,String>? query])=>origin.replace(
      pathSegments:['api','v1','organizations',organization,'projects',project,'integration-hub','managed',...path],
      queryParameters:query);
  bool same(Scope other)=>origin==other.origin&&issuer==other.issuer&&subject==other.subject&&organization==other.organization&&project==other.project&&session==other.session;
}
