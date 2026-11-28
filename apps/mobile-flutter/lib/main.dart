import 'dart:async';
import 'dart:io';
import 'dart:typed_data';
import 'package:file_picker/file_picker.dart';
import 'package:share_plus/share_plus.dart';
import 'package:path_provider/path_provider.dart';
import 'native_map.dart';
import 'package:material_ui/material_ui.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'workspaces.dart';

const offlineStyle = <String,Object>{
  'version':8, 'sources': <String,Object>{},
  'layers':[
    {'id':'background','type':'background','paint':{'background-color':'#e9efed'}}
  ]
};
String tr(bool vi,String vn,String en)=>vi?vn:en;
final controllerProvider=ChangeNotifierProvider<WorkspaceController>((ref){
  final c=WorkspaceController();
  unawaited(c.boot());
  return c;
});
final router=GoRouter(routes:[
  GoRoute(path:'/',builder:(context,state)=>const MapPage()),
  GoRoute(path:'/workspaces',builder:(context,state)=>const WorkspacePage())
]);
void main(){
  WidgetsFlutterBinding.ensureInitialized();
  runApp(const ProviderScope(child:OpenVietGis()));
}
class OpenVietGis extends StatelessWidget {
  const OpenVietGis({super.key});
  @override
  Widget build(BuildContext context)=>MaterialApp.router(
    title:'OpenVietGIS', theme:ThemeData(colorScheme:ColorScheme.fromSeed(seedColor:const Color(0xff00695c)),useMaterial3:true),
    routerConfig:router,
  );
}
class MapPage extends ConsumerWidget{
  const MapPage({super.key});
  @override
  Widget build(BuildContext context,WidgetRef ref){
    final c=ref.watch(controllerProvider); final vi=c.vi;
    return Scaffold(
      appBar:AppBar(title:const Text('OpenVietGIS'),actions:[
        IconButton(tooltip:tr(vi,'Không gian làm việc','Workspaces'),onPressed:()=>context.push('/workspaces'),icon:const Icon(Icons.folder_open)),
        IconButton(tooltip:tr(vi,'Đổi ngôn ngữ','Change language'),onPressed:c.toggleLanguage,icon:const Icon(Icons.language)),
      ]),
      body:Stack(children:[
        Positioned.fill(child:NativeLocalMap(key:ValueKey(c.selected),controller:c)),
        SafeArea(child:Align(alignment:Alignment.topLeft,child:Padding(padding:const EdgeInsets.all(12),child:Card(child:Padding(
          padding:const EdgeInsets.all(12), child:Column(mainAxisSize:MainAxisSize.min,crossAxisAlignment:CrossAxisAlignment.start,children:[
            Text(tr(vi,'Chế độ độc lập','Standalone'),style:Theme.of(context).textTheme.titleSmall),
            Text(c.selected==null?tr(vi,'Chưa mở kho local','No local workspace open'):'Local • ${c.selected!.substring(0,8)}'),
            if(c.busy)const LinearProgressIndicator(),
            if(c.error!=null) Text(tr(vi,'Không mở được kho local','Local storage unavailable')),
          ]),
        ))))),
      ]),
      floatingActionButton:FloatingActionButton.extended(
        onPressed:()=>showModalBottomSheet<void>(context:context,builder:(sheet)=>SafeArea(child:Column(mainAxisSize:MainAxisSize.min,children:[
          ListTile(leading:const Icon(Icons.folder),title:Text(tr(vi,'Không gian làm việc local','Local workspaces')),onTap:(){Navigator.pop(sheet);context.push('/workspaces');}),
          ListTile(leading:const Icon(Icons.file_open),title:Text(tr(vi,'Nhập GeoJSON','Import GeoJSON')),subtitle:Text(tr(vi,'WGS84 XY · tối đa 16 MiB / 4096 đối tượng','WGS84 XY · max 16 MiB / 4096 features')),enabled:c.ready&&!c.busy,onTap:(){Navigator.pop(sheet);unawaited(importFile(context,c));}),
          ListTile(leading:const Icon(Icons.share),title:Text(tr(vi,'Xuất GeoJSON','Export GeoJSON')),enabled:c.ready&&!c.busy,onTap:(){Navigator.pop(sheet);unawaited(exportFile(context,c));}),
          ListTile(leading:const Icon(Icons.cloud_off),title:Text(tr(vi,'Kết nối máy chủ','Connect server')),subtitle:Text(tr(vi,'Chưa đủ điều kiện xác thực Mobile','Mobile authentication not qualified')),enabled:false),
          ListTile(leading:const Icon(Icons.extension),title:Text(tr(vi,'Plugin','Plugins')),subtitle:Text(tr(vi,'Chưa có runtime đã nghiệm thu','Runtime not qualified')),enabled:false),
        ]))),
        icon:const Icon(Icons.menu),label:Text(tr(vi,'Công cụ','Tools')),
      ),
    );
  }
}
class WorkspacePage extends ConsumerWidget{
  const WorkspacePage({super.key});
  @override
  Widget build(BuildContext context,WidgetRef ref){
    final c=ref.watch(controllerProvider);final vi=c.vi;
    return Scaffold(
      appBar:AppBar(title:Text(tr(vi,'Không gian local','Local workspaces'))),
      body:Column(children:[
        if(c.busy)const LinearProgressIndicator(),
        if(c.error!=null) Padding(padding:const EdgeInsets.all(16),child:Text(tr(vi,'Kho local không sẵn sàng. Dữ liệu hiện hữu không bị xóa.','Local storage unavailable. Existing data is not deleted.'))),
        Expanded(child:ListView.builder(itemCount:c.ids.length,itemBuilder:(context,index){
          final id=c.ids[index];
          return ListTile(leading:Icon(c.selected==id?Icons.check_circle:Icons.folder_open),title:Text(id),subtitle:const Text('GeoPackage • Local'),enabled:!c.busy,onTap:()=>c.select(id));
        })),
      ]),
      floatingActionButton:FloatingActionButton.extended(onPressed:c.busy||!c.ready?null:c.create,icon:const Icon(Icons.add),label:Text(tr(vi,'Tạo kho local','Create workspace'))),
    );
  }
}

