import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:math';
import 'package:file_picker/file_picker.dart';
import 'package:material_ui/material_ui.dart';
import 'package:path_provider/path_provider.dart';
import 'package:share_plus/share_plus.dart';
import 'src/rust/api/simple.dart';
import 'unified_map.dart';

String _uuid(){
  final r=Random.secure();
  final b=List<int>.generate(16,(_)=>r.nextInt(256));
  b[6]=(b[6]&15)|64;b[8]=(b[8]&63)|128;
  final s=b.map((n)=>n.toRadixString(16).padLeft(2,'0')).join();
  return '${s.substring(0,8)}-${s.substring(8,12)}-${s.substring(12,16)}-${s.substring(16,20)}-${s.substring(20)}';
}
/// Explicit standalone candidate. No implicit upgrade of existing GeoPackage.
class UnifiedPage extends StatefulWidget{
  const UnifiedPage({super.key});
  @override State<UnifiedPage> createState()=>_UnifiedPageState();
}
class _UnifiedPageState extends State<UnifiedPage>{
  String? _root,_exchange,_selected,_layer,_revision;
  NativeUnifiedProject? _project;
  List<String> _ids=[];
  List<Map<String,dynamic>> _layers=[],_rows=[];
  String? _cursor,_error,_message;
  bool _busy=false;
  final _name=TextEditingController(text:'Layer 2');
  final _lon=TextEditingController(text:'106.7');
  final _lat=TextEditingController(text:'11');
  final _bbox=TextEditingController(text:'105,10,108,12');
  final _attrs=TextEditingController(text:'{}');
  @override void initState(){super.initState();unawaited(_run(()async{
    final root=await getApplicationSupportDirectory();
    _root='${root.path}${Platform.pathSeparator}unified-projects';
    _exchange='${root.path}${Platform.pathSeparator}unified-exchange';
    final ids=await listUnifiedProjects(rootPath:_root!);
    if(mounted)setState(()=>_ids=ids);
  }));}
  @override void dispose(){
    final p=_project;_project=null;if(p!=null)unawaited(p.close().catchError((Object _){}));
    for(final c in [_name,_lon,_lat,_bbox,_attrs]){c.dispose();}super.dispose();
  }
  Future<void> _run(Future<void> Function() fn)async{
    if(_busy||!mounted)return;setState((){_busy=true;_error=null;_message=null;});
    try{await fn();}catch(e){if(mounted)setState(()=>_error=e.toString());}
    finally{if(mounted)setState(()=>_busy=false);}
  }
  Future<void> _refresh()async{
    final p=_project;if(p==null)return;
    final c=jsonDecode(await p.catalog()) as Map<String,dynamic>;
    final layers=(c['layers'] as List).cast<Map<String,dynamic>>();
    if(!mounted||p!=_project)return;
    setState((){
      _revision=c['revision'].toString();_layers=layers;
      if(!layers.any((l)=>l['id']==_layer))_layer=layers.isEmpty?null:layers.first['id'].toString();
      _rows=[];_cursor=null;
    });
  }
  Future<void> _open(String id)async{
    final next=await openUnifiedProject(rootPath:_root!,projectId:id);
    if(!mounted){await next.close();return;}
    final previous=_project;
    setState((){_project=next;_selected=id;_layer=null;_rows=[];_cursor=null;});
    if(previous!=null)await previous.close();
    if(mounted)await _refresh();
  }
  Future<void> _create()async{
    final id=await createUnifiedProject(rootPath:_root!);
    final ids=await listUnifiedProjects(rootPath:_root!);
    if(!mounted)return;setState(()=>_ids=ids);await _open(id);
  }
  Future<void> _add()async{
    final p=_project;if(p==null||_revision==null)return;
    final name=_name.text.trim();if(name.isEmpty)throw StateError('Tên lớp không hợp lệ');
    final id=_uuid();
    await p.addLayer(layerId:id,name:name,expected:_revision!);
    await _refresh();if(mounted)setState(()=>_layer=id);
  }
  Future<void> _openMap()async{
    final p=_project,l=_layer,r=_revision;
    if(p==null||l==null||r==null)return;
    await Navigator.of(context).push(MaterialPageRoute<void>(builder:(_)=>UnifiedMap(project:p,layer:l,revision:r,layers:List.unmodifiable(_layers),uuid:_uuid)));
    if(mounted)await _refresh();
  }
  Future<void> _draw()async{
    final p=_project;if(p==null||_layer==null||_revision==null)return;
    final lon=double.tryParse(_lon.text),lat=double.tryParse(_lat.text);
    if(lon==null||lat==null||!lon.isFinite||!lat.isFinite||lon.abs()>180||lat.abs()>90)throw StateError('WGS84 XY required');
    if(jsonDecode(_attrs.text) is! Map)throw StateError('JSON object required');
    await p.drawPoint(layerId:_layer!,featureId:_uuid(),operationId:_uuid(),expected:_revision!,lon:lon,lat:lat,attributesJson:_attrs.text);
    await _refresh();
  }
  Future<void> _query(bool next)async{
    final p=_project;if(p==null||_layer==null||_revision==null)return;
    final b=_bbox.text.split(',').map((v)=>double.parse(v.trim())).toList();
    if(b.length!=4||b.any((v)=>!v.isFinite)||b[0]>b[2]||b[1]>b[3])throw StateError('BBox west,south,east,north');
    final r=jsonDecode(await p.query(layerId:_layer!,west:b[0],south:b[1],east:b[2],north:b[3],after:next?_cursor??'0':'0',limit:50,expected:_revision!)) as Map<String,dynamic>;
    if(!mounted)return;
    setState((){_rows=(r['rows'] as List).cast<Map<String,dynamic>>();_cursor=r['next']?.toString();});
  }
  Future<void> _backup()async{
    final p=_project;if(p==null||_revision==null)return;
    final confirmed=await showDialog<bool>(context:context,builder:(d)=>AlertDialog(title:const Text("Chia sẻ bản sao dự án?"),content:const Text("Backup chứa toàn bộ dữ liệu độc lập của dự án. Chỉ chia sẻ đến thiết bị hoặc ứng dụng tin cậy."),actions:[TextButton(onPressed:()=>Navigator.pop(d,false),child:const Text("Hủy")),FilledButton(onPressed:()=>Navigator.pop(d,true),child:const Text("Sao lưu"))]));
    if(confirmed!=true||!mounted)return;
    final name=await p.backup(exchangeRoot:_exchange!,expected:_revision!);
    if(!mounted)return;
    final f=File('$_exchange${Platform.pathSeparator}$name');
    setState(()=>_message='Backup: $name');
    await Share.shareXFiles([XFile(f.path)],subject:'OpenVietGIS Unified backup');
  }
  Future<void> _restore()async{
    final pick=await FilePicker.platform.pickFiles(type:FileType.custom,allowedExtensions:['sqlite'],withData:false);
    if(pick==null||!mounted)return;
    final path=pick.files.single.path;if(path==null)throw StateError('Không có bản sao file cục bộ');
    final src=File(path);
    const cap=512*1024*1024;
    final size=await src.length();if(size<=0||size>cap)throw StateError('Mobile import: tối đa 512 MiB');
    final name='${_uuid()}.sqlite';final dir=Directory(_exchange!);await dir.create(recursive:true);
    final target=File('${dir.path}${Platform.pathSeparator}$name');
    var created=false;
    try{
      await target.create(exclusive:true);created=true;
      var copied=0;
      final sink=target.openWrite();
      try{await for(final b in src.openRead()){copied+=b.length;if(copied>cap||copied>size)throw StateError('SOURCE_CHANGED');sink.add(b);await sink.flush();}}
      finally{await sink.close();}
      if(copied!=size)throw StateError('SOURCE_CHANGED');
      final id=await restoreUnifiedProject(rootPath:_root!,exchangeRoot:_exchange!,name:name);
      final ids=await listUnifiedProjects(rootPath:_root!);
      if(!mounted)return;setState(()=>_ids=ids);await _open(id);
    }finally{if(created&&await target.exists())await target.delete();}
  }
  @override Widget build(BuildContext context){
    final ready=_root!=null&&!_busy;
    final edit=ready&&_project!=null&&_layer!=null&&_revision!=null;
    return PopScope(canPop:!_busy,child:Scaffold(appBar:AppBar(title:const Text('Unified Store · thử nghiệm')),
      body:ListView(padding:const EdgeInsets.all(16),children:[
        const Text('Kho độc lập dùng cùng SQLite kernel với Desktop. Không chuyển đổi GeoPackage cũ hoặc dữ liệu máy chủ.'),
        if(_busy)const LinearProgressIndicator(),
        if(_error!=null)Text(_error!),if(_message!=null)Text(_message!),
        Wrap(spacing:8,runSpacing:8,children:[
          FilledButton(onPressed:ready?()=>unawaited(_run(_create)):null,child:const Text('Tạo dự án')),
          OutlinedButton(onPressed:ready?()=>unawaited(_run(_restore)):null,child:const Text('Nhập backup SQLite')),
        ]),
        for(final id in _ids)ListTile(title:Text(id),leading:Icon(id==_selected?Icons.check_circle:Icons.folder),enabled:ready,onTap:()=>unawaited(_run(()=>_open(id)))),
        if(_project!=null)...[
          Text('Revision ${_revision??"?"}'),
          FilledButton.icon(onPressed:edit?()=>unawaited(_run(_openMap)):null,icon:const Icon(Icons.map),label:const Text('Mở bản đồ Unified')),
          for(final l in _layers)ListTile(title:Text(l['name'].toString()),leading:Icon(l['id']==_layer?Icons.check_circle:Icons.layers),enabled:ready,onTap:()=>setState((){_layer=l['id'].toString();_rows=[];_cursor=null;})),
          TextField(controller:_name,enabled:ready,maxLength:256,decoration:const InputDecoration(labelText:'Tên lớp')),
          OutlinedButton(onPressed:ready&&_revision!=null?()=>unawaited(_run(_add)):null,child:const Text('Thêm lớp')),
          TextField(controller:_lon,enabled:edit,decoration:const InputDecoration(labelText:'Kinh độ')),
          TextField(controller:_lat,enabled:edit,decoration:const InputDecoration(labelText:'Vĩ độ')),
          TextField(controller:_attrs,enabled:edit,maxLines:3,decoration:const InputDecoration(labelText:'Thuộc tính JSON')),
          FilledButton(onPressed:edit?()=>unawaited(_run(_draw)):null,child:const Text('Thêm điểm WGS84')),
          TextField(controller:_bbox,enabled:edit,decoration:const InputDecoration(labelText:'BBox west,south,east,north')),
          Wrap(spacing:8,children:[
            OutlinedButton(onPressed:edit?()=>unawaited(_run(()=>_query(false))):null,child:const Text('Truy vấn')),
            OutlinedButton(onPressed:edit&&_cursor!=null?()=>unawaited(_run(()=>_query(true))):null,child:const Text('Trang tiếp')),
            OutlinedButton(onPressed:ready?()=>unawaited(_run(()async{final ok=await _project!.integrity();if(mounted)setState(()=>_message='Integrity: $ok');})):null,child:const Text('Kiểm tra')),
            OutlinedButton(onPressed:ready&&_revision!=null?()=>unawaited(_run(_backup)):null,child:const Text('Sao lưu / chia sẻ')),
          ]),
          for(final row in _rows)ListTile(title:Text(row['id'].toString()),subtitle:Text(jsonEncode((row['image'] as Map)['attributes'])))
        ]
      ])));
  }
}
