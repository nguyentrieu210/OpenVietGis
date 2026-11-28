import 'dart:async';
import 'dart:convert';
import 'package:material_ui/material_ui.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:maplibre_gl/maplibre_gl.dart';
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
        Positioned.fill(child:MapLibreMap(
          initialCameraPosition:const CameraPosition(target:LatLng(10.8,106.7),zoom:4),
          styleString:jsonEncode(offlineStyle),
        )),
        SafeArea(child:Align(alignment:Alignment.topLeft,child:Padding(padding:const EdgeInsets.all(12),child:Card(child:Padding(
          padding:const EdgeInsets.all(12), child:Column(mainAxisSize:MainAxisSize.min,crossAxisAlignment:CrossAxisAlignment.start,children:[
            Text(tr(vi,'Chế độ độc lập','Standalone'),style:Theme.of(context).textTheme.titleSmall),
            Text(c.selected==null?tr(vi,'Chưa mở kho local','No local workspace open'):'Local • ${c.selected!.substring(0,8)}'),
            if(c.busy)const LinearProgressIndicator(),
            if(c.error!=null) Text(tr(vi,'Không mở được kho local','Local storage unavailable')),
          ]),
        ))))),
        Align(alignment:Alignment.bottomCenter,child:SafeArea(child:Padding(padding:const EdgeInsets.all(12),child:Card(child:Padding(padding:const EdgeInsets.all(12),child:Text(
          tr(vi,'Bản đồ offline trống • Chưa nạp nguồn dữ liệu','Empty offline map • No data source loaded'),
        )))))),
      ]),
      floatingActionButton:FloatingActionButton.extended(
        onPressed:()=>showModalBottomSheet<void>(context:context,builder:(sheet)=>SafeArea(child:Column(mainAxisSize:MainAxisSize.min,children:[
          ListTile(leading:const Icon(Icons.folder),title:Text(tr(vi,'Không gian làm việc local','Local workspaces')),onTap:(){Navigator.pop(sheet);context.push('/workspaces');}),
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
