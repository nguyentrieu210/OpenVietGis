import 'dart:async';
import 'dart:convert';
import 'package:material_ui/material_ui.dart';
import 'package:maplibre_gl/maplibre_gl.dart';
import 'src/rust/api/simple.dart';
import 'local_tile_server.dart';
import 'geometry_draft.dart';
import 'unified_map.dart';

class UnifiedOnDemandMap extends StatefulWidget {
 final NativeUnifiedProject project;
 final String revision;
 final List<String> layers;
 final List<Map<String,dynamic>> catalogLayers;
 final String Function() uuid;
 const UnifiedOnDemandMap({super.key,required this.project,required this.revision,required this.layers,required this.catalogLayers,required this.uuid});
 @override State<UnifiedOnDemandMap> createState()=>_UnifiedOnDemandMapState();
}
class _UnifiedOnDemandMapState extends State<UnifiedOnDemandMap> with WidgetsBindingObserver {
 LocalTileServer? _server;
 MapLibreMapController? _map;
 String? _style,_error;
 bool _busy=true,_foreground=true,_closing=false,_canPop=false;
 int _epoch=0;
 late String _revision;
 CameraPosition _camera=const CameraPosition(target:LatLng(11,106.7),zoom:8);
 static const _colors=['#00695c','#315b9b','#9b4c80','#927017','#4b768e','#99533a','#6555a5','#627b36'];
 @override void initState(){super.initState();_revision=widget.revision;WidgetsBinding.instance.addObserver(this);unawaited(_start());}
 Future<void> _start()async{
  final epoch=++_epoch;
  try{
   final revision=_revision;
   final s=await LocalTileServer.start(widget.layers.toSet(),(l,z,x,y)=>widget.project.vectorTile(layerId:l,z:z,x:x,y:y,expected:revision));
   if(!mounted||epoch!=_epoch||!_foreground){await s.close();return;}
   final sources=<String,dynamic>{};
   final layers=<Map<String,dynamic>>[{'id':'background','type':'background','paint':{'background-color':'#e9efed'}}];
   for(var i=0;i<widget.layers.length;i++){
    final id=widget.layers[i],key='s$i',color=_colors[i];
    sources[key]={'type':'vector','tiles':[s.template(id)],'minzoom':0,'maxzoom':22};
    layers.addAll([
     {'id':'$key-fill','type':'fill','source':key,'source-layer':'local','filter':['==',['geometry-type'],'Polygon'],'paint':{'fill-color':color,'fill-opacity':0.4}},
     {'id':'$key-line','type':'line','source':key,'source-layer':'local','filter':['in',['geometry-type'],['literal',['Polygon','LineString']]],'paint':{'line-color':color,'line-width':2}},
     {'id':'$key-point','type':'circle','source':key,'source-layer':'local','filter':['==',['geometry-type'],'Point'],'paint':{'circle-color':color,'circle-radius':5}}
    ]);
   }
   _server=s;
   setState(()=>_style=jsonEncode({'version':8,'sources':sources,'layers':layers}));
  }catch(_){if(mounted&&epoch==_epoch)setState(()=>_error='Không khởi tạo được tile bridge. Sử dụng chế độ MBTiles.');}
  finally{if(mounted&&epoch==_epoch)setState(()=>_busy=false);}
 }
 Future<void> _stop()async{
  ++_epoch;final s=_server;_server=null;_map=null;
  if(mounted)setState((){_style=null;_busy=true;});
  await s?.close();
  if(mounted)setState(()=>_busy=false);
 }
 @override void didChangeAppLifecycleState(AppLifecycleState state){
  _foreground=state==AppLifecycleState.resumed;
  if(!_foreground&&!_closing){_closing=true;unawaited(_stop().whenComplete((){if(mounted)setState(()=>_closing=false);}));}
 }
 @override void dispose(){WidgetsBinding.instance.removeObserver(this);++_epoch;unawaited(_server?.close()??Future<void>.value());super.dispose();}
 Future<void> _pick(dynamic point)async{
  final map=_map,epoch=_epoch;if(map==null||_busy)return;
  try{
   for(var i=0;i<widget.layers.length;i++){
    final hits=await map.queryRenderedFeatures(point,['s$i-fill','s$i-line','s$i-point'],null);
    if(!mounted||epoch!=_epoch)return;
    if(hits.isEmpty)continue;
    final id=((hits.first as Map)['properties'] as Map?)?['stable_id']?.toString();
    if(id==null)continue;
    final detail=jsonDecode(await widget.project.featureDetail(layerId:widget.layers[i],featureId:id,expected:_revision));
    if(!mounted||epoch!=_epoch)return;
    final action=await showDialog<String>(context:context,builder:(d)=>AlertDialog(title:const Text('Đối tượng gốc'),content:SingleChildScrollView(child:SelectableText(const JsonEncoder.withIndent('  ').convert(detail))),actions:[TextButton(onPressed:()=>Navigator.pop(d),child:const Text('Đóng')),TextButton(onPressed:()=>Navigator.pop(d,'attrs'),child:const Text('Thuộc tính')),TextButton(onPressed:()=>Navigator.pop(d,'vertices'),child:const Text('Chỉnh đỉnh'))]));
    if(!mounted||epoch!=_epoch||action==null)return;
    await _edit(widget.layers[i],id,Map<String,dynamic>.from(detail as Map),action);
    return;
   }
  }catch(_){if(mounted)setState(()=>_error='Không đọc được đối tượng ở revision hiện tại.');}
 }
 Future<void> _edit(String layer,String id,Map<String,dynamic> detail,String action)async{
  if(_busy||_closing||!_foreground)return;
  _camera=_map?.cameraPosition??_camera;
  _closing=true;
  try{
   await _stop();
   if(!mounted||!_foreground)return;
   if(action=='vertices'){
    await Navigator.of(context).push(MaterialPageRoute<void>(builder:(_)=>UnifiedMap(project:widget.project,layer:layer,revision:_revision,layers:widget.catalogLayers,uuid:widget.uuid,initialFeature:id,initialCamera:_camera)));
   }else{
    final ctrl=TextEditingController(text:const JsonEncoder.withIndent('  ').convert(detail['attributes']));
    try{
     final ok=await showDialog<bool>(context:context,builder:(d)=>AlertDialog(title:const Text('Sửa thuộc tính gốc'),content:TextField(controller:ctrl,maxLines:8),actions:[TextButton(onPressed:()=>Navigator.pop(d,false),child:const Text('Hủy')),FilledButton(onPressed:()=>Navigator.pop(d,true),child:const Text('Lưu'))]));
     if(ok==true&&mounted&&_foreground){
      final attrs=jsonDecode(ctrl.text);if(attrs is! Map)throw FormatException('Thuộc tính phải là JSON object');
      GeometryDraft.parse(Map<String,dynamic>.from(detail['geometry'] as Map));
      await widget.project.editAttributes(layerId:layer,featureId:id,operationId:widget.uuid(),expected:_revision,featureRevision:detail['revision'].toString(),attributesJson:jsonEncode(attrs));
     }
    }finally{ctrl.dispose();}
   }
  }catch(e){if(mounted)setState(()=>_error=e.toString());}
  finally{
   _closing=false;
   if(mounted){
    try{final c=jsonDecode(await widget.project.catalog()) as Map;_revision=c['revision'].toString();}
    catch(e){if(mounted)setState(()=>_error=e.toString());}
    if(mounted&&_foreground){setState(()=>_busy=true);await _start();}
   }
  }
 }
 Future<void> _leave()async{
  if(_closing)return;_closing=true;await _stop();if(mounted){setState(()=>_canPop=true);WidgetsBinding.instance.addPostFrameCallback((_){if(mounted)Navigator.of(context).pop();});}
 }
 @override Widget build(BuildContext context)=>PopScope(canPop:_canPop,onPopInvokedWithResult:(didPop,result){if(!didPop)unawaited(_leave());},child:Scaffold(
 appBar:AppBar(leading:IconButton(icon:const Icon(Icons.arrow_back),onPressed:_closing||_busy?null:()=>unawaited(_leave())),title:const Text('MVT theo yêu cầu')),
 body:Stack(children:[
  if(_style!=null)Positioned.fill(child:MapLibreMap(key:ValueKey(_epoch),styleString:_style!,initialCameraPosition:_camera,trackCameraPosition:true,onMapCreated:(m)=>_map=m,onMapClick:(p,c)=>unawaited(_pick(p)))),
  if(_style==null)Center(child:_busy?const CircularProgressIndicator():FilledButton(onPressed:!_foreground||_closing?null:(){setState((){_busy=true;_error=null;});unawaited(_start());},child:const Text('Mở lại tile bridge'))),
  Positioned(left:8,right:8,bottom:8,child:Card(child:Padding(padding:const EdgeInsets.all(8),child:Text(_error??'Revision $_revision · ${widget.layers.length} lớp. Tile qua loopback giới hạn. Chọn đối tượng để chỉnh sửa dữ liệu gốc. Đóng màn hình để ngắt. Không dùng cho dữ liệu managed.'))))
 ])));
}
