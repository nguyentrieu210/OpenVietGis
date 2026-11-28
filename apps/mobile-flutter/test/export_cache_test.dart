import 'dart:io';
import 'package:flutter_test/flutter_test.dart';
import 'package:openvietgis_mobile/export_cache.dart';

void main() {
  late Directory root;
  setUp(() async {
    root = await Directory.systemTemp.createTemp('ovg-export-test-');
  });
  tearDown(() async {
    await root.delete(recursive: true);
  });

  test('staging publishes complete GeoJSON and no partial file', () async {
    const json = '{"type":"FeatureCollection","features":[]}';
    final result = await stageGeojsonExport(root, json);
    expect(await result.readAsString(), json);
    expect(result.path.endsWith('workspace.geojson'), isTrue);
    expect(await result.parent.list().length, 1);
  });

  test('cleanup preserves fresh exports and unrelated directories', () async {
    final result = await stageGeojsonExport(root, '{}');
    final unrelated = await root.createTemp('workspace-');
    final original = await File('${unrelated.path}/data').writeAsString('keep');
    final now = DateTime.now();
    await pruneExportCache(root, now: now);
    expect(await result.exists(), isTrue);
    await pruneExportCache(root, now: now.add(const Duration(hours: 25)));
    expect(await result.parent.exists(), isFalse);
    expect(await original.readAsString(), 'keep');
  });

  test('cleanup ignores a symlink masquerading as an export directory', () async {
    final target = await root.createTemp('workspace-');
    final original = await File('${target.path}/data').writeAsString('keep');
    final link = await Link('${root.path}/ovg-export-link').create(target.path);
    await pruneExportCache(root, now: DateTime.now().add(const Duration(days: 2)));
    expect(await link.exists(), isTrue);
    expect(await original.readAsString(), 'keep');
  }, skip: Platform.isWindows);
}
