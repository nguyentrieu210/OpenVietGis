import 'dart:typed_data';
import 'package:material_ui/material_ui.dart';
import 'package:ovg_bridge_spike/src/rust/api/simple.dart';
import 'package:ovg_bridge_spike/src/rust/frb_generated.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  await RustLib.init();
  runApp(MaterialApp(home: Scaffold(
    appBar: AppBar(title: const Text('OpenVietGIS portable kernel qualification')),
    body: FutureBuilder<List<double>>(
      future: transformBatch(sourceEpsg: 4326, targetEpsg: 3857,
          xy: Float64List.fromList([1.0, 0.0])),
      builder: (context, snapshot) {
        if (snapshot.hasError) return Text('Native probe failed: ${snapshot.error}');
        if (!snapshot.hasData) return const CircularProgressIndicator();
        return Text('Native EPSG:3857 X = ${snapshot.data![0]} m');
      },
    ),
  )));
}
