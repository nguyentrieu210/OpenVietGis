import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'package:material_ui/material_ui.dart';
import 'package:path_provider/path_provider.dart';
import 'package:share_plus/share_plus.dart';
import 'workspaces.dart';
import 'src/rust/api/simple.dart';

class ProcessingPage extends StatefulWidget{
 final WorkspaceController controller;
 const ProcessingPage({super.key,required this.controller});
 @override State<ProcessingPage> createState()=>_ProcessingPageState();
}
class _ProcessingPageState extends State<ProcessingPage>{
 final _input=TextEditingController(),_overlay=TextEditingController(),_params=TextEditingController(text:'{}');
 String _algorithm='clip';
 NativeProcessingJob? _job;
 bool _busy=false,_cancel=false;
 int _progress=0;
 String? _result,_error;
 Map<String,dynamic>? _summary;
 @override void dispose(){_cancel=true;final j=_job;if(j!=null){unawaited(j.close());}_input.dispose();_overlay.dispose();_params.dispose();super.dispose();}
 List<String> _ids(String s)=>s.split(RegExp(r'[\s,;]+')).where((v)=>v.isNotEmpty).toList();
 Future<void> _run()async{
  if(_busy)return;final scope=widget.controller.selected;
  setState((){_busy=true;_cancel=false;_error=null;_result=null;_summary=null;_progress=0;});
  NativeProcessingJob? job;
  try{
    job=await widget.controller.local.startLocalProcessing(algorithm:_algorithm,inputIds:_ids(_input.text),overlayIds:_ids(_overlay.text),parametersJson:_params.text);
    if(!mounted||_cancel){await job.close();return;}_job=job;
    while(!await job.isFinished()){
      if(_cancel){await job.cancel();}
      final progress=await job.progress();if(!mounted||scope!=widget.controller.selected){await job.close();return;}
      setState(()=>_progress=progress);
      await Future<void>.delayed(const Duration(milliseconds:100));
    }
    if(_cancel){throw StateError('CANCELLED');}
    final result=await job.takeProcessingJson();final summary=jsonDecode(result) as Map<String,dynamic>;
    if(mounted&&scope==widget.controller.selected)setState((){_result=result;_summary=summary;_progress=100;});
  }catch(_){if(mounted)setState(()=>_error=_cancel?'Cancelled — no output published':'Processing failed: check input IDs, parameters, CRS and topology');}
  finally{if(job!=null){try{await job.close();}catch(_){}}_job=null;if(mounted)setState(()=>_busy=false);}
 }
 Future<void> _publish()async{
  final result=_result;if(result==null||_busy||_summary?['epsg']!=4326)return;
  setState(()=>_busy=true);
  try{await widget.controller.publishResult(result);if(mounted)Navigator.pop(context);}
  catch(_){if(mounted)setState(()=>_error='Publication failed or outcome unknown; check workspace list before retry.');}
  finally{if(mounted)setState(()=>_busy=false);}
 }
 Future<void> _export()async{
  final result=_result;if(result==null||_busy)return;
  final confirm=await showDialog<bool>(context:context,builder:(d)=>AlertDialog(title:const Text('Export processing result'),content:const Text('Share CRS-tagged WKB JSON with attributes and lineage. This is not GeoJSON or a GeoPackage.'),actions:[TextButton(onPressed:()=>Navigator.pop(d,false),child:const Text('Cancel')),TextButton(onPressed:()=>Navigator.pop(d,true),child:const Text('Share'))]));
  if(confirm!=true||!mounted)return;
  try{final dir=await(await getTemporaryDirectory()).createTemp('ovg-result-');final f=await File(dir.path+'/result.ovg.json').writeAsString(result,flush:true);await Share.shareXFiles([XFile(f.path,mimeType:'application/json')]);}
  catch(_){if(mounted)setState(()=>_error='Export failed');}
 }
 @override Widget build(BuildContext context){
  final vi=widget.controller.vi;final r=_summary;
  return Scaffold(appBar:AppBar(title:Text(vi?'Xử lý không gian':'Spatial processing')),body:ListView(padding:const EdgeInsets.all(16),children:[
   Text(vi?'Chế độ local · tối đa 128 đối tượng mỗi tập. Buffer dùng Web Mercator, không phải khoảng cách trắc địa.':'Local mode · max 128 features per set. Buffer uses Web Mercator, not geodesic distance.'),
   DropdownButton<String>(value:_algorithm,isExpanded:true,items:[for(final a in ['buffer','clip','dissolve','difference','intersection','union','reproject','spatial_join'])DropdownMenuItem(value:a,child:Text(a))],onChanged:_busy?null:(a){if(a!=null)setState((){_algorithm=a;_params.text=a=='buffer'?'{"distance_meters":10}':a=='reproject'?'{"target_srid":3857}':'{}';});}),
   TextField(controller:_input,enabled:!_busy,maxLines:3,decoration:const InputDecoration(labelText:'Input feature UUIDs')),
   TextField(controller:_overlay,enabled:!_busy,maxLines:3,decoration:const InputDecoration(labelText:'Overlay feature UUIDs')),
   TextField(controller:_params,enabled:!_busy,maxLines:3,decoration:const InputDecoration(labelText:'Parameters JSON')),
   FilledButton(onPressed:_busy?null:_run,child:Text(vi?'Chạy local':'Run local')),
   if(_busy)...[LinearProgressIndicator(value:_progress/100),TextButton(onPressed:(){_cancel=true;final j=_job;if(j!=null){unawaited(j.cancel());}},child:const Text('Cancel'))],
   if(_error!=null)Text(_error!),
   if(r!=null)...[
     Text('EPSG: '+r['epsg'].toString()+' · '+(r['rows'] as List).length.toString()+' features'),
     Text('Dropped empty: '+r['dropped_empty'].toString()),
     FilledButton(onPressed:_busy||r['epsg']!=4326?null:_publish,child:Text(vi?'Lưu workspace mới':'Publish new workspace')),
     TextButton(onPressed:_busy?null:_export,child:const Text('Export WKB JSON')),
   ],
   const Text('Server processing: unavailable until connected mobile authentication is qualified.'),
  ]));
 }
}
