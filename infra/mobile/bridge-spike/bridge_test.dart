import 'dart:typed_data';
import 'package:flutter_test/flutter_test.dart';
import 'package:ovg_bridge_spike/src/rust/api/simple.dart';
import 'package:ovg_bridge_spike/src/rust/frb_generated.dart';

void main() {
  setUpAll(() async { await RustLib.init(); });
  tearDownAll(() { RustLib.dispose(); });
  test('generated bridge calls real shared Rust CRS kernel', () async {
    final result = await transformBatch(sourceEpsg: 4326, targetEpsg: 3857,
        xy: Float64List.fromList([1.0, 0.0]));
    expect(result[0], closeTo(111319.49079327357, 0.001));
    expect(result[1], closeTo(0, 0.001));
  });
  test('unsupported CRS and oversized batches fail through generated bridge', () async {
    await expectLater(transformBatch(sourceEpsg: 4326, targetEpsg: 999999,
        xy: Float64List.fromList([1.0, 0.0])), throwsA(anything));
    await expectLater(transformBatch(sourceEpsg: 4326, targetEpsg: 3857,
        xy: Float64List(8194)), throwsA(anything));
  });
}
