import 'dart:async';
import 'dart:convert';
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
 String? _error;
 bool _busy=false;
 @override void initState(){super.initState();WidgetsBinding.instance.addObserver(this);}
 @override void dispose(){WidgetsBinding.instance.removeObserver(this);super.dispose();}
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
   if(state!=AppLifecycleState.resumed&&mounted)setState(()=>_fix=null);
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
 @override Widget build(BuildContext context){
   final vi=widget.controller.vi;final p=_fix;
   return Scaffold(appBar:AppBar(title:Text(vi?'Khảo sát GPS':'GPS survey')),body:ListView(padding:const EdgeInsets.all(16),children:[
     Text(vi?'Vị trí chỉ được lấy khi yêu cầu. Không ghi nền hoặc gửi lên máy chủ.':'Location is requested explicitly. No background tracking or server upload.'),
     if(_error!=null)Text(_error!),
     FilledButton(onPressed:_busy?null:_read,child:Text(vi?'Lấy vị trí':'Get fix')),
     if(p!=null)...[
       Text('XY: ${p.longitude.toStringAsFixed(7)}, ${p.latitude.toStringAsFixed(7)}'),
       Text('Accuracy: ${p.accuracy.toStringAsFixed(1)} m · OS fix, RTK unknown'),
       Text(jsonEncode({'time':p.timestamp.toUtc().toIso8601String(),'altitude_m':p.altitude})),
       FilledButton(onPressed:_busy?null:_save,child:Text(vi?'Lưu điểm khảo sát':'Save waypoint'))
     ],
   ]));
 }
}
