import 'dart:async';
import 'dart:convert';
import 'package:material_ui/material_ui.dart';
import 'package:maplibre_gl/maplibre_gl.dart';
import 'src/rust/api/simple.dart';
import 'local_tile_server.dart';

class UnifiedOnDemandMap extends StatefulWidget {
 final NativeUnifiedProject project;
 final String revision;
 final List<String> layers;
 const UnifiedOnDemandMap({super.key,required this.project,required this.revision,required this.layers});
 @override State<UnifiedOnDemandMap> createState()=>_UnifiedOnDemandMapState();
}
class _UnifiedOnDemandMapState extends State<UnifiedOnDemandMap> with WidgetsBindingObserver {
 LocalTileServer? _server;
 MapLibreMapController? _map;
 String? _style,_error;
 bool _busy=true,_foreground=true,_closing=false,_canPop=false;
 int _epoch=0;
 static const _colors=['#00695c','#315b9b','#9b4c80','#927017','#4b768e','#99533a','#6555a5','#627b36'];
 @override void initState(){super.initState();WidgetsBinding.instance.addObserver(this);unawaited(_start());}
 Future<void> _start()async{
  final epoch=++_epoch;
  try{
   final s=await LocalTileServer.start(widget.layers.toSet(),(l,z,x,y)=>widget.project.vectorTile(layerId:l,z:z,x:x,y:y,expected:widget.revision));
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
    final detail=jsonDecode(await widget.project.featureDetail(layerId:widget.layers[i],featureId:id,expected:widget.revision));
    if(!mounted||epoch!=_epoch)return;
    await showDialog<void>(context:context,builder:(d)=>AlertDialog(title:const Text('Đối tượng gốc'),content:SingleChildScrollView(child:SelectableText(const JsonEncoder.withIndent('  ').convert(detail))),actions:[TextButton(onPressed:()=>Navigator.pop(d),child:const Text('Đóng'))]));
    return;
   }
  }catch(_){if(mounted)setState(()=>_error='Không đọc được đối tượng ở revision hiện tại.');}
 }
 Future<void> _leave()async{
  if(_closing)return;_closing=true;await _stop();if(mounted){setState(()=>_canPop=true);WidgetsBinding.instance.addPostFrameCallback((_){if(mounted)Navigator.of(context).pop();});}
 }
 @override Widget build(BuildContext context)=>PopScope(canPop:_canPop,onPopInvokedWithResult:(didPop,result){if(!didPop)unawaited(_leave());},child:Scaffold(
 appBar:AppBar(leading:IconButton(icon:const Icon(Icons.arrow_back),onPressed:_closing||_busy?null:()=>unawaited(_leave())),title:const Text('MVT theo yêu cầu')),
 body:Stack(children:[
  if(_style!=null)Positioned.fill(child:MapLibreMap(key:ValueKey(_epoch),styleString:_style!,initialCameraPosition:const CameraPosition(target:LatLng(11,106.7),zoom:8),onMapCreated:(m)=>_map=m,onMapClick:(p,c)=>unawaited(_pick(p)))),
  if(_style==null)Center(child:_busy?const CircularProgressIndicator():FilledButton(onPressed:!_foreground||_closing?null:(){setState((){_busy=true;_error=null;});unawaited(_start());},child:const Text('Mở lại tile bridge'))),
  Positioned(left:8,right:8,bottom:8,child:Card(child:Padding(padding:const EdgeInsets.all(8),child:Text(_error??'Revision ${widget.revision} · ${widget.layers.length} lớp. Tile qua loopback giới hạn, chỉ xem. Đóng màn hình để ngắt. Không dùng cho dữ liệu managed.'))))
 ])));
}
