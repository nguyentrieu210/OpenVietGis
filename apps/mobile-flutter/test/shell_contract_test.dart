import 'dart:convert';
import 'package:flutter_test/flutter_test.dart';
import 'package:openvietgis_mobile/main.dart';
void main(){
  test('fresh offline map is empty and has no remote dependencies',(){
    expect(offlineStyle['sources'],isEmpty);
    expect(jsonEncode(offlineStyle),isNot(contains('http')));
    expect(tr(true,'Độc lập','Standalone'),'Độc lập');
    expect(tr(false,'Độc lập','Standalone'),'Standalone');
  });
}
