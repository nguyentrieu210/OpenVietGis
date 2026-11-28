import 'dart:convert';
import 'package:material_ui/material_ui.dart';
import 'workspaces.dart';
class AttributePage extends StatefulWidget{
 final WorkspaceController controller;
 const AttributePage({super.key,required this.controller});
 @override State<AttributePage> createState()=>_AttributePageState();
}
class _AttributePageState extends State<AttributePage>{
 final _search=TextEditingController();
 final _rows=<Map<String,dynamic>>[];
 String _cursor='0',_query='';
 bool _busy=false,_end=false;
 String? _error;
 @override void initState(){super.initState();_load(true);}
 @override void dispose(){_search.dispose();super.dispose();}
 Future<void> _load(bool reset)async{
   if(_busy)return;
   setState((){_busy=true;_error=null;if(reset){_cursor='0';_rows.clear();_query=_search.text;_end=false;}});
   try{
     final v=jsonDecode(await widget.controller.local.attributePage(after:_cursor,limit:100,contains:_query)) as List;
     if(!mounted)return;
     setState((){_rows.addAll(v.cast<Map<String,dynamic>>());if(v.isNotEmpty)_cursor=v.last['fid'].toString();else _end=true;});
   }catch(e){if(mounted)setState(()=>_error=e.toString());}
   finally{if(mounted)setState(()=>_busy=false);}
 }
 @override Widget build(BuildContext context){
   final vi=widget.controller.vi;
   return Scaffold(appBar:AppBar(title:Text(vi?'Bảng thuộc tính':'Attributes')),body:Column(children:[
     Padding(padding:const EdgeInsets.all(12),child:TextField(controller:_search,maxLength:128,onSubmitted:(_)=>_load(true),decoration:InputDecoration(labelText:vi?'Tìm thuộc tính':'Filter attributes',suffixIcon:IconButton(icon:const Icon(Icons.search),onPressed:_busy?null:()=>_load(true))))),
     if(_error!=null)Text(_error!),if(_busy)const LinearProgressIndicator(),
     Expanded(child:ListView.builder(itemCount:_rows.length,itemBuilder:(context,i){final row=_rows[i];return ListTile(title:Text(row['id'].toString()),subtitle:Text(jsonEncode(row['attributes'])),onTap:()=>showDialog<void>(context:context,builder:(d)=>AlertDialog(title:Text(row['id'].toString()),content:SingleChildScrollView(child:SelectableText(const JsonEncoder.withIndent('  ').convert(row['attributes']))),actions:[TextButton(onPressed:()=>Navigator.pop(d),child:const Text('OK'))])));})),
     if(!_end)TextButton(onPressed:_busy?null:()=>_load(false),child:Text(vi?'Tải thêm':'Load more')),
   ]));
 }
}
