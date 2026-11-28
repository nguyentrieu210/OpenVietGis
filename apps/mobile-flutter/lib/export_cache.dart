import 'dart:io';

/// App-owned export cache. Never follows symlinks or touches workspace storage.
const exportPrefix = 'ovg-export-';
const exportRetention = Duration(hours: 24);

Future<void> pruneExportCache(Directory root, {DateTime? now}) async {
  final cutoff = (now ?? DateTime.now()).subtract(exportRetention);
  await for (final entry in root.list(followLinks: false)) {
    final name = entry.path.split(Platform.pathSeparator).last;
    if (entry is! Directory || !name.startsWith(exportPrefix)) continue;
    try {
      final type = await FileSystemEntity.type(entry.path, followLinks: false);
      if (type != FileSystemEntityType.directory) continue;
      // Only prune directories created by the exporter and older than retention.
      final stat = await entry.stat();
      if (stat.modified.isBefore(cutoff)) {
        await entry.delete(recursive: true);
      }
    } on FileSystemException {
      // An inaccessible or already removed cache entry is not a workspace error.
    }
  }
}

/// Stage a complete file before handing its path to the operating system.
Future<File> stageGeojsonExport(Directory root, String json) async {
  final directory = await root.createTemp(exportPrefix);
  try {
    final staged = File('${directory.path}${Platform.pathSeparator}workspace.part');
    await staged.writeAsString(json, flush: true);
    return await staged.rename('${directory.path}${Platform.pathSeparator}workspace.geojson');
  } catch (_) {
    try {
      await directory.delete(recursive: true);
    } on FileSystemException {
      // Best-effort cleanup; stale cache is pruned on the next launch.
    }
    rethrow;
  }
}
