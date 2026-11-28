import 'dart:async';
import 'dart:convert';
import 'dart:math' as math;
import 'package:material_ui/material_ui.dart';
import 'package:maplibre_gl/maplibre_gl.dart';
import 'workspaces.dart';

const _source='local';
const _layers=['local-fill','local-line','local-point'];
const _empty=<String,dynamic>{'type':'FeatureCollection','features':[]};
const _style=<String,Object>{
  'version':8,
  'sources':{'local':{'type':'geojson','data':_empty},'draft':{'type':'geojson','data':_empty}},
  'layers':[
    {'id':'background','type':'background','paint':{'background-color':'#e9efed'}},
    {'id':'local-fill','type':'fill','source':'local','filter':['==',['geometry-type'],'Polygon'],'paint':{'fill-color':['case',['boolean',['feature-state','selected'],false],'#f57c00','#278876'],'fill-opacity':0.5}},
    {'id':'local-line','type':'line','source':'local','filter':['in',['geometry-type'],['literal',['LineString','Polygon']]],'paint':{'line-color':['case',['boolean',['feature-state','selected'],false],'#f57c00','#00695c'],'line-width':2}},
    {'id':'draft-line','type':'line','source':'draft','filter':['==',['geometry-type'],'LineString'],'paint':{'line-color':'#e65100','line-width':3}},
    {'id':'draft-point','type':'circle','source':'draft','filter':['==',['geometry-type'],'Point'],'paint':{'circle-color':'#e65100','circle-radius':5}},
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
  int _epoch=0,_seen=-1,_focusSeen=-1;
  String? _scope,_selected,_drawing;
  final List<LatLng> _vertices=[];
  bool _snapping=true,_editingBusy=false;
  int _draftEpoch=0;
  Timer? _timer;
  @override void initState(){super.initState();WidgetsBinding.instance.addObserver(this);widget.controller.addListener(_changed);}
  @override void dispose(){_epoch++;_timer?.cancel();widget.controller.removeListener(_changed);WidgetsBinding.instance.removeObserver(this);super.dispose();}
  @override void didChangeAppLifecycleState(AppLifecycleState state){
    _foreground=state==AppLifecycleState.resumed;_epoch++;
    if(_foreground){_schedule();}else{_timer?.cancel();}
  }
  void _changed(){
    final c=widget.controller;
    if(_focusSeen!=c.focusEpoch){
      _focusSeen=c.focusEpoch;
      final p=c.focusPoint;final map=_map;
      if(p!=null&&map!=null){unawaited(map.animateCamera(CameraUpdate.newLatLngZoom(LatLng(p[1],p[0]),16)).then<void>((_){}).catchError((Object _){}));}
    }
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
  Future<void> _preview()async{
    final map=_map;if(map==null||!_loaded)return;
    final coords=_vertices.map((v)=>[v.longitude,v.latitude]).toList();
    final features=<Map<String,dynamic>>[
      for(final p in coords){'type':'Feature','properties':{},'geometry':{'type':'Point','coordinates':p}},
      if(coords.length>=2){'type':'Feature','properties':{},'geometry':{'type':'LineString','coordinates':_drawing=='polygon'&&coords.length>=3?[...coords,coords.first]:coords}},
    ];
    await map.setGeoJsonSource('draft',{'type':'FeatureCollection','features':features});
  }
  Future<void> _addVertex(LatLng p)async{
    if(_editingBusy||_drawing==null||_vertices.length>=4095)return;
    final epoch=++_draftEpoch;final dataEpoch=widget.controller.dataEpoch;setState(()=>_editingBusy=true);
    try{
      if(_snapping){
        final zoom=_map?.cameraPosition?.zoom??18;
        final metres=(8*156543.03392804097*math.cos(p.latitude*math.pi/180)/math.pow(2,zoom)).clamp(0.1,100.0).toDouble();
        final raw=await widget.controller.local.snapLocal(x:p.longitude,y:p.latitude,toleranceM:metres);
        if(!mounted||epoch!=_draftEpoch||dataEpoch!=widget.controller.dataEpoch)return;
        final hit=jsonDecode(raw);
        if(hit is Map)p=LatLng((hit['y'] as num).toDouble(),(hit['x'] as num).toDouble());
      }
      if(!mounted||epoch!=_draftEpoch)return;
      setState((){if(_drawing=='point'){_vertices.clear();}_vertices.add(p);});
      await _preview();
    }catch(_){if(mounted)setState(()=>_failed=true);}
    finally{if(mounted)setState(()=>_editingBusy=false);}
  }
  Future<void> _endDraft(bool save)async{
    if(_editingBusy)return;
    final kind=_drawing;if(kind==null)return;
    if(save){
      setState(()=>_editingBusy=true);
      try{await widget.controller.draw(kind,[for(final p in _vertices)...[p.longitude,p.latitude]],{});}
      catch(_){if(mounted){setState(()=>_editingBusy=false);ScaffoldMessenger.of(context).showSnackBar(const SnackBar(content:Text('Invalid geometry or attribute constraints')));}return;}
    }
    if(!mounted)return;
    _draftEpoch++;setState((){_drawing=null;_vertices.clear();_editingBusy=false;});
    try{await _preview();}catch(_){}
  }
  Future<void> _startDraft()async{
    final vi=widget.controller.vi;
    final kind=await showModalBottomSheet<String>(context:context,builder:(sheet)=>SafeArea(child:Column(mainAxisSize:MainAxisSize.min,children:[
      for(final item in ['point','line','polygon'])ListTile(title:Text(item=='point'?(vi?'Điểm':'Point'):item=='line'?(vi?'Đường':'Line'):(vi?'Vùng':'Polygon')),onTap:()=>Navigator.pop(sheet,item)),
      ListTile(title:Text(vi?'Undo / Hoàn tác':'Undo'),onTap:(){Navigator.pop(sheet);unawaited(widget.controller.history(false));}),
      ListTile(title:Text(vi?'Redo / Làm lại':'Redo'),onTap:(){Navigator.pop(sheet);unawaited(widget.controller.history(true));}),
    ])));
    if(kind!=null&&mounted){setState(()=>_drawing=kind);}
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
        trackCameraPosition:true,
        onMapClick:(p,coord)=>unawaited(_drawing==null?_tap(p):_addVertex(coord)),
      )),
      Positioned(top:8,right:8,child:SafeArea(child:Card(child:IconButton(
        tooltip:vi?'Hiện/ẩn lớp local':'Toggle local layer',
        icon:Icon(_visible?Icons.layers:Icons.layers_clear),
        onPressed:(){setState(()=>_visible=!_visible);_schedule();}
      )))),
      Positioned(top:64,right:8,child:SafeArea(child:Card(child:IconButton(tooltip:vi?'Vẽ / Chỉnh sửa':'Draw / Edit',icon:const Icon(Icons.edit),onPressed:widget.controller.ready&&!widget.controller.busy&&_drawing==null?_startDraft:null)))),
      if(_drawing!=null) Positioned(top:120,right:8,child:SafeArea(child:Card(child:Column(mainAxisSize:MainAxisSize.min,children:[
        IconButton(tooltip:'Snap',icon:Icon(_snapping?Icons.my_location:Icons.location_searching),onPressed:_editingBusy?null:()=>setState(()=>_snapping=!_snapping)),
        IconButton(tooltip:'Remove vertex',icon:const Icon(Icons.undo),onPressed:_editingBusy||_vertices.isEmpty?null:(){setState(()=>_vertices.removeLast());unawaited(_preview());}),
        IconButton(tooltip:vi?'Lưu':'Commit',icon:const Icon(Icons.check),onPressed:_editingBusy?null:()=>_endDraft(true)),
        IconButton(tooltip:vi?'Hủy':'Cancel',icon:const Icon(Icons.close),onPressed:_editingBusy?null:()=>_endDraft(false)),
      ]))))),
      Positioned(bottom:12,left:12,right:80,child:SafeArea(child:Card(child:Padding(
        padding:const EdgeInsets.all(8),
        child:Text(_failed?(vi?'Lỗi tải lớp local':'Local layer unavailable'):_truncated?(vi?'Giới hạn hiển thị · phóng to để xem thêm':'Display limit · zoom in for more'):_selected!=null?'ID: ${_selected!}':(vi?'Nguồn: kho GeoPackage local':'Source: local GeoPackage')),
      )))),
    ]);
  }
}
