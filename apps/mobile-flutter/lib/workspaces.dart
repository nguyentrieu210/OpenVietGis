import 'dart:io';
import 'dart:convert';
import 'dart:typed_data';
import 'package:material_ui/material_ui.dart';
import 'package:path_provider/path_provider.dart';
import 'package:shared_preferences/shared_preferences.dart';
import 'src/rust/api/simple.dart';
import 'src/rust/frb_generated.dart';

/// Only workspace selection and language are UI preferences. Features are Rust-owned.
class WorkspaceController extends ChangeNotifier{
  bool busy=true,ready=false,vi=true;
  String? selected,error;
  List<String> ids=[];
  String? _root;
  SharedPreferences? _prefs;
  NativeWorkspace? _workspace;
  bool _disposed=false;
  int dataEpoch=0;
  void _notify(){if(!_disposed)notifyListeners();}
  Future<void> boot()async{
    try{
      await RustLib.init();
      final support=await getApplicationSupportDirectory();
      _root='${support.path}${Platform.pathSeparator}local-workspaces';
      _prefs=await SharedPreferences.getInstance();
      vi=_prefs!.getBool('vi')??true;
      ids=await listLocalWorkspaces(rootPath:_root!);
      if(ids.isEmpty){ids=[await createLocalWorkspace(rootPath:_root!)];}
      final saved=_prefs!.getString('selected-local-workspace');
      final next=saved!=null&&ids.contains(saved)?saved:ids.first;
      await _open(next);
      ready=true;
    }catch(_){error='LOCAL_STORAGE_UNAVAILABLE';}
    finally{busy=false;_notify();}
  }
  Future<void> _open(String id)async{
    final next=await openLocalWorkspace(rootPath:_root!,workspaceId:id);
    if(_disposed){await next.close();return;}
    final previous=_workspace;
    _workspace=next;selected=id;dataEpoch++;
    if(previous!=null)await previous.close();
    await _prefs!.setString('selected-local-workspace',id);
  }
  Future<void> select(String id)async{
    if(busy||!ready||!ids.contains(id)||id==selected)return;
    busy=true;error=null;_notify();
    try{await _open(id);}catch(_){error='OPEN_WORKSPACE_FAILED';}
    finally{busy=false;_notify();}
  }
  Future<void> create()async{
    if(busy||!ready)return;
    busy=true;error=null;_notify();
    try{
      final id=await createLocalWorkspace(rootPath:_root!);
      ids=await listLocalWorkspaces(rootPath:_root!);
      await _open(id);
    }catch(_){error='CREATE_WORKSPACE_FAILED';}
    finally{busy=false;_notify();}
  }
  Future<String> viewport(double west,double south,double east,double north)async{
    final w=_workspace;
    if(w==null)throw StateError('NO_LOCAL_WORKSPACE');
    return w.viewportGeojson(west:west,south:south,east:east,north:north);
  }
  Future<int> importGeojson(Uint8List bytes)async{
    final w=_workspace;if(w==null||busy||!ready)throw StateError('WORKSPACE_UNAVAILABLE');
    busy=true;error=null;_notify();
    try{final n=await w.importGeojson(bytes:bytes);dataEpoch++;return n;}
    finally{busy=false;_notify();}
  }
  Future<String> exportGeojson()async{
    final w=_workspace;if(w==null||busy||!ready)throw StateError('WORKSPACE_UNAVAILABLE');
    return w.exportGeojson();
  }
  NativeWorkspace get local{final w=_workspace;if(w==null||!ready)throw StateError('NO_WORKSPACE');return w;}
  Future<String> draw(String kind,List<double> xy,Map<String,dynamic> attributes)async{
    if(busy)throw StateError('BUSY');final w=local;busy=true;_notify();
    try{final id=await w.commitDrawing(kind:kind,xy:Float64List.fromList(xy),attributesJson:jsonEncode(attributes));dataEpoch++;return id;}
    finally{busy=false;_notify();}
  }
  Future<void> updateAttributes(String id,String revision,String attributes)async{
    if(busy)throw StateError('BUSY');final w=local;busy=true;_notify();
    try{await w.updateAttributes(featureId:id,expectedRevision:revision,attributesJson:attributes);dataEpoch++;}
    finally{busy=false;_notify();}
  }
  Future<void> history(bool redo)async{
    if(busy)return;final w=local;busy=true;_notify();
    try{if(await w.undoRedo(redo:redo))dataEpoch++;}
    finally{busy=false;_notify();}
  }
  Future<void> toggleLanguage()async{vi=!vi;_notify();await _prefs?.setBool('vi',vi);}
  @override
  void dispose(){_disposed=true;_workspace?.close();super.dispose();}
}
