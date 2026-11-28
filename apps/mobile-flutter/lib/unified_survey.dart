import 'dart:async';
import 'dart:convert';
import 'package:material_ui/material_ui.dart';
import 'package:geolocator/geolocator.dart';
import 'src/rust/api/simple.dart';

/// Foreground-only location recording. One write in flight; pause waits for its
/// acknowledgement before changing segment state. No automatic resume.
class UnifiedSurvey extends StatefulWidget{
 final NativeUnifiedProject project;
 final String layer,revision;
 final String Function() uuid;
 const UnifiedSurvey({super.key,required this.project,required this.layer,required this.revision,required this.uuid});
 @override State<UnifiedSurvey> createState()=>_UnifiedSurveyState();
}
class _UnifiedSurveyState extends State<UnifiedSurvey> with WidgetsBindingObserver{
 late String _revision;
 String? _track,_error;
 Position? _last;
 List<Map<String,dynamic>> _tracks=[];
 StreamSubscription<Position>? _stream;
 Future<void>? _write,_stopping;
 bool _recording=false,_busy=true,_foreground=true;
 int _epoch=0,_saved=0,_skipped=0;
 @override void initState(){super.initState();_revision=widget.revision;WidgetsBinding.instance.addObserver(this);unawaited(_recover());}
 @override void dispose(){WidgetsBinding.instance.removeObserver(this);_foreground=false;_epoch++;unawaited(_stream?.cancel()??Future<void>.value());super.dispose();}
 @override void didChangeAppLifecycleState(AppLifecycleState state){
  _foreground=state==AppLifecycleState.resumed;
  if(!_foreground){_epoch++;if(mounted)setState(()=>_last=null);if(_recording)unawaited(_stop(false));}
 }
 void _fail(Object e){if(mounted)setState(()=>_error=e.toString());}
 Future<void> _reload()async{
  final data=jsonDecode(await widget.project.surveyTracks(layerId:widget.layer,after:'',expected:_revision)) as List;
  if(mounted)setState(()=>_tracks=data.cast<Map<String,dynamic>>());
 }
 Future<void> _recover()async{
  try{
   final c=jsonDecode(await widget.project.catalog()) as Map;
   _revision=await widget.project.recoverSurveys(expected:c['revision'].toString());
   await _reload();
  }catch(e){_fail(e);}finally{if(mounted)setState(()=>_busy=false);}
 }
 void _receipt(String raw){final v=jsonDecode(raw) as Map;_revision=v['revision'].toString();}
 Future<bool> _permission()async{
  if(!await Geolocator.isLocationServiceEnabled())return false;
  var p=await Geolocator.checkPermission();
  if(p==LocationPermission.denied)p=await Geolocator.requestPermission();
  return p==LocationPermission.whileInUse||p==LocationPermission.always;
 }
 Future<void> _start(String? track)async{
  if(_busy||_recording||_stopping!=null||!_foreground)return;
  setState((){_busy=true;_error=null;});
  try{
   if(!await _permission()||!mounted||!_foreground)throw StateError('LOCATION_UNAVAILABLE');
   final id=track??widget.uuid();
   if(track==null){
    _receipt(await widget.project.createSurvey(layerId:widget.layer,trackId:id,name:'GPS ${DateTime.now().toUtc().toIso8601String()}',expected:_revision));
   }
   if(!mounted||!_foreground)return;
   _receipt(await widget.project.surveyState(layerId:widget.layer,trackId:id,action:'resume',expected:_revision));
   _track=id;_recording=true;final epoch=++_epoch;_saved=0;_skipped=0;
   if(!mounted||!_foreground){await _stop(false);return;}
   _stream=Geolocator.getPositionStream(locationSettings:const LocationSettings(accuracy:LocationAccuracy.high,distanceFilter:3)).listen((p){
    if(!mounted||!_foreground||!_recording||epoch!=_epoch)return;
    if(_write!=null){setState(()=>_skipped++);return;}
    if(DateTime.now().difference(p.timestamp).abs()>const Duration(seconds:30)){setState(()=>_skipped++);return;}
    _write=_append(id,p,epoch).whenComplete(()=>_write=null);
   },onError:(Object e){_fail(e);unawaited(_stop(false));},onDone:(){if(_recording)unawaited(_stop(false));});
   await _reload();
  }catch(e){_fail(e);if(_recording)await _stop(false);}
  finally{if(mounted&&_stopping==null)setState(()=>_busy=false);}
 }
 Future<void> _append(String id,Position p,int epoch)async{
  try{
   final raw=jsonEncode({'lon':p.longitude,'lat':p.latitude,'accuracy_m':p.accuracy.isFinite&&p.accuracy>0?p.accuracy:null,'altitude_m':p.altitude.isFinite?p.altitude:null,'utc_ms':p.timestamp.millisecondsSinceEpoch,'provider':'os','quality':'unknown','satellites':null});
   _receipt(await widget.project.appendSurvey(layerId:widget.layer,trackId:id,fixJson:raw,expected:_revision));
   if(mounted&&epoch==_epoch)setState((){_last=p;_saved++;});
  }catch(e){_fail(e);scheduleMicrotask(()=>unawaited(_stop(false)));}
 }
 Future<void> _stop(bool finish){
  final current=_stopping;if(current!=null)return current;
  if(!_recording||_track==null)return Future<void>.value();
  _recording=false;_epoch++;
  if(mounted)setState(()=>_busy=true);
  final id=_track!;
  final future=_doStop(id,finish);
  _stopping=future;
  return future.whenComplete((){_stopping=null;if(mounted)setState(()=>_busy=false);});
 }
 Future<void> _doStop(String id,bool finish)async{
  try{
   final s=_stream;_stream=null;await s?.cancel();await _write;
   // Re-read authoritative revision after the final fix.
   final c=jsonDecode(await widget.project.catalog()) as Map;_revision=c['revision'].toString();
   _receipt(await widget.project.surveyState(layerId:widget.layer,trackId:id,action:finish?'finish':'pause',expected:_revision));
   await _reload();
  }catch(e){_fail(e);}
 }
 Future<void> _finishPaused(String id)async{
  if(_busy||_recording)return;setState(()=>_busy=true);
  try{_receipt(await widget.project.surveyState(layerId:widget.layer,trackId:id,action:'finish',expected:_revision));await _reload();}
  catch(e){_fail(e);}finally{if(mounted)setState(()=>_busy=false);}
 }
 @override Widget build(BuildContext context)=>PopScope(canPop:!_busy&&!_recording&&_stopping==null,child:Scaffold(
 appBar:AppBar(title:const Text('Unified GPS Survey')),
 body:ListView(padding:const EdgeInsets.all(16),children:[
  const Text('Chỉ ghi GPS khi màn hình khảo sát đang mở và ứng dụng ở tiền cảnh. Không tự tiếp tục hoặc tải vị trí lên máy chủ.'),
  Text('Revision $_revision · $_saved fix đã ghi trong phiên · $_skipped fix bỏ qua'),
  if(_error!=null)Text(_error!),if(_busy)const LinearProgressIndicator(),
  FilledButton(onPressed:_busy||_recording?null:()=>unawaited(_start(null)),child:const Text('Bắt đầu khảo sát')),
  if(_recording)Wrap(children:[TextButton(onPressed:()=>unawaited(_stop(false)),child:const Text('Tạm dừng')),TextButton(onPressed:()=>unawaited(_stop(true)),child:const Text('Kết thúc'))]),
  if(_last!=null&&_foreground)Text('XY: ${_last!.longitude}, ${_last!.latitude} · độ chính xác OS: ${_last!.accuracy} m · RTK chưa xác định'),
  if(!_recording)OutlinedButton(onPressed:_busy?null:(){setState(()=>_busy=true);unawaited(_recover());},child:const Text('Phục hồi trạng thái tạm dừng')),
  for(final t in _tracks)ListTile(title:Text(t['name'].toString()),subtitle:Text('${t["state"]} · ${t["segment"]} đoạn'),trailing:t['state']=='paused'?Wrap(children:[TextButton(onPressed:_busy||_recording?null:()=>unawaited(_start(t['id'].toString())),child:const Text('Tiếp tục')),TextButton(onPressed:_busy||_recording?null:()=>unawaited(_finishPaused(t['id'].toString())),child:const Text('Kết thúc'))]):null)
 ])));
}
