import 'dart:async';
import 'dart:convert';
import 'dart:typed_data';
import 'package:material_ui/material_ui.dart';
import 'package:maplibre_gl/maplibre_gl.dart';
import 'src/rust/api/simple.dart';

const _empty=<String,dynamic>{'type':'FeatureCollection','features':[]};
const _style=<String,Object>{
 'version':8,'sources':{'unified':{'type':'geojson','data':_empty},'draft':{'type':'geojson','data':_empty}},
 'layers':[
 {'id':'background','type':'background','paint':{'background-color':'#e9efed'}},
 {'id':'u-fill','type':'fill','source':'unified','filter':['==',['geometry-type'],'Polygon'],'paint':{'fill-color':'#278876','fill-opacity':0.4}},
 {'id':'u-line','type':'line','source':'unified','filter':['in',['geometry-type'],['literal',['Polygon','LineString']]],'paint':{'line-color':'#00695c','line-width':2}},
 {'id':'u-point','type':'circle','source':'unified','filter':['==',['geometry-type'],'Point'],'paint':{'circle-color':'#00695c','circle-radius':6}},
 {'id':'d-line','type':'line','source':'draft','filter':['==',['geometry-type'],'LineString'],'paint':{'line-color':'#e65100','line-width':3}},
 {'id':'d-point','type':'circle','source':'draft','filter':['==',['geometry-type'],'Point'],'paint':{'circle-color':'#e65100','circle-radius':5}}
 ]};

