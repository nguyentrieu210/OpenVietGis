import 'dart:async';
import 'dart:convert';
import 'dart:typed_data';
import 'dart:math' as math;
import 'package:material_ui/material_ui.dart';
import 'package:maplibre_gl/maplibre_gl.dart';
import 'src/rust/api/simple.dart';

const _empty=<String,dynamic>{'type':'FeatureCollection','features':[]};
const _style=<String,Object>{
 'version':8,'sources':{'unified':{'type':'geojson','data':_empty},'draft':{'type':'geojson','data':_empty}},
 'layers':[
 {'id':'background','type':'background','paint':{'background-color':'#e9efed'}},
 {'id':'u-fill','type':'fill','source':'unified','filter':['==',['geometry-type'],'Polygon'],'paint':{'fill-color':['get','display_color'],'fill-opacity':0.4}},
 {'id':'u-line','type':'line','source':'unified','filter':['in',['geometry-type'],['literal',['Polygon','LineString']]],'paint':{'line-color':['get','display_color'],'line-width':2}},
 {'id':'u-point','type':'circle','source':'unified','filter':['==',['geometry-type'],'Point'],'paint':{'circle-color':['get','display_color'],'circle-radius':6}},
 {'id':'d-line','type':'line','source':'draft','filter':['==',['geometry-type'],'LineString'],'paint':{'line-color':'#e65100','line-width':3}},
 {'id':'d-point','type':'circle','source':'draft','filter':['==',['geometry-type'],'Point'],'paint':{'circle-color':'#e65100','circle-radius':5}}
 ]};

