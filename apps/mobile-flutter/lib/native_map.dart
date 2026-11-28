import 'dart:async';
import 'dart:convert';
import 'package:material_ui/material_ui.dart';
import 'package:maplibre_gl/maplibre_gl.dart';
import 'workspaces.dart';

const _source='local';
const _layers=['local-fill','local-line','local-point'];
const _empty=<String,dynamic>{'type':'FeatureCollection','features':[]};
const _style=<String,Object>{
  'version':8,
  'sources':{'local':{'type':'geojson','data':_empty}},
  'layers':[
    {'id':'background','type':'background','paint':{'background-color':'#e9efed'}},
    {'id':'local-fill','type':'fill','source':'local','filter':['==',['geometry-type'],'Polygon'],'paint':{'fill-color':['case',['boolean',['feature-state','selected'],false],'#f57c00','#278876'],'fill-opacity':0.5}},
    {'id':'local-line','type':'line','source':'local','filter':['in',['geometry-type'],['literal',['LineString','Polygon']]],'paint':{'line-color':['case',['boolean',['feature-state','selected'],false],'#f57c00','#00695c'],'line-width':2}},
    {'id':'local-point','type':'circle','source':'local','filter':['==',['geometry-type'],'Point'],'paint':{'circle-color':['case',['boolean',['feature-state','selected'],false],'#f57c00','#00695c'],'circle-radius':6}},
  ]
};
class NativeLocalMap extends StatefulWidget{
  final WorkspaceController controller;
  const NativeLocalMap({super.key,required this.controller});
  @override State<NativeLocalMap> createState()=>_NativeLocalMapState();
}
class _NativeLocalMapState extends State<NativeLocalMap> with WidgetsBindingObserver{
  MapLibreMapController? _map;
  bool _loaded=false,_visible=true,_running=false,_pending=false,_truncated=false,_failed=false,_foreground=true;
  int _epoch=0,_seen=-1;
  String? _scope,_selected;
  Timer? _timer;
  @override void initState(){super.initState();WidgetsBinding.instance.addObserver(this);widget.controller.addListener(_changed);}
  @override void dispose(){_epoch++;_timer?.cancel();widget.controller.removeListener(_changed);WidgetsBinding.instance.removeObserver(this);super.dispose();}
  @override void didChangeAppLifecycleState(AppLifecycleState state){
    _foreground=state==AppLifecycleState.resumed;_epoch++;
    if(_foreground)_schedule();else _timer?.cancel();
  }
  void _changed(){
    final c=widget.controller;
    if(_scope==c.selected&&_seen==c.dataEpoch)return;
    _scope=c.selected;_seen=c.dataEpoch;_selected=null;_epoch++;
    _schedule();
  }
  void _schedule(){
    _epoch++;_timer?.cancel();
    if(!_loaded||!_foreground)return;
    _timer=Timer(const Duration(milliseconds:120),(){_pending=true;unawaited(_drain());});
  }
  Future<void> _drain()async{
    if(_running)return;_running=true;
    try{
      while(mounted&&_pending&&_loaded&&_foreground){
        _pending=false;final epoch=_epoch;final map=_map;if(map==null)break;
        try{
          // A new workspace gets a new platform view. Hide explicitly, otherwise retain the previous same-scope frame.
          if(!_visible||!widget.controller.ready){await map.setGeoJsonSource(_source,_empty);continue;}
          if(widget.controller.busy)continue;
          final bounds=await map.getVisibleRegion();
          if(bounds.southwest.longitude>bounds.northeast.longitude){await map.setGeoJsonSource(_source,_empty);if(mounted)setState(()=>_truncated=true);continue;}
          final raw=await widget.controller.viewport(bounds.southwest.longitude,bounds.southwest.latitude,bounds.northeast.longitude,bounds.northeast.latitude);
          if(!mounted||epoch!=_epoch)continue;
          final v=jsonDecode(raw) as Map<String,dynamic>;
          await map.setGeoJsonSource(_source,v['data'] as Map<String,dynamic>);
          if(!mounted||epoch!=_epoch)continue;
          if(_selected!=null)await map.setFeatureState(_source,_selected!,{'selected':true});
          if(mounted&&epoch==_epoch)setState((){_truncated=v['truncated']==true;_failed=false;});
        }catch(_){if(mounted&&epoch==_epoch)setState(()=>_failed=true);}
      }
    }finally{_running=false;}
  }
  Future<void> _tap(dynamic point)async{
    final map=_map;final epoch=_epoch;if(map==null||!_visible||!_loaded)return;
    try{
      final hits=await map.queryRenderedFeatures(point,_layers,null);
      if(!mounted||epoch!=_epoch)return;
      final old=_selected;
      if(old!=null)await map.removeFeatureState(_source,featureId:old);
      if(!mounted||epoch!=_epoch)return;
      final id=hits.isEmpty?null:(hits.first as Map)['id']?.toString();
      _selected=id;
      if(id!=null)await map.setFeatureState(_source,id,{'selected':true});
      if(mounted)setState((){});
    }catch(_){if(mounted)setState(()=>_failed=true);}
  }
  @override Widget build(BuildContext context){
    final vi=widget.controller.vi;
    return Stack(children:[
      Positioned.fill(child:MapLibreMap(
        styleString:jsonEncode(_style),
        initialCameraPosition:const CameraPosition(target:LatLng(10.8,106.7),zoom:4),
        onMapCreated:(map)=>_map=map,
        onStyleLoadedCallback:(){_loaded=true;_schedule();},
        onCameraIdle:_schedule,
        onMapClick:(p,_)=>unawaited(_tap(p)),
      )),
      Positioned(top:8,right:8,child:SafeArea(child:Card(child:IconButton(
        tooltip:vi?'Hiện/ẩn lớp local':'Toggle local layer',
        icon:Icon(_visible?Icons.layers:Icons.layers_clear),
        onPressed:(){setState(()=>_visible=!_visible);_schedule();}
      )))),
      Positioned(bottom:12,left:12,right:80,child:SafeArea(child:Card(child:Padding(
        padding:const EdgeInsets.all(8),
        child:Text(_failed?(vi?'Lỗi tải lớp local':'Local layer unavailable'):_truncated?(vi?'Giới hạn hiển thị · phóng to để xem thêm':'Display limit · zoom in for more'):_selected!=null?'ID: ${_selected!}':(vi?'Nguồn: kho GeoPackage local':'Source: local GeoPackage')),
      )))),
    ]);
  }
}
