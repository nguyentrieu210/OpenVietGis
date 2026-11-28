import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'package:material_ui/material_ui.dart';
import 'package:maplibre_gl/maplibre_gl.dart';
import 'package:path_provider/path_provider.dart';
import 'src/rust/api/simple.dart';

const _background='{"version":8,"sources":{},"layers":[{"id":"background","type":"background","paint":{"background-color":"#e9efed"}}]}';
class _Lease{
 final String id,source;
 final List<String> renderLayers=[];
 bool sourceAdded=false;
 _Lease(this.id):source='mvt-$id';
}
/// Opt-in native MVT viewer: camera-idle prefetch, bounded archive lease,
/// MapLibre reads PBF directly from a local MBTiles file, no loopback server.
class UnifiedMvtMap extends StatefulWidget{
 final NativeUnifiedProject project;
 final String revision;
 final List<Map<String,dynamic>> layers;
 final String initialLayer;
 const UnifiedMvtMap({super.key,required this.project,required this.revision,required this.layers,required this.initialLayer});
 @override State<UnifiedMvtMap> createState()=>_UnifiedMvtMapState();
}
class _UnifiedMvtMapState extends State<UnifiedMvtMap> with WidgetsBindingObserver{
 MapLibreMapController? _map;
 late Set<String> _visible;
 _Lease? _current;
 bool _ready=false,_running=false,_pending=false,_foreground=true,_fatal=false;
 int _epoch=0;
 Timer? _timer;
 String? _lastKey,_error,_info;
 static const _palette=['#00695c','#315b9b','#9b4c80','#927017','#4b768e','#99533a','#6555a5','#627b36'];
 @override void initState(){super.initState();_visible={widget.initialLayer};WidgetsBinding.instance.addObserver(this);}
 @override void dispose(){_epoch++;_timer?.cancel();WidgetsBinding.instance.removeObserver(this);final old=_current;if(old!=null)unawaited(widget.project.releaseTileArchive(lease:old.id).catchError((Object _){}));super.dispose();}
 @override void didChangeAppLifecycleState(AppLifecycleState state){_foreground=state==AppLifecycleState.resumed;_epoch++;_timer?.cancel();if(_foreground)_schedule();}
 void _schedule(){_epoch++;_timer?.cancel();if(!_ready||!_foreground||_fatal)return;_timer=Timer(const Duration(milliseconds:250),(){_pending=true;unawaited(_drain());});}
 Future<void> _remove(MapLibreMapController map,_Lease lease)async{
  // Remove native style references before releasing the underlying file.
  for(final l in lease.renderLayers.reversed){await map.removeLayer(l);}
  lease.renderLayers.clear();
  if(lease.sourceAdded){await map.removeSource(lease.source);lease.sourceAdded=false;}
  await widget.project.releaseTileArchive(lease:lease.id);
 }
 Future<void> _drain()async{
  if(_running)return;_running=true;if(mounted)setState((){});
  try{
   while(mounted&&_pending&&_foreground&&!_fatal){
    _pending=false;final epoch=_epoch,map=_map;
    if(map==null)break;
    _Lease? next;
    try{
     final b=await map.getVisibleRegion();
     final z=(map.cameraPosition?.zoom??8).floor().clamp(0,22).toInt();
     final west=b.southwest.longitude.clamp(-180.0,180.0).toDouble();
     final south=b.southwest.latitude.clamp(-85.05112878,85.05112878).toDouble();
     final east=b.northeast.longitude.clamp(-180.0,180.0).toDouble();
     final north=b.northeast.latitude.clamp(-85.05112878,85.05112878).toDouble();
     if(west>east)throw StateError('Khung nhìn qua kinh tuyến 180° chưa hỗ trợ.');
     final ls=_visible.toList()..sort();
     final key=jsonEncode([z,west,south,east,north,ls]);
     if(key==_lastKey)continue;
     final cache=await getApplicationCacheDirectory();
     if(!mounted||epoch!=_epoch)continue;
     final raw=await widget.project.tileArchive(layers:ls,west:west,south:south,east:east,north:north,z:z,expected:widget.revision,cacheRoot:'${cache.path}${Platform.pathSeparator}unified-mvt');
     final res=jsonDecode(raw) as Map<String,dynamic>;final arc=res['archive'] as Map<String,dynamic>;
     next=_Lease(res['lease'].toString());
     if(!mounted||epoch!=_epoch){await widget.project.releaseTileArchive(lease:next.id);next=null;continue;}
     final path=arc['path'].toString();
     if(!path.startsWith('/')||!path.endsWith('/viewport.mbtiles')||path.contains('?')||path.contains('#'))throw StateError('INVALID_ARCHIVE_PATH');
     await map.addSource(next.source,VectorSourceProperties(url:'mbtiles://$path'));
     next.sourceAdded=true;
     for(final l in ls){
      final color=_palette[widget.layers.indexWhere((v)=>v['id']==l).clamp(0,255).toInt()%_palette.length];
      final prefix='${next.source}-$l';
      await map.addFillLayer(next.source,'$prefix-fill',FillLayerProperties(fillColor:color,fillOpacity:0.4),sourceLayer:l,filter:['==',['geometry-type'],'Polygon']);
      next.renderLayers.add('$prefix-fill');
      await map.addLineLayer(next.source,'$prefix-line',LineLayerProperties(lineColor:color,lineWidth:2),sourceLayer:l,filter:['in',['geometry-type'],['literal',['LineString','Polygon']]]);
      next.renderLayers.add('$prefix-line');
      await map.addCircleLayer(next.source,'$prefix-point',CircleLayerProperties(circleColor:color,circleRadius:5),sourceLayer:l,filter:['==',['geometry-type'],'Point']);
      next.renderLayers.add('$prefix-point');
     }
     final old=_current;_current=next;next=null;
     if(old!=null){try{await _remove(map,old);}catch(e){_fatal=true;rethrow;}}
     if(mounted){setState((){_lastKey=key;_error=null;_info='Zoom $z · ${arc["tiles"]} tile · ${arc["bytes"]} byte nén';});}
    }catch(e){
     if(next!=null){try{await _remove(map,next);}catch(_){_fatal=true;}}
     // Existing tiles may be outside the new viewport; label stale rather than
     // presenting partial coverage as complete.
     if(mounted)setState(()=>_error='Chưa cập nhật khung nhìn: $e. Phóng to hoặc giảm số lớp.');
    }
   }
  }finally{_running=false;if(mounted)setState((){});}
 }
 Future<void> _selectLayers()async{
  final selected=Set<String>.from(_visible);
  final ok=await showDialog<bool>(context:context,builder:(d)=>StatefulBuilder(builder:(d,update)=>AlertDialog(
   title:const Text('Lớp MVT (tối đa 8)'),content:SizedBox(width:320,child:ListView(shrinkWrap:true,children:[
    for(final l in widget.layers)CheckboxListTile(title:Text(l['name'].toString()),value:selected.contains(l['id'].toString()),onChanged:(v)=>update((){final id=l['id'].toString();if(v==true&&selected.length<8)selected.add(id);else if(v==false&&selected.length>1)selected.remove(id);}))
   ])),actions:[TextButton(onPressed:()=>Navigator.pop(d,false),child:const Text('Hủy')),FilledButton(onPressed:()=>Navigator.pop(d,true),child:const Text('Áp dụng'))])));
  if(ok==true&&mounted){setState(()=>_visible=selected);_schedule();}
 }
 Future<void> _pick(dynamic point)async{
  final map=_map,lease=_current;if(map==null||lease==null||_running||_error!=null)return;
  final epoch=_epoch;
  try{
   // Query per layer to retain canonical layer UUID even when clipped in MVT.
   for(final l in _visible){
    final prefix='${lease.source}-$l';
    final hits=await map.queryRenderedFeatures(point,['$prefix-fill','$prefix-line','$prefix-point'],null);
    if(!mounted||epoch!=_epoch)return;
    if(hits.isEmpty)continue;
    final id=((hits.first as Map)['properties'] as Map?)?['stable_id']?.toString();
    if(id==null)continue;
    final data=jsonDecode(await widget.project.featureDetail(layerId:l,featureId:id,expected:widget.revision));
    if(!mounted||epoch!=_epoch)return;
    await showDialog<void>(context:context,builder:(d)=>AlertDialog(title:const Text('Đối tượng gốc'),content:SingleChildScrollView(child:SelectableText(const JsonEncoder.withIndent('  ').convert(data))),actions:[TextButton(onPressed:()=>Navigator.pop(d),child:const Text('Đóng'))]));
    return;
   }
  }catch(e){if(mounted)setState(()=>_error=e.toString());}
 }
 @override Widget build(BuildContext context)=>PopScope(canPop:!_running,child:Scaffold(
  appBar:AppBar(title:const Text('Unified MVT · xem bản đồ'),actions:[
   IconButton(tooltip:'Lớp',onPressed:_running||_fatal?null:()=>unawaited(_selectLayers()),icon:const Icon(Icons.layers)),
   IconButton(tooltip:'Tải lại',onPressed:_running||_fatal?null:(){_lastKey=null;_schedule();},icon:const Icon(Icons.refresh))
  ]),
  body:Stack(children:[
   Positioned.fill(child:MapLibreMap(styleString:_background,initialCameraPosition:const CameraPosition(target:LatLng(11,106.7),zoom:8),trackCameraPosition:true,onMapCreated:(m)=>_map=m,onStyleLoadedCallback:(){_ready=true;_schedule();},onCameraIdle:_schedule,onMapClick:(p,c)=>unawaited(_pick(p)))),
   Positioned(left:8,right:8,bottom:8,child:Card(child:Padding(padding:const EdgeInsets.all(8),child:Column(mainAxisSize:MainAxisSize.min,crossAxisAlignment:CrossAxisAlignment.start,children:[
    Text('Revision ${widget.revision} · ${_visible.length} lớp · ${_info??"Chờ tile"}'),
    if(_running)const LinearProgressIndicator(),
    if(_error!=null)Text(_error!),if(_fatal)const Text('Đóng bản đồ và mở lại để phục hồi nguồn tile.'),
    const Text('MVT chỉ xem; chỉnh sửa dùng Unified Map. Archive tạm, không tải mạng.')
   ]))))
  ])));
}
