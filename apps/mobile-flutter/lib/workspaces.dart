import 'dart:io';
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
    _workspace=next;selected=id;
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
  Future<void> toggleLanguage()async{vi=!vi;_notify();await _prefs?.setBool('vi',vi);}
  @override
  void dispose(){_disposed=true;_workspace?.close();super.dispose();}
}
