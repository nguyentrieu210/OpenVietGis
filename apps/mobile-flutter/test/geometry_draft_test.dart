import 'package:flutter_test/flutter_test.dart';
import '../lib/geometry_draft.dart';

void main(){
 test('multipart edit preserves other polygons and holes',(){
  final g=<String,dynamic>{'type':'MultiPolygon','coordinates':[
   [[[106,11],[107,11],[107,12],[106,11]],[[106.2,11.1],[106.4,11.1],[106.4,11.2],[106.2,11.1]]],
   [[[108,11],[109,11],[109,12],[108,11]]]
  ]};
  final d=GeometryDraft.parse(g);
  expect(d.paths.length,3);expect(d.polygonRingCounts,[2,1]);
  d.paths[1][0]=[106.25,11.1];
  final result=d.toGeoJson();
  expect(result['type'],'MultiPolygon');
  expect(result['coordinates'][0][0],g['coordinates'][0][0]);
  expect(result['coordinates'][0][1].first,result['coordinates'][0][1].last);
  expect(result['coordinates'][1],g['coordinates'][1]);
 });
 test('rejects open ring and dimensional loss',(){
  expect(()=>GeometryDraft.parse({'type':'Polygon','coordinates':[[[1,1],[2,1],[2,2],[1,2]]]}),throwsFormatException);
  expect(()=>GeometryDraft.parse({'type':'Point','coordinates':[1,2,3]}),throwsFormatException);
 });
 test('multiline keeps path boundaries',(){
  final d=GeometryDraft.parse({'type':'MultiLineString','coordinates':[[[1,2],[3,4]],[[5,6],[7,8]]]});
  d.paths[0].insert(1,[2,3]);
  expect(d.toGeoJson()['coordinates'],[[[1,2],[2,3],[3,4]],[[5,6],[7,8]]]);
 });
}
