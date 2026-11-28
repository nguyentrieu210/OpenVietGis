import 'dart:convert';
import 'package:material_ui/material_ui.dart';
import 'package:maplibre_gl/maplibre_gl.dart';

const offlineStyle = <String, Object>{
  'version': 8,
  'sources': {
    'qualification': {
      'type': 'geojson',
      'data': {
        'type': 'FeatureCollection',
        'features': [
          {'type': 'Feature', 'id': 1, 'properties': {'label': 'Control point'},
           'geometry': {'type': 'Point', 'coordinates': [106.7, 10.8]}}
        ]
      }
    }
  },
  'layers': [
    {'id': 'background', 'type': 'background', 'paint': {'background-color': '#dce7e9'}},
    {'id': 'qualification', 'type': 'circle', 'source': 'qualification',
     'paint': {'circle-radius': 9, 'circle-color': '#00665c'}}
  ]
};
void main() => runApp(const MaterialApp(home: NativeMapProbe()));
class NativeMapProbe extends StatefulWidget {
  const NativeMapProbe({super.key});
  @override
  State<NativeMapProbe> createState() => _NativeMapProbeState();
}
class _NativeMapProbeState extends State<NativeMapProbe> {
  bool loaded = false;
  @override
  Widget build(BuildContext context) => Scaffold(
    appBar: AppBar(title: Text(loaded ? 'Offline style loaded' : 'Loading native map')),
    body: MapLibreMap(
      initialCameraPosition: const CameraPosition(target: LatLng(10.8, 106.7), zoom: 12),
      styleString: jsonEncode(offlineStyle),
      onStyleLoadedCallback: () { if (mounted) setState(() => loaded = true); },
    ),
  );
}