class UnifiedMap extends StatefulWidget{
 final NativeUnifiedProject project;
 final String layer;
 final String revision;
 final String Function() uuid;
 const UnifiedMap({super.key,required this.project,required this.layer,required this.revision,required this.uuid});
 @override State<UnifiedMap> createState()=>_UnifiedMapState();
}
class _UnifiedMapState extends State<UnifiedMap> with WidgetsBindingObserver{
 MapLibreMapController? _map;
 late String _revision;
 bool _loaded=false,_busy=false,_running=false,_pending=false,_foreground=true,_truncated=false;
 int _epoch=0;
 Timer? _timer;
 String? _error,_kind,_selected,_featureRevision,_operation,_featureId;
 Map<String,dynamic> _attributes={};
 final _vertices=<LatLng>[];
 @override void initState(){super.initState();_revision=widget.revision;WidgetsBinding.instance.addObserver(this);}
 @override void dispose(){_epoch++;_timer?.cancel();WidgetsBinding.instance.removeObserver(this);super.dispose();}
 @override void didChangeAppLifecycleState(AppLifecycleState state){_foreground=state==AppLifecycleState.resumed;_epoch++;_timer?.cancel();if(_foreground)_schedule();}
 void _schedule(){
  _epoch++;_timer?.cancel();if(!_loaded||!_foreground)return;
  _timer=Timer(const Duration(milliseconds:120),(){_pending=true;unawaited(_drain());});
 }
 Future<void> _drain()async{
  if(_running)return;_running=true;
  try{
   while(mounted&&_pending&&_loaded&&_foreground){
    _pending=false;final epoch=_epoch;final map=_map;if(map==null||_busy)continue;
    try{
     final b=await map.getVisibleRegion();
     if(b.southwest.longitude>b.northeast.longitude){
      await map.setGeoJsonSource('unified',_empty);
      if(mounted&&epoch==_epoch)setState(()=>_truncated=true);continue;
     }
     final raw=await widget.project.viewport(layerId:widget.layer,west:b.southwest.longitude.clamp(-180.0,180.0).toDouble(),south:b.southwest.latitude.clamp(-90.0,90.0).toDouble(),east:b.northeast.longitude.clamp(-180.0,180.0).toDouble(),north:b.northeast.latitude.clamp(-90.0,90.0).toDouble(),expected:_revision);
     if(!mounted||epoch!=_epoch)continue;
     final v=jsonDecode(raw) as Map<String,dynamic>;
     await map.setGeoJsonSource('unified',v['data'] as Map<String,dynamic>);
     if(mounted&&epoch==_epoch)setState((){_truncated=v['truncated']==true;_error=null;});
    }catch(e){if(mounted&&epoch==_epoch)setState(()=>_error=e.toString());}
   }
  }finally{_running=false;}
 }
 Future<void> _run(Future<void> Function() f)async{
  if(_busy||!mounted)return;
  _epoch++;setState((){_busy=true;_error=null;});
  try{await f();}
  catch(e){if(mounted)setState(()=>_error=e.toString());}
  finally{if(mounted){setState(()=>_busy=false);_schedule();}}
 }
 Future<void> _refreshRevision()async{
  final c=jsonDecode(await widget.project.catalog()) as Map<String,dynamic>;
  if(!mounted)return;
  setState((){_revision=c['revision'].toString();_selected=null;_featureRevision=null;});
 }
 Future<void> _preview()async{
  final map=_map;if(map==null||!_loaded)return;
  final p=_vertices.map((v)=>[v.longitude,v.latitude]).toList();
  await map.setGeoJsonSource('draft',{'type':'FeatureCollection','features':[
    for(final xy in p){'type':'Feature','properties':{},'geometry':{'type':'Point','coordinates':xy}},
    if(p.length>=2){'type':'Feature','properties':{},'geometry':{'type':'LineString','coordinates':_kind=='polygon'&&p.length>=3?[...p,p.first]:p}}
  ]});
 }
 Future<void> _tap(dynamic point,LatLng coordinate)async{
  if(_busy||!_loaded)return;
  if(_kind!=null){
   if(_vertices.length>=4095)return;
   setState((){if(_kind=='point')_vertices.clear();_vertices.add(coordinate);});
   await _preview();return;
  }
  final map=_map;if(map==null)return;
  final epoch=_epoch;
  try{
   final hits=await map.queryRenderedFeatures(point,['u-fill','u-line','u-point'],null);
   if(!mounted||epoch!=_epoch)return;
   final id=hits.isEmpty?null:(hits.first as Map)['id']?.toString();
   if(id==null){setState((){_selected=null;_featureRevision=null;});return;}
   final d=jsonDecode(await widget.project.featureDetail(layerId:widget.layer,featureId:id,expected:_revision)) as Map<String,dynamic>;
   if(!mounted||epoch!=_epoch)return;
   setState((){_selected=id;_featureRevision=d['revision'].toString();_attributes=Map<String,dynamic>.from(d['attributes'] as Map);});
  }catch(e){if(mounted&&epoch==_epoch)setState(()=>_error=e.toString());}
 }
 void _start(String kind,{bool replace=false}){
  setState((){_kind=kind;_vertices.clear();_operation=widget.uuid();_featureId=replace?_selected:widget.uuid();if(!replace){_featureRevision='0';_attributes={};_selected=null;}});
 }
 Future<void> _commit()async{
  final kind=_kind;if(kind==null||_featureId==null||_featureRevision==null||_operation==null)return;
  await _run(()async{
   final r=jsonDecode(await widget.project.commitGeometry(layerId:widget.layer,featureId:_featureId!,operationId:_operation!,expected:_revision,featureRevision:_featureRevision!,kind:kind,xy:Float64List.fromList([for(final p in _vertices)...[p.longitude,p.latitude]]),attributesJson:jsonEncode(_attributes))) as Map;
   if(!mounted)return;
   setState((){_revision=r['revision'].toString();_kind=null;_vertices.clear();_selected=null;_featureRevision=null;_operation=null;_featureId=null;});
   await _preview();
  });
 }
 Future<void> _editAttrs()async{
  final id=_selected,fr=_featureRevision;if(id==null||fr==null||_busy)return;
  final c=TextEditingController(text:const JsonEncoder.withIndent('  ').convert(_attributes));
  final text=await showDialog<String>(context:context,builder:(d)=>AlertDialog(title:const Text('Thuộc tính JSON'),content:TextField(controller:c,maxLines:8),actions:[TextButton(onPressed:()=>Navigator.pop(d),child:const Text('Hủy')),FilledButton(onPressed:()=>Navigator.pop(d,c.text),child:const Text('Lưu'))]));
  c.dispose();if(text==null||!mounted)return;
  await _run(()async{if(jsonDecode(text) is! Map)throw StateError('JSON_OBJECT_REQUIRED');final r=await widget.project.editAttributes(layerId:widget.layer,featureId:id,operationId:widget.uuid(),expected:_revision,featureRevision:fr,attributesJson:text);if(mounted)setState((){_revision=r;_selected=null;_featureRevision=null;});});
 }
 Future<void> _delete()async{
  final id=_selected,fr=_featureRevision;if(id==null||fr==null)return;
  final ok=await showDialog<bool>(context:context,builder:(d)=>AlertDialog(title:const Text('Xóa đối tượng?'),content:Text(id),actions:[TextButton(onPressed:()=>Navigator.pop(d,false),child:const Text('Hủy')),FilledButton(onPressed:()=>Navigator.pop(d,true),child:const Text('Xóa'))]));
  if(ok!=true||!mounted)return;
  await _run(()async{final r=jsonDecode(await widget.project.deleteFeature(layerId:widget.layer,featureId:id,operationId:widget.uuid(),expected:_revision,featureRevision:fr)) as Map;if(mounted)setState((){_revision=r['revision'].toString();_selected=null;_featureRevision=null;});});
 }
 Future<void> _compensate()async{
  await _run(()async{final r=jsonDecode(await widget.project.compensate(operationId:widget.uuid(),expected:_revision)) as Map;if(mounted)setState((){_revision=r['revision'].toString();_selected=null;_featureRevision=null;});});
 }
 @override Widget build(BuildContext context)=>Scaffold(
  appBar:AppBar(title:const Text('Unified Map'),actions:[IconButton(tooltip:'Đọc lại revision',onPressed:_busy||_kind!=null?null:()=>unawaited(_run(_refreshRevision)),icon:const Icon(Icons.refresh))]),
  body:Stack(children:[
   Positioned.fill(child:MapLibreMap(styleString:jsonEncode(_style),initialCameraPosition:const CameraPosition(target:LatLng(11,106.7),zoom:8),onMapCreated:(m)=>_map=m,onStyleLoadedCallback:(){_loaded=true;_schedule();},onCameraIdle:_schedule,trackCameraPosition:true,onMapClick:(p,c)=>unawaited(_tap(p,c)))),
   Positioned(top:8,right:8,child:Card(child:Column(mainAxisSize:MainAxisSize.min,children:[
    if(_kind==null)...[
     for(final k in ['point','line','polygon'])IconButton(tooltip:'Vẽ $k',icon:Icon(k=='point'?Icons.place:k=='line'?Icons.timeline:Icons.crop_square),onPressed:_busy?null:()=>_start(k)),
     IconButton(tooltip:'Bù trừ giao dịch cuối',icon:const Icon(Icons.undo),onPressed:_busy?null:()=>unawaited(_compensate())),
    ]else...[
     IconButton(tooltip:'Bỏ đỉnh',icon:const Icon(Icons.undo),onPressed:_busy||_vertices.isEmpty?null:(){setState(()=>_vertices.removeLast());unawaited(_preview());}),
     IconButton(tooltip:'Lưu',icon:const Icon(Icons.check),onPressed:_busy||_vertices.isEmpty?null:()=>unawaited(_commit())),
     IconButton(tooltip:'Hủy',icon:const Icon(Icons.close),onPressed:_busy?null:(){setState((){_kind=null;_vertices.clear();_selected=null;_featureRevision=null;});unawaited(_preview());}),
    ]
   ]))),
   Positioned(left:8,right:68,bottom:8,child:Card(child:Padding(padding:const EdgeInsets.all(8),child:Column(mainAxisSize:MainAxisSize.min,crossAxisAlignment:CrossAxisAlignment.start,children:[
    Text('Revision $_revision · ${_kind??"Bản đồ"}'),
    if(_busy)const LinearProgressIndicator(),
    if(_error!=null)Text(_error!),
    if(_truncated)const Text('Giới hạn 512 đối tượng / 1 MiB. Phóng to để xem thêm.'),
    if(_selected!=null&&_kind==null)...[
     Text('ID: $_selected'),
     Wrap(children:[TextButton(onPressed:_busy?null:()=>unawaited(_editAttrs()),child:const Text('Thuộc tính')),TextButton(onPressed:_busy?null:()=>unawaited(_delete()),child:const Text('Xóa')),PopupMenuButton<String>(tooltip:'Vẽ lại hình học',onSelected:(k)=>_start(k,replace:true),itemBuilder:(_)=>[for(final k in ['point','line','polygon'])PopupMenuItem(value:k,child:Text('Vẽ lại $k'))])])
    ]
   ]))))
  ])
 );
}
