import 'package:material_ui/material_ui.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:openvietgis_ui/openvietgis_ui.dart';
void main(){
 test('native host theme is generated from shared token source',(){
  expect(ovgNativeTheme(Brightness.light).colorScheme.primary,OvgTokens.light['primary']);
  expect(ovgNativeTheme(Brightness.dark).colorScheme.primary,OvgTokens.dark['primary']);
  expect(ovgNativeTheme(Brightness.light).filledButtonTheme.style!.minimumSize!.resolve({})!.height,48);
 });
}
