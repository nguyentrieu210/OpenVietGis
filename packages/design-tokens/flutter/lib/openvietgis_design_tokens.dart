// Generated from tokens.json; do not edit.
import 'package:flutter/material.dart';

enum OvgDensity { compact, comfortable, touch }

class OvgTokens {
  OvgTokens._();
  static const Map<String, Color> light = {
    'surface': Color(0xffffffff),
    'panel': Color(0xfff2f5f6),
    'ink': Color(0xff263c49),
    'muted': Color(0xff526b79),
    'line': Color(0xffccd8df),
    'primary': Color(0xff1565a9),
    'on-primary': Color(0xffffffff),
    'focus': Color(0xff1565a9),
    'line-strong': Color(0xff64748b),
    'danger': Color(0xffb91c1c),
    'success': Color(0xff166534),
    'warning': Color(0xff92400e),
  };
  static const Map<String, Color> dark = {
    'surface': Color(0xff202b34),
    'panel': Color(0xff26343e),
    'ink': Color(0xffe2eaf0),
    'muted': Color(0xffaebfca),
    'line': Color(0xff425461),
    'primary': Color(0xff62a9e5),
    'on-primary': Color(0xff10202c),
    'focus': Color(0xff62a9e5),
    'line-strong': Color(0xff94a3b8),
    'danger': Color(0xfffca5a5),
    'success': Color(0xff86efac),
    'warning': Color(0xfffcd34d),
  };
  static const Map<String, double> compact = {
    'gap': 8,
    'inset': 16,
    'section-gap': 16,
    'field-gap': 6,
    'control-height': 32,
    'toolbar-gap': 4,
    'touch-target': 32,
  };
  static const Map<String, double> comfortable = {
    'gap': 12,
    'inset': 20,
    'section-gap': 20,
    'field-gap': 8,
    'control-height': 40,
    'toolbar-gap': 8,
    'touch-target': 40,
  };
  static const Map<String, double> touch = {
    'gap': 12,
    'inset': 20,
    'section-gap': 24,
    'field-gap': 10,
    'control-height': 48,
    'toolbar-gap': 8,
    'touch-target': 48,
  };
  static const double radiusUi = 4;
  static const double radiusSm = 4;
  static const double radiusMd = 8;
  static const double radiusLg = 12;
  static const double textXs = 12;
  static const double textSm = 13;
  static const double textBody = 14;
  static const double textHeading = 18;
  static const Duration motionFast = Duration(milliseconds: 100);
  static const Duration motionNormal = Duration(milliseconds: 180);
  static const Duration motionSlow = Duration(milliseconds: 260);
  static const Duration motionReduced = Duration(milliseconds: 0);
  static const List<String> fontFamilies = ['Be Vietnam Pro','Inter','system-ui','sans-serif'];
  static Map<String, double> spacing(OvgDensity density) => switch (density) {
    OvgDensity.compact => compact,
    OvgDensity.comfortable => comfortable,
    OvgDensity.touch => touch,
  };
  static Duration motion(Duration value, {required bool reduced}) => reduced ? motionReduced : value;
  static ThemeData theme(Brightness brightness, {OvgDensity density = OvgDensity.touch}) {
    final c = brightness == Brightness.dark ? dark : light;
    final d = spacing(density);
    final scheme = ColorScheme.fromSeed(seedColor: c['primary']!, brightness: brightness).copyWith(
      primary: c['primary'], onPrimary: c['on-primary'], surface: c['surface'], onSurface: c['ink'],
      onSurfaceVariant: c['muted'], outline: c['line-strong'], outlineVariant: c['line'], error: c['danger'],
    );
    final minimum = Size(d['touch-target']!, d['control-height']!);
    final shape = RoundedRectangleBorder(borderRadius: BorderRadius.circular(radiusUi));
    final base = ThemeData(useMaterial3: true, colorScheme: scheme, scaffoldBackgroundColor: c['surface'],
      materialTapTargetSize: MaterialTapTargetSize.padded,
      filledButtonTheme: FilledButtonThemeData(style: FilledButton.styleFrom(minimumSize: minimum, shape: shape)),
      outlinedButtonTheme: OutlinedButtonThemeData(style: OutlinedButton.styleFrom(minimumSize: minimum, shape: shape)),
      textButtonTheme: TextButtonThemeData(style: TextButton.styleFrom(minimumSize: minimum, shape: shape)),
      inputDecorationTheme: InputDecorationTheme(border: OutlineInputBorder(borderRadius: BorderRadius.circular(radiusUi)), contentPadding: EdgeInsets.all(d['gap']!)),
    );
    return base.copyWith(textTheme: base.textTheme.copyWith(bodyMedium: base.textTheme.bodyMedium?.copyWith(fontSize: textBody), titleMedium: base.textTheme.titleMedium?.copyWith(fontSize: textHeading)));
  }
}
