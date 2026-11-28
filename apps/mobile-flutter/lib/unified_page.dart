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
import 'unified_mvt_map.dart';
import 'unified_ondemand_map.dart';
import 'unified_survey.dart';

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
  String? _root,_exchange,_legacyRoot,_selected,_layer,_revision;
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
    _legacyRoot='${root.path}${Platform.pathSeparator}local-workspaces';
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
  Future<void> _migrateLegacy()async{
    final ids=await listLocalWorkspaces(rootPath:_legacyRoot!);
    if(!mounted)return;
    final id=await showDialog<String>(context:context,builder:(d)=>AlertDialog(
      title:const Text('Sao chép dự án GeoPackage'),
      content:SizedBox(width:320,child:ListView(shrinkWrap:true,children:[
        const Text('Giữ nguyên file nguồn, vector, revision và GPS Survey. Media mã hóa vẫn ở kho cũ; lịch sử cũ chỉ lưu trữ, không phát lại Undo.'),
        for(final id in ids)ListTile(title:Text(id),onTap:()=>Navigator.pop(d,id))
      ])),actions:[TextButton(onPressed:()=>Navigator.pop(d),child:const Text('Hủy'))]));
    if(id==null||!mounted)return;
    final legacy=await openLocalWorkspace(rootPath:_legacyRoot!,workspaceId:id);
    String project;
    try{project=await legacy.migrateUnifiedCopy(unifiedRoot:_root!,layerId:id,name:'Imported $id');}
    finally{await legacy.close();}
    final projects=await listUnifiedProjects(rootPath:_root!);
    if(!mounted)return;setState(()=>_ids=projects);await _open(project);
    if(mounted)setState(()=>_message='Đã sao chép vector và GPS Survey; media mã hóa vẫn ở nguồn.');
  }
  Future<void> _captureSurvey()async{
    final p=_project,l=_layer,r=_revision;if(p==null||l==null||r==null)return;
    await Navigator.of(context).push(MaterialPageRoute<void>(builder:(_)=>UnifiedSurvey(project:p,layer:l,revision:r,uuid:_uuid)));
    if(mounted)await _refresh();
  }
  Future<void> _mediaInventory()async{
    final p=_project,l=_layer,r=_revision;if(p==null||l==null||r==null)return;
    await Navigator.of(context).push(MaterialPageRoute<void>(builder:(_)=>_MediaInventoryPage(project:p,layer:l,revision:r,legacyRoot:_legacyRoot!)));
    if(mounted)await _refresh();
  }
  Future<void> _survey()async{
    final p=_project,l=_layer,r=_revision;if(p==null||l==null||r==null)return;
    final tracks=<Map<String,dynamic>>[];var after='';
    for(var page=0;page<10;page++){
      final list=(jsonDecode(await p.surveyTracks(layerId:l,after:after,expected:r)) as List).cast<Map<String,dynamic>>();
      tracks.addAll(list);if(list.length<100)break;after=list.last['id'].toString();
    }
    if(!mounted)return;
    final track=await showDialog<String>(context:context,builder:(d)=>AlertDialog(title:const Text('GPS Survey đã lưu (tối đa 1.000 track)'),content:SizedBox(width:320,child:ListView(shrinkWrap:true,children:[
      if(tracks.isEmpty)const Text('Không có track trong lớp này.'),
      for(final t in tracks)ListTile(title:Text(t['name'].toString()),subtitle:Text('${t["state"]} · ${t["segment"]} đoạn'),onTap:()=>Navigator.pop(d,t['id'].toString()))
    ])),actions:[TextButton(onPressed:()=>Navigator.pop(d),child:const Text('Đóng'))]));
    if(track==null||!mounted)return;
    await Navigator.of(context).push(MaterialPageRoute<void>(builder:(_)=>_SurveyReadPage(project:p,layer:l,track:track,revision:r)));
  }
  Future<void> _openOnDemandMvt()async{
    final p=_project,l=_layer,r=_revision;if(p==null||l==null||r==null)return;
    final selected=<String>{l};
    final ok=await showDialog<bool>(context:context,builder:(d)=>StatefulBuilder(builder:(d,update)=>AlertDialog(
      title:const Text('MVT theo yêu cầu · tối đa 8 lớp'),
      content:SizedBox(width:320,child:ListView(shrinkWrap:true,children:[
        const Text('Bản đồ đọc tile qua bridge loopback riêng của phiên. Không dùng cho dữ liệu managed.'),
        for(final item in _layers)CheckboxListTile(title:Text(item['name'].toString()),value:selected.contains(item['id'].toString()),onChanged:(v)=>update((){final id=item['id'].toString();if(v==true&&selected.length<8)selected.add(id);else if(v==false&&selected.length>1)selected.remove(id);}))
      ])),actions:[TextButton(onPressed:()=>Navigator.pop(d,false),child:const Text('Hủy')),FilledButton(onPressed:()=>Navigator.pop(d,true),child:const Text('Mở'))])));
    if(ok!=true||!mounted)return;
    await Navigator.of(context).push(MaterialPageRoute<void>(builder:(_)=>UnifiedOnDemandMap(project:p,revision:r,layers:selected.toList())));
    if(mounted)await _refresh();
  }
  Future<void> _openMvt()async{
    final p=_project,l=_layer,r=_revision;if(p==null||l==null||r==null)return;
    await Navigator.of(context).push(MaterialPageRoute<void>(builder:(_)=>UnifiedMvtMap(project:p,revision:r,layers:List.unmodifiable(_layers),initialLayer:l)));
    if(mounted)await _refresh();
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
          OutlinedButton(onPressed:ready?()=>unawaited(_run(_migrateLegacy)):null,child:const Text('Sao chép GeoPackage / Survey')),
        ]),
        for(final id in _ids)ListTile(title:Text(id),leading:Icon(id==_selected?Icons.check_circle:Icons.folder),enabled:ready,onTap:()=>unawaited(_run(()=>_open(id)))),
        if(_project!=null)...[
          Text('Revision ${_revision??"?"}'),
          FilledButton.icon(onPressed:edit?()=>unawaited(_run(_openMap)):null,icon:const Icon(Icons.map),label:const Text('Mở bản đồ Unified')),
          FilledButton.icon(onPressed:edit?()=>unawaited(_run(_openMvt)):null,icon:const Icon(Icons.layers),label:const Text('Bản đồ MVT thử nghiệm')),
          FilledButton.icon(onPressed:edit?()=>unawaited(_run(_openOnDemandMvt)):null,icon:const Icon(Icons.map_outlined),label:const Text('MVT theo yêu cầu · thử nghiệm')),
          OutlinedButton(onPressed:edit?()=>unawaited(_run(_survey)):null,child:const Text('GPS Survey đã lưu')),
          OutlinedButton(onPressed:edit?()=>unawaited(_run(_mediaInventory)):null,child:const Text('Media mã hóa · danh mục')),
          FilledButton.icon(onPressed:edit?()=>unawaited(_run(_captureSurvey)):null,icon:const Icon(Icons.gps_fixed),label:const Text('Khảo sát GPS Unified')),
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

class _SurveyReadPage extends StatefulWidget{
 final NativeUnifiedProject project;final String layer,track,revision;
 const _SurveyReadPage({required this.project,required this.layer,required this.track,required this.revision});
 @override State<_SurveyReadPage> createState()=>_SurveyReadPageState();
}
class _SurveyReadPageState extends State<_SurveyReadPage>{
 List<Map<String,dynamic>> _rows=[];String? _next,_error;bool _busy=false;
 @override void initState(){super.initState();unawaited(_load('0'));}
 Future<void> _load(String after)async{
  if(_busy)return;setState(()=>_busy=true);
  try{
   final v=jsonDecode(await widget.project.surveyFixes(layerId:widget.layer,trackId:widget.track,after:after,expected:widget.revision)) as Map<String,dynamic>;
   if(mounted)setState((){_rows=(v['rows'] as List).cast<Map<String,dynamic>>();_next=v['next']?.toString();_error=null;});
  }catch(e){if(mounted)setState(()=>_error=e.toString());}finally{if(mounted)setState(()=>_busy=false);}
 }
 @override Widget build(BuildContext context)=>PopScope(canPop:!_busy,child:Scaffold(appBar:AppBar(title:const Text('GPS Fix · dữ liệu gốc')),body:ListView(padding:const EdgeInsets.all(12),children:[
  if(_busy)const LinearProgressIndicator(),if(_error!=null)Text(_error!),
  for(final row in _rows)ListTile(title:Text('Fix ${row["seq"]} · Đoạn ${row["segment"]}'),subtitle:Text(jsonEncode(row['fix']))),
  if(_next!=null)OutlinedButton(onPressed:_busy?null:()=>unawaited(_load(_next!)),child:const Text('Trang tiếp'))
 ])));
}

class _MediaInventoryPage extends StatefulWidget{
 final NativeUnifiedProject project;final String layer,revision,legacyRoot;
 const _MediaInventoryPage({required this.project,required this.layer,required this.revision,required this.legacyRoot});
 @override State<_MediaInventoryPage> createState()=>_MediaInventoryPageState();
}
class _MediaInventoryPageState extends State<_MediaInventoryPage>{
 List<Map<String,dynamic>> _rows=[];String? _next,_error,_message;bool _busy=false,_provider=false,_key=false;
 late String _revision;
 @override void initState(){super.initState();_revision=widget.revision;unawaited(_run(()=>_load('')));}
 Future<void> _run(Future<void> Function() f)async{
  if(_busy)return;setState(()=>_busy=true);
  try{await f();if(mounted)setState(()=>_error=null);}
  catch(e){if(mounted)setState(()=>_error=e.toString());}finally{if(mounted)setState(()=>_busy=false);}
 }
 Future<void> _load(String after)async{
  final v=jsonDecode(await widget.project.mediaInventory(layerId:widget.layer,after:after,expected:_revision)) as Map<String,dynamic>;
  final status=jsonDecode(await widget.project.mediaKeyStatus(layerId:widget.layer,expected:_revision)) as Map;
  final rows=(v['items'] as List).cast<Map<String,dynamic>>();
  if(mounted)setState((){_rows=rows;_next=rows.length==100?rows.last['id'].toString():null;_provider=status['provider_available']==true;_key=status['key_available']==true;});
 }
 Future<void> _import()async{
  final pick=await FilePicker.platform.pickFiles(type:FileType.custom,allowedExtensions:['json'],withData:false);
  if(pick==null||!mounted)return;
  final path=pick.files.single.path;if(path==null)throw StateError('INVALID_MANIFEST');
  final f=File(path);final data=<int>[];
  await for(final b in f.openRead()){if(data.length+b.length>65536)throw StateError('RESOURCE_LIMIT');data.addAll(b);}
  final raw=utf8.decode(data);final v=jsonDecode(raw) as Map;
  final items=v['items'];if(v['version']!=1||items is! List||items.isEmpty||items.length>16)throw StateError('INVALID_MANIFEST');
  if(!mounted)return;
  final ok=await showDialog<bool>(context:context,builder:(d)=>AlertDialog(title:const Text('Sao chép media mã hóa?'),content:Text('Nguồn: ${v["source_scope"]}\n${items.length} liên kết. Chỉ thực hiện khi khóa nguồn đã được cấp trên thiết bị; không thay đổi file nguồn. Kiểm tra danh sách đối tượng trước khi xác nhận.'),actions:[TextButton(onPressed:()=>Navigator.pop(d,false),child:const Text('Hủy')),FilledButton(onPressed:()=>Navigator.pop(d,true),child:const Text('Nhập'))]));
  if(ok!=true||!mounted)return;
  final r=jsonDecode(await widget.project.migrateMediaManifest(legacyRoot:widget.legacyRoot,manifestJson:raw,expected:_revision)) as Map;
  _revision=r['revision'].toString();await _load('');
  if(mounted)setState(()=>_message='Đã nhập ${r["count"]} media vào revision $_revision.');
 }
 Future<void> _verify(String id)async{
  final ok=await widget.project.verifyMediaUnlock(attachmentId:id,expected:_revision);
  if(mounted)setState(()=>_message=ok?'Xác thực AEAD thành công; nội dung không xuất qua Flutter.':'Không xác thực được.');
 }
 @override Widget build(BuildContext context)=>PopScope(canPop:!_busy,child:Scaffold(appBar:AppBar(title:const Text('Media mã hóa')),body:ListView(padding:const EdgeInsets.all(16),children:[
  Text(_provider?'Bộ cấp khóa native có mặt':'Bộ cấp khóa native chưa sẵn sàng'),
  Text(_key?'Có khóa miền đích trên thiết bị':'Miền đích chưa có khóa khả dụng'),
  const Text('Nhập qua manifest liên kết đã xác thực nguồn. Không gửi khóa hoặc ảnh giải mã qua Flutter. Backup không mang theo khóa; không tự tạo khóa thay thế khi khóa cũ mất.'),
  OutlinedButton(onPressed:_busy||!_provider?null:()=>unawaited(_run(_import)),child:const Text('Nhập manifest media (tối đa 16)')),
  if(_busy)const LinearProgressIndicator(),if(_error!=null)Text(_error!),if(_message!=null)Text(_message!),
  if(!_busy&&_rows.isEmpty&&_error==null)const Text('Không có media mã hóa trong trang này.'),
  for(final row in _rows)ListTile(leading:Icon(_key?Icons.verified_user:Icons.lock_outline),title:Text(row['id'].toString()),subtitle:Text('Đối tượng: ${row["feature"]}\n${row["cipher_bytes"]} byte ciphertext'),trailing:TextButton(onPressed:_busy||!_key?null:()=>unawaited(_run(()=>_verify(row['id'].toString()))),child:const Text('Xác thực'))),
  if(_next!=null)OutlinedButton(onPressed:_busy?null:()=>unawaited(_run(()=>_load(_next!))),child:const Text('Trang tiếp'))
 ])));
}
