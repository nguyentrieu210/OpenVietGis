import 'package:test/test.dart';
import 'package:openvietgis_house_numbering_mobile/house_numbering_mobile.dart';
import 'package:openvietgis_plugin_sdk/openvietgis_plugin_sdk.dart';
const id='12345678-1234-4234-8234-123456789abc';
const other='22345678-1234-4234-8234-123456789abc';
HnScope scope({String project=id,bool manage=false,bool active=true})=>HnScope(issuer:'server',organization:id,project:project,subject:id,session:id,manifestId:id,version:'1.0.0',active:active,online:true,leaseValid:true,permissions:{HnPermission.read,HnPermission.edit,if(manage)HnPermission.manage});
void main(){
 test('canonical API and immutable survey identifiers',()async{
  var s=scope();Json? body;HnCommand? sent;
  final h=HouseNumbering(current:()=>s,transport:(c,q,b,r)async{sent=c;body=b;return {'applied':[id],'conflicts':[]};},photo:(_,_)async=>{'id':id},confirm:(_)async=>true);
  final draft=SurveyDraft(clientUuid:id,buildingId:other,condition:'house',longitude:106.7,latitude:10.8,accuracy:3);
  expect((await h.submitSurvey(draft))['applied'],[id]);expect(sent,HnCommand.syncSurvey);
  expect(body!['operations'][0],draft.toWire());expect(body!['operations'][0]['crs_code'],4326);
  expect(HnRoute.resolve(HnCommand.syncSurvey,s).path,'/api/v1/organizations/$id/projects/$id/modules/house-numbering/survey/sync');
  await expectLater(h.transition(id,1,'approve'),throwsStateError);s=scope(manage:true);await h.transition(id,1,'approve');expect(body,{'expected_revision':1,'action':'approve'});
  s=scope(project:other);await expectLater(h.routes(),throwsStateError);h.close();
 });
 test('photo consent and review permission',()async{
  var uploaded=false;final h=HouseNumbering(current:()=>scope(),transport:(_,_,_,_)async=>{},photo:(_,_)async{uploaded=true;return {'id':id};},confirm:(_)async=>false);
  await expectLater(h.uploadPhoto([1,2],'image/jpeg'),throwsStateError);expect(uploaded,false);
  await expectLater(h.reviewSurvey(id,1,'accepted'),throwsStateError);
  expect(()=>SurveyDraft(clientUuid:id,buildingId:id,condition:'house',longitude:1),throwsStateError);
 });
}
