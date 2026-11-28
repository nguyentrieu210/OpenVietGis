import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'package:path_provider/path_provider.dart';
import 'package:share_plus/share_plus.dart';
import 'package:material_ui/material_ui.dart';
import 'package:geolocator/geolocator.dart';
import 'workspaces.dart';

class SurveyPage extends StatefulWidget{
 final WorkspaceController controller;
 const SurveyPage({super.key,required this.controller});
 @override State<SurveyPage> createState()=>_SurveyPageState();
}
class _SurveyPageState extends State<SurveyPage> with WidgetsBindingObserver{
 Position? _fix;
 String? _track;
 List<Map<String,dynamic>> _tracks=[];
 StreamSubscription<Position>? _subscription;
 bool _recording=false,_foreground=true,_writing=false;
 int _trackEpoch=0;
 String? _error;
 bool _busy=false;
 @override void initState(){super.initState();WidgetsBinding.instance.addObserver(this);unawaited(_restore());}
 @override void dispose(){_foreground=false;_trackEpoch++;unawaited(_stop(false));WidgetsBinding.instance.removeObserver(this);super.dispose();}
 Future<void> _read()async{
   if(_busy)return;
   setState((){_busy=true;_error=null;});
   try{
     if(!await Geolocator.isLocationServiceEnabled())throw StateError('GPS_DISABLED');
     var permission=await Geolocator.checkPermission();
     if(permission==LocationPermission.denied)permission=await Geolocator.requestPermission();
     if(permission!=LocationPermission.whileInUse&&permission!=LocationPermission.always)throw StateError('LOCATION_PERMISSION_DENIED');
     final p=await Geolocator.getCurrentPosition(locationSettings:const LocationSettings(accuracy:LocationAccuracy.high,timeLimit:Duration(seconds:20)));
     if(mounted)setState(()=>_fix=p);
   }catch(e){if(mounted)setState(()=>_error=e.toString());}
   finally{if(mounted)setState(()=>_busy=false);}
 }
 @override void didChangeAppLifecycleState(AppLifecycleState state){
   _foreground=state==AppLifecycleState.resumed;
   if(!_foreground){_trackEpoch++;unawaited(_stop(false));if(mounted)setState(()=>_fix=null);}
 }
 Future<void> _save()async{
   final p=_fix;if(p==null||_busy)return;
   setState(()=>_busy=true);
   try{
     if(DateTime.now().difference(p.timestamp).abs()>const Duration(seconds:30))throw StateError('STALE_FIX');
     await widget.controller.draw('point',[p.longitude,p.latitude],{
       'survey_provider':'os','fix_quality':'unknown','accuracy_m':p.accuracy,
       'altitude_m':p.altitude,'utc':p.timestamp.toUtc().toIso8601String(),
     });
     if(mounted)ScaffoldMessenger.of(context).showSnackBar(const SnackBar(content:Text('Waypoint saved')));
   }catch(e){if(mounted)setState(()=>_error=e.toString());}
   finally{if(mounted)setState(()=>_busy=false);}
 }
 Future<void> _restore()async{
   try{await widget.controller.local.recoverTracks();await _reload();}
   catch(e){if(mounted)setState(()=>_error=e.toString());}
 }
 Future<void> _reload()async{
   final list=jsonDecode(await widget.controller.local.listTracks()) as List;
   if(mounted)setState(()=>_tracks=list.cast<Map<String,dynamic>>());
 }
 Future<bool> _permission()async{
   if(!await Geolocator.isLocationServiceEnabled())return false;
   var p=await Geolocator.checkPermission();
   if(p==LocationPermission.denied)p=await Geolocator.requestPermission();
   return p==LocationPermission.whileInUse||p==LocationPermission.always;
 }
 Future<void> _start(String? id)async{
   if(_recording||_busy||!_foreground)return;
   setState(()=>_busy=true);
   try{
     if(!await _permission()||!mounted||!_foreground)throw StateError('LOCATION_PERMISSION_DENIED');
     final track=id??await widget.controller.local.createTrack(name:'Track ${DateTime.now().toUtc().toIso8601String()}');
     await widget.controller.local.trackState(trackId:track,action:'resume');
     _track=track;_recording=true;final epoch=++_trackEpoch;
     if(!_foreground||!mounted){await _stop(false);return;}
     _subscription=Geolocator.getPositionStream(locationSettings:const LocationSettings(accuracy:LocationAccuracy.high,distanceFilter:3)).listen((p){
       if(!mounted||!_foreground||epoch!=_trackEpoch||_writing)return;
       _writing=true;unawaited(_append(track,p,epoch));
     },onError:(Object e){if(mounted)setState(()=>_error=e.toString());unawaited(_stop(false));});
     await _reload();
   }catch(e){if(mounted)setState(()=>_error=e.toString());await _stop(false);}
   finally{if(mounted)setState(()=>_busy=false);}
 }
 Future<void> _append(String id,Position p,int epoch)async{
   try{
     if(DateTime.now().difference(p.timestamp).abs()>const Duration(seconds:30))return;
     await widget.controller.local.appendFix(trackId:id,fixJson:jsonEncode({
       'lon':p.longitude,'lat':p.latitude,'accuracy_m':p.accuracy,'altitude_m':p.altitude,
       'utc_ms':p.timestamp.millisecondsSinceEpoch,'provider':'os','quality':'unknown','satellites':null
     }));
     if(mounted&&epoch==_trackEpoch)setState(()=>_fix=p);
   }catch(e){if(mounted)setState(()=>_error=e.toString());unawaited(_stop(false));}
   finally{_writing=false;}
 }
 Future<void> _stop(bool finish)async{
   final track=_track;
   if(track==null||!_recording)return;
   _recording=false;_trackEpoch++;
   final sub=_subscription;_subscription=null;
   await sub?.cancel();
   try{await widget.controller.local.trackState(trackId:track,action:finish?'finish':'pause');if(mounted)await _reload();}
   catch(e){if(mounted)setState(()=>_error=e.toString());}
   if(mounted)setState((){});
 }
 Future<void> _exportTrack(String id)async{
   final allow=await showDialog<bool>(context:context,builder:(d)=>AlertDialog(title:const Text('Export GPX'),content:Text(widget.controller.vi?'Chia sẻ vị trí và thời gian track. GPX này không giữ metadata độ chính xác/RTK.':'Share track locations and timestamps. This GPX omits accuracy/RTK metadata.'),actions:[TextButton(onPressed:()=>Navigator.pop(d,false),child:const Text('Cancel')),TextButton(onPressed:()=>Navigator.pop(d,true),child:const Text('Export'))]));
   if(allow!=true||!mounted)return;
   try{final xml=await widget.controller.local.trackGpx(trackId:id);final dir=await (await getTemporaryDirectory()).createTemp('ovg-gpx-');
     final file=await File('${dir.path}/track.gpx').writeAsString(xml,flush:true);await Share.shareXFiles([XFile(file.path,mimeType:'application/gpx+xml')]);
   }catch(e){if(mounted)setState(()=>_error=e.toString());}
 }
 @override Widget build(BuildContext context){
   final vi=widget.controller.vi;final p=_fix;
   return Scaffold(appBar:AppBar(title:Text(vi?'Khảo sát GPS':'GPS survey')),body:ListView(padding:const EdgeInsets.all(16),children:[
     Text(vi?'Chỉ lấy vị trí khi yêu cầu. Track dừng khi rời ứng dụng; không ghi nền hoặc gửi lên máy chủ.':'Location is requested explicitly. Tracks pause on leaving the app; no background recording or upload.'),
     if(_error!=null)Text(_error!),
     FilledButton(onPressed:_busy?null:_read,child:Text(vi?'Lấy vị trí':'Get fix')),
     const Divider(),
     FilledButton(onPressed:_busy||_recording?null:()=>_start(null),child:Text(vi?'Bắt đầu track (màn hình mở)':'Start foreground track')),
     if(_recording)Row(children:[TextButton(onPressed:()=>_stop(false),child:Text(vi?'Tạm dừng':'Pause')),TextButton(onPressed:()=>_stop(true),child:Text(vi?'Kết thúc':'Finish'))]),
     for(final track in _tracks)ListTile(title:Text(track['name'].toString()),subtitle:Text(track['state'].toString()),trailing:track['state']=='paused'?TextButton(onPressed:_busy||_recording?null:()=>_start(track['id'].toString()),child:Text(vi?'Tiếp tục':'Resume')):track['state']=='finished'?IconButton(tooltip:'GPX',icon:const Icon(Icons.share),onPressed:()=>_exportTrack(track['id'].toString())):null),
     if(p!=null)...[
       Text('XY: ${p.longitude.toStringAsFixed(7)}, ${p.latitude.toStringAsFixed(7)}'),
       Text('Accuracy: ${p.accuracy.toStringAsFixed(1)} m · OS fix, RTK unknown'),
       Text(jsonEncode({'time':p.timestamp.toUtc().toIso8601String(),'altitude_m':p.altitude})),
       FilledButton(onPressed:_busy?null:_save,child:Text(vi?'Lưu điểm khảo sát':'Save waypoint'))
     ],
   ]));
 }
}
