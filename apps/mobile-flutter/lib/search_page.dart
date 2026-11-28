import 'dart:convert';
import 'package:material_ui/material_ui.dart';
import 'workspaces.dart';
class SearchPage extends StatefulWidget{
 final WorkspaceController controller;
 const SearchPage({super.key,required this.controller});
 @override State<SearchPage> createState()=>_SearchPageState();
}
class _SearchPageState extends State<SearchPage>{
 final _query=TextEditingController();
 List<Map<String,dynamic>> _hits=[];
 bool _busy=false,_end=false;
 String _cursor='0',_term='';
 String? _error;
 @override void dispose(){_query.dispose();super.dispose();}
 Future<void> _search(bool reset)async{
  if(_busy)return;
  final scope=widget.controller.selected;
  setState((){_busy=true;_error=null;if(reset){_cursor='0';_term=_query.text.trim();_hits=[];_end=false;}});
  try{
    final parts=_term.split(',').map((s)=>double.tryParse(s.trim())).toList();
    if(parts.length==2&&parts.every((v)=>v!=null)){
      final x=parts[0]!,y=parts[1]!;
      if(!x.isFinite||!y.isFinite||x.abs()>180||y.abs()>90)throw StateError('INVALID_XY');
      if(mounted&&scope==widget.controller.selected){widget.controller.focus(x,y);Navigator.pop(context);}return;
    }
    final list=jsonDecode(await widget.controller.local.searchFeatures(query:_term,after:_cursor,limit:100)) as List;
    if(!mounted||scope!=widget.controller.selected)return;
    setState((){_hits.addAll(list.cast<Map<String,dynamic>>());if(list.isEmpty){_end=true;}else{_cursor=list.last['fid'].toString();}});
  }catch(_){if(mounted)setState(()=>_error='Search unavailable or invalid coordinates');}
  finally{if(mounted)setState(()=>_busy=false);}
 }
 @override Widget build(BuildContext context){
  final vi=widget.controller.vi;
  return Scaffold(appBar:AppBar(title:Text(vi?'Tìm kiếm local':'Offline search')),body:Column(children:[
    Padding(padding:const EdgeInsets.all(12),child:TextField(controller:_query,maxLength:256,onSubmitted:(_)=>_search(true),decoration:InputDecoration(labelText:vi?'Thuộc tính, UUID hoặc kinh độ, vĩ độ':'Attribute, UUID or longitude, latitude',suffixIcon:IconButton(icon:const Icon(Icons.search),onPressed:_busy?null:()=>_search(true))))),
    if(_busy)const LinearProgressIndicator(),if(_error!=null)Text(_error!),
    Expanded(child:ListView.builder(itemCount:_hits.length,itemBuilder:(context,i){final h=_hits[i];return ListTile(title:Text(h['id'].toString()),subtitle:Text(jsonEncode(h['attributes']),maxLines:2,overflow:TextOverflow.ellipsis),onTap:(){final b=(h['bbox'] as List).cast<num>();widget.controller.focus((b[0]+b[2])/2,(b[1]+b[3])/2);Navigator.pop(context);});})),
    if(!_end&&_hits.isNotEmpty)TextButton(onPressed:_busy?null:()=>_search(false),child:Text(vi?'Tải thêm':'Load more')),
  ]));
 }
}