class UnifiedMap extends StatefulWidget{
 final NativeUnifiedProject project;
 final String layer;
 final String revision;
 final List<Map<String,dynamic>> layers;
 final String Function() uuid;
 const UnifiedMap({super.key,required this.project,required this.layer,required this.revision,required this.layers,required this.uuid});
 @override State<UnifiedMap> createState()=>_UnifiedMapState();
}
class _UnifiedMapState extends State<UnifiedMap> with WidgetsBindingObserver{
 MapLibreMapController? _map;
 late String _revision,_active;
 late Set<String> _visible;
 bool _snap=true,_vertexMode=false,_insert=false;
 int _vertex=0;
 Map<String,dynamic>? _geometry;
 static const _colors=['#00695c','#315b9b','#9b4c80','#927017','#4b768e','#99533a','#6555a5','#627b36'];
 bool _loaded=false,_busy=false,_running=false,_pending=false,_foreground=true,_truncated=false;
 int _epoch=0;
 Timer? _timer;
 String? _error,_kind,_selected,_featureRevision,_operation,_featureId;
 Map<String,dynamic> _attributes={};
 final _vertices=<LatLng>[];
 @override void initState(){super.initState();_revision=widget.revision;_active=widget.layer;_visible={widget.layer};WidgetsBinding.instance.addObserver(this);}
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
     final layers=[_active,..._visible.where((l)=>l!=_active)];
     final features=<Map<String,dynamic>>[];var truncated=false,bytes=0;
     final quota=1024~/layers.length;
     for(var i=0;i<layers.length;i++){
      final l=layers[i];
      final raw=await widget.project.viewport(layerId:l,west:b.southwest.longitude.clamp(-180.0,180.0).toDouble(),south:b.southwest.latitude.clamp(-90.0,90.0).toDouble(),east:b.northeast.longitude.clamp(-180.0,180.0).toDouble(),north:b.northeast.latitude.clamp(-90.0,90.0).toDouble(),expected:_revision);
      if(!mounted||epoch!=_epoch)break;
      final v=jsonDecode(raw) as Map<String,dynamic>;
      final fs=(v['data'] as Map)['features'] as List;
      truncated=truncated||v['truncated']==true||fs.length>quota;
      for(final rawFeature in fs.take(quota)){
       final f=Map<String,dynamic>.from(rawFeature as Map);
       final id=f['id'].toString();
       f['id']='$l:$id';
       // Only display metadata crosses the map bridge; attributes loaded on pick.
       f['properties']={'layer':l,'feature_id':id,'display_color':_colors[i%_colors.length]};
       final n=utf8.encode(jsonEncode(f)).length+1;
       if(bytes+n>2*1024*1024-1024){truncated=true;break;}
       bytes+=n;features.add(f);
      }
     }
     if(!mounted||epoch!=_epoch)continue;
     await map.setGeoJsonSource('unified',{'type':'FeatureCollection','features':features});
     if(mounted&&epoch==_epoch)setState(()=>_truncated=truncated);
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
  try{await map.setGeoJsonSource('draft',{'type':'FeatureCollection','features':[
    for(final xy in p){'type':'Feature','properties':{},'geometry':{'type':'Point','coordinates':xy}},
    if(p.length>=2){'type':'Feature','properties':{},'geometry':{'type':'LineString','coordinates':_kind=='polygon'&&p.length>=3?[...p,p.first]:p}}
  ]});}catch(e){if(mounted)setState(()=>_error=e.toString());}
 }
 Future<void> _tap(dynamic point,LatLng coordinate)async{
  if(_busy||!_loaded)return;
  if(_kind!=null){
   if(_vertices.length>=4095&&(!_vertexMode||_insert))return;
   await _run(()async{
    var p=coordinate;
    if(_snap){
     final zoom=_map?.cameraPosition?.zoom??18;
     final metres=(8*156543.03392804097*math.cos(p.latitude*math.pi/180)/math.pow(2,zoom)).clamp(0.1,100.0).toDouble();
     final hit=jsonDecode(await widget.project.snapUnified(layers:[_active,..._visible.where((l)=>l!=_active)],x:p.longitude,y:p.latitude,toleranceM:metres,expected:_revision,excludeLayer:_featureRevision!='0'?_active:null,excludeFeature:_featureRevision!='0'?_featureId:null));
     if(hit is Map)p=LatLng((hit['y'] as num).toDouble(),(hit['x'] as num).toDouble());
    }
    if(!mounted)return;
    setState((){
     if(_vertexMode){if(_insert){_vertices.insert(_vertex+1,p);_vertex++;_insert=false;}else{_vertices[_vertex]=p;}}
     else{if(_kind=='point')_vertices.clear();_vertices.add(p);}
    });await _preview();
   });return;
  }
  final map=_map;if(map==null)return;
  final epoch=++_epoch;
  try{
   final hits=await map.queryRenderedFeatures(point,['u-fill','u-line','u-point'],null);
   if(!mounted||epoch!=_epoch)return;
   final props=hits.isEmpty?null:(hits.first as Map)['properties'] as Map?;
   final id=props?['feature_id']?.toString(),layer=props?['layer']?.toString();
   if(id==null||layer==null||!_visible.contains(layer)){setState((){_selected=null;_featureRevision=null;});return;}
   final d=jsonDecode(await widget.project.featureDetail(layerId:layer,featureId:id,expected:_revision)) as Map<String,dynamic>;
   if(!mounted||epoch!=_epoch)return;
   setState((){_active=layer;_geometry=Map<String,dynamic>.from(d['geometry'] as Map);_selected=id;_featureRevision=d['revision'].toString();_attributes=Map<String,dynamic>.from(d['attributes'] as Map);});
  }catch(e){if(mounted&&epoch==_epoch)setState(()=>_error=e.toString());}
 }
 void _start(String kind,{bool replace=false}){
  setState((){_vertexMode=false;_insert=false;_kind=kind;_vertices.clear();_operation=widget.uuid();_featureId=replace?_selected:widget.uuid();if(!replace){_featureRevision='0';_attributes={};_selected=null;}});
 }
 Future<void> _chooseLayers()async{
  final selected=Set<String>.from(_visible);var active=_active;
  final ok=await showDialog<bool>(context:context,builder:(d)=>StatefulBuilder(builder:(d,update)=>AlertDialog(title:const Text('Lớp hiển thị / lớp chỉnh sửa'),content:SizedBox(width:320,child:ListView(shrinkWrap:true,children:[
   for(final l in widget.layers)CheckboxListTile(title:Text(l['name'].toString()),value:selected.contains(l['id'].toString()),onChanged:(v)=>update((){final id=l['id'].toString();if(v==true&&selected.length<8){selected.add(id);}else if(v==false&&id!=active){selected.remove(id);}})),
   const Text('Lớp chỉnh sửa'),
   DropdownButton<String>(isExpanded:true,value:active,items:[for(final l in widget.layers.where((l)=>selected.contains(l['id'].toString())))DropdownMenuItem(value:l['id'].toString(),child:Text(l['name'].toString()))],onChanged:(v)=>update((){if(v!=null)active=v;}))
  ])),actions:[TextButton(onPressed:()=>Navigator.pop(d,false),child:const Text('Hủy')),FilledButton(onPressed:()=>Navigator.pop(d,true),child:const Text('Áp dụng'))])));
  if(ok!=true||!mounted)return;
  setState((){_active=active;_visible=selected;_selected=null;_featureRevision=null;_geometry=null;});_schedule();
 }
 Future<void> _editVertices()async{
  if(_geometry==null||_selected==null||_featureRevision==null)return;
  final g=_geometry!;final type=g['type'];final raw=g['coordinates'];
  List<dynamic> coords;
  String kind;
  if(type=='Point'){kind='point';coords=[raw];}
  else if(type=='LineString'){kind='line';coords=raw as List;}
  else if(type=='Polygon'&&raw is List&&raw.length==1){kind='polygon';final ring=raw.first as List;coords=ring.take(ring.length-1).toList();}
  else{setState(()=>_error='Chỉ hỗ trợ chỉnh đỉnh Point/LineString/Polygon một vòng; không làm mất lỗ hoặc multipart.');return;}
  if(coords.isEmpty||coords.length>4095)return;
  final points=coords.map((v)=>LatLng((v[1] as num).toDouble(),(v[0] as num).toDouble())).toList();
  _start(kind,replace:true);setState((){_vertexMode=true;_vertex=0;_vertices.addAll(points);});await _preview();
 }
 Future<void> _commit()async{
  final kind=_kind;if(kind==null||_featureId==null||_featureRevision==null||_operation==null)return;
  await _run(()async{
   final r=jsonDecode(await widget.project.commitGeometry(layerId:_active,featureId:_featureId!,operationId:_operation!,expected:_revision,featureRevision:_featureRevision!,kind:kind,xy:Float64List.fromList([for(final p in _vertices)...[p.longitude,p.latitude]]),attributesJson:jsonEncode(_attributes))) as Map;
   if(!mounted)return;
   setState((){_revision=r['revision'].toString();_kind=null;_vertexMode=false;_insert=false;_vertices.clear();_selected=null;_featureRevision=null;_operation=null;_featureId=null;});
   await _preview();
  });
 }
 Future<void> _editAttrs()async{
  final id=_selected,fr=_featureRevision;if(id==null||fr==null||_busy)return;
  final c=TextEditingController(text:const JsonEncoder.withIndent('  ').convert(_attributes));
  final text=await showDialog<String>(context:context,builder:(d)=>AlertDialog(title:const Text('Thuộc tính JSON'),content:TextField(controller:c,maxLines:8),actions:[TextButton(onPressed:()=>Navigator.pop(d),child:const Text('Hủy')),FilledButton(onPressed:()=>Navigator.pop(d,c.text),child:const Text('Lưu'))]));
  c.dispose();if(text==null||!mounted)return;
  await _run(()async{if(jsonDecode(text) is! Map)throw StateError('JSON_OBJECT_REQUIRED');final r=await widget.project.editAttributes(layerId:_active,featureId:id,operationId:widget.uuid(),expected:_revision,featureRevision:fr,attributesJson:text);if(mounted)setState((){_revision=r;_selected=null;_featureRevision=null;});});
 }
 Future<void> _delete()async{
  final id=_selected,fr=_featureRevision;if(id==null||fr==null)return;
  final ok=await showDialog<bool>(context:context,builder:(d)=>AlertDialog(title:const Text('Xóa đối tượng?'),content:Text(id),actions:[TextButton(onPressed:()=>Navigator.pop(d,false),child:const Text('Hủy')),FilledButton(onPressed:()=>Navigator.pop(d,true),child:const Text('Xóa'))]));
  if(ok!=true||!mounted)return;
  await _run(()async{final r=jsonDecode(await widget.project.deleteFeature(layerId:_active,featureId:id,operationId:widget.uuid(),expected:_revision,featureRevision:fr)) as Map;if(mounted)setState((){_revision=r['revision'].toString();_selected=null;_featureRevision=null;});});
 }
 Future<void> _compensate()async{
  await _run(()async{final r=jsonDecode(await widget.project.compensate(operationId:widget.uuid(),expected:_revision)) as Map;if(mounted)setState((){_revision=r['revision'].toString();_selected=null;_featureRevision=null;});});
 }
 @override Widget build(BuildContext context)=>PopScope(canPop:!_busy&&_kind==null,child:Scaffold(
  appBar:AppBar(title:const Text('Unified Map'),actions:[IconButton(tooltip:'Đọc lại revision',onPressed:_busy||_kind!=null?null:()=>unawaited(_run(_refreshRevision)),icon:const Icon(Icons.refresh))]),
  body:Stack(children:[
   Positioned.fill(child:MapLibreMap(styleString:jsonEncode(_style),initialCameraPosition:const CameraPosition(target:LatLng(11,106.7),zoom:8),onMapCreated:(m)=>_map=m,onStyleLoadedCallback:(){_loaded=true;_schedule();},onCameraIdle:_schedule,trackCameraPosition:true,onMapClick:(p,c)=>unawaited(_tap(p,c)))),
   Positioned(top:8,right:8,child:Card(child:Column(mainAxisSize:MainAxisSize.min,children:[
    if(_kind==null)...[
     IconButton(tooltip:'Quản lý lớp',icon:const Icon(Icons.layers),onPressed:_busy?null:()=>unawaited(_chooseLayers())),
     for(final k in ['point','line','polygon'])IconButton(tooltip:'Vẽ $k',icon:Icon(k=='point'?Icons.place:k=='line'?Icons.timeline:Icons.crop_square),onPressed:_busy?null:()=>_start(k)),
     IconButton(tooltip:'Bù trừ giao dịch cuối',icon:const Icon(Icons.undo),onPressed:_busy?null:()=>unawaited(_compensate())),
    ]else...[
     IconButton(tooltip:'Snapping',icon:Icon(_snap?Icons.my_location:Icons.location_searching),onPressed:_busy?null:()=>setState(()=>_snap=!_snap)),
     if(_vertexMode&&_kind!='point')IconButton(tooltip:'Chèn sau đỉnh chọn',icon:const Icon(Icons.add_location_alt),onPressed:_busy||_vertices.length>=4095?null:()=>setState(()=>_insert=!_insert)),
     IconButton(tooltip:'Bỏ đỉnh',icon:const Icon(Icons.undo),onPressed:_busy||_vertices.isEmpty?null:(){setState((){if(_vertexMode){if(_vertices.length<=(_kind=='polygon'?3:_kind=='line'?2:1))return;_vertices.removeAt(_vertex);_vertex=math.min(_vertex,_vertices.length-1);_insert=false;}else{_vertices.removeLast();}});unawaited(_preview());}),
     IconButton(tooltip:'Lưu',icon:const Icon(Icons.check),onPressed:_busy||_vertices.isEmpty?null:()=>unawaited(_commit())),
     IconButton(tooltip:'Hủy',icon:const Icon(Icons.close),onPressed:_busy?null:(){setState((){_kind=null;_vertexMode=false;_insert=false;_vertices.clear();_selected=null;_featureRevision=null;});unawaited(_preview());}),
    ]
   ]))),
   Positioned(left:8,right:68,bottom:8,child:Card(child:Padding(padding:const EdgeInsets.all(8),child:Column(mainAxisSize:MainAxisSize.min,crossAxisAlignment:CrossAxisAlignment.start,children:[
    Text('Revision $_revision · ${_kind??"Bản đồ"}'),
    Text('Lớp: '+widget.layers.firstWhere((l)=>l['id']==_active,orElse:()=>{'name':_active})['name'].toString()+' · '+_visible.length.toString()+' hiển thị'),
    if(_vertexMode&&_vertices.isNotEmpty)Row(children:[IconButton(onPressed:_busy||_vertex==0?null:()=>setState(()=>_vertex--),icon:const Icon(Icons.chevron_left)),Expanded(child:Text('Đỉnh ${_vertex+1}/${_vertices.length} · ${_insert?"chèn":"chạm bản đồ để di chuyển"}')),IconButton(onPressed:_busy||_vertex==_vertices.length-1?null:()=>setState(()=>_vertex++),icon:const Icon(Icons.chevron_right))]),
    if(_busy)const LinearProgressIndicator(),
    if(_error!=null)Text(_error!),
    if(_truncated)const Text('Giới hạn khung nhìn / 2 MiB tổng. Phóng to để xem thêm.'),
    if(_selected!=null&&_kind==null)...[
     Text('ID: $_selected'),
     Wrap(children:[TextButton(onPressed:_busy?null:()=>unawaited(_editVertices()),child:const Text('Chỉnh đỉnh')),TextButton(onPressed:_busy?null:()=>unawaited(_editAttrs()),child:const Text('Thuộc tính')),TextButton(onPressed:_busy?null:()=>unawaited(_delete()),child:const Text('Xóa')),PopupMenuButton<String>(tooltip:'Vẽ lại hình học',enabled:!_busy,onSelected:(k)=>_start(k,replace:true),itemBuilder:(_)=>[for(final k in ['point','line','polygon'])PopupMenuItem(value:k,child:Text('Vẽ lại $k'))])])
    ]
   ]))))
  ])
 ));
}