Future<void> importFile(BuildContext context,WorkspaceController c)async{
  final vi=c.vi;final workspace=c.selected;
  try{
    final selected=await FilePicker.platform.pickFiles(type:FileType.custom,allowedExtensions:['geojson','json'],withData:false);
    if(selected==null)return;
    final item=selected.files.single;
    if(item.path==null||item.size>16*1024*1024)throw StateError('INPUT_LIMIT');
    final f=File(item.path!);
    if(await f.length()>16*1024*1024)throw StateError('INPUT_LIMIT');
    if(!context.mounted)return;
    final confirm=await showDialog<bool>(context:context,builder:(d)=>AlertDialog(
      title:Text(tr(vi,'Nhập bản sao vào kho local','Import a local copy')),
      content:Text(tr(vi,'Chỉ hỗ trợ GeoJSON WGS84 XY, thuộc tính JSON và ID UUID (hoặc không ID). CRS khác, Z/M hoặc ID khác sẽ bị từ chối.','Only WGS84 XY, JSON attributes and UUID or absent IDs. Other CRS, Z/M or incompatible IDs are rejected.')),
      actions:[TextButton(onPressed:()=>Navigator.pop(d,false),child:Text(tr(vi,'Hủy','Cancel'))),TextButton(onPressed:()=>Navigator.pop(d,true),child:Text(tr(vi,'Nhập','Import')))]));
    if(confirm!=true)return;
    final bytes=await f.openRead().fold<List<int>>([], (all,chunk){
      if(all.length+chunk.length>16*1024*1024)throw StateError('INPUT_LIMIT');
      return all..addAll(chunk);
    });
    if(c.selected!=workspace)throw StateError('WORKSPACE_CHANGED');
    final n=await c.importGeojson(Uint8List.fromList(bytes));
    if(context.mounted)ScaffoldMessenger.of(context).showSnackBar(SnackBar(content:Text(tr(vi,'Đã nhập $n đối tượng','Imported $n features'))));
  }catch(_){if(context.mounted)ScaffoldMessenger.of(context).showSnackBar(SnackBar(content:Text(tr(vi,'Không thể nhập. Không ghi một phần dữ liệu.','Import failed. No partial writes.'))));}
}
Future<void> exportFile(BuildContext context,WorkspaceController c)async{
  final vi=c.vi;final workspace=c.selected;
  try{
    final confirm=await showDialog<bool>(context:context,builder:(d)=>AlertDialog(
      title:Text(tr(vi,'Chia sẻ dữ liệu local','Share local data')),
      content:Text(tr(vi,'Xuất bản sao GeoJSON WGS84 XY. Không mang theo lịch sử sửa đổi hoặc quy tắc hệ thống.','Export WGS84 XY GeoJSON copy. Edit history and system rules are not exported.')),
      actions:[TextButton(onPressed:()=>Navigator.pop(d,false),child:Text(tr(vi,'Hủy','Cancel'))),TextButton(onPressed:()=>Navigator.pop(d,true),child:Text(tr(vi,'Xuất','Export')))]));
    if(confirm!=true)return;
    if(c.selected!=workspace)throw StateError('WORKSPACE_CHANGED');
    final json=await c.exportGeojson();
    final dir=await (await getTemporaryDirectory()).createTemp('ovg-export-');
    final f=await File('${dir.path}/workspace.geojson').writeAsString(json,flush:true);
    // Share is explicit; platform-specific recipient copies are outside app ownership.
    await Share.shareXFiles([XFile(f.path,mimeType:'application/geo+json')]);
  }catch(_){if(context.mounted)ScaffoldMessenger.of(context).showSnackBar(SnackBar(content:Text(tr(vi,'Không thể xuất bộ dữ liệu này','Unable to export this dataset'))));}
  // Do not delete while the share target may still be reading; OS cache cleanup.
}
