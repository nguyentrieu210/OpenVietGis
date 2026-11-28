import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:openvietgis_design_tokens/openvietgis_design_tokens.dart';
void main() {
  test('light and dark match semantic colors', () {
    for (final b in Brightness.values) {
      final theme = OvgTokens.theme(b);
      final c = b == Brightness.dark ? OvgTokens.dark : OvgTokens.light;
      expect(theme.colorScheme.primary, c['primary']);
      expect(theme.colorScheme.onPrimary, c['on-primary']);
      expect(theme.colorScheme.surface, c['surface']);
      expect(theme.brightness, b);
      expect(theme.filledButtonTheme.style!.minimumSize!.resolve({})!.height, 48);
    }
  });
  test('density and reduced motion', () {
    expect(OvgTokens.spacing(OvgDensity.compact)['control-height'], 32);
    expect(OvgTokens.spacing(OvgDensity.touch)['touch-target'], 48);
    expect(OvgTokens.motion(OvgTokens.motionNormal, reduced: true), Duration.zero);
  });
}
