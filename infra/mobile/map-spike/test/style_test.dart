import 'dart:convert';
import 'package:flutter_test/flutter_test.dart';
import 'package:ovg_map_spike/main.dart';
void main() {
  test('fixture is an inline GeoJSON source with no network dependencies', () {
    final encoded = jsonEncode(offlineStyle);
    expect(encoded, isNot(contains('http')));
    expect(encoded, contains('FeatureCollection'));
    expect(encoded, contains('[106.7,10.8]'));
  });
}
