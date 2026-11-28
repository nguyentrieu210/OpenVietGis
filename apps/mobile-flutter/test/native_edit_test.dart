import 'dart:io';
import 'dart:convert';
import 'dart:typed_data';
import 'package:flutter_test/flutter_test.dart';
import 'package:openvietgis_mobile/src/rust/api/simple.dart';
import 'package:openvietgis_mobile/src/rust/frb_generated.dart';
void main(){
  setUpAll(()async{await RustLib.init();});
  tearDownAll(()=>RustLib.dispose());
  test('preview is nonmutating and committed geometry survives history',()async{
    final dir=await Directory.systemTemp.createTemp('ovg-edit-');
    try{
      final id=await createLocalWorkspace(rootPath:dir.path);
      final w=await openLocalWorkspace(rootPath:dir.path,workspaceId:id);
      final xy=Float64List.fromList([106,10,106.01,10.01]);
      expect(await w.previewEdit(kind:'line',xy:xy),isNotEmpty);
      expect(jsonDecode(await w.attributePage(after:'0',limit:100,contains:'')),isEmpty);
      final feature=await w.commitDrawing(kind:'line',xy:xy,attributesJson:'{"name":"test"}');
      final row=jsonDecode(await w.featureJson(featureId:feature)) as Map;
      expect(row['revision'],1);
      expect(await w.undoRedo(redo:false),true);
      expect(jsonDecode(await w.attributePage(after:'0',limit:100,contains:'')),isEmpty);
      expect(await w.undoRedo(redo:true),true);
      final restored=jsonDecode(await w.featureJson(featureId:feature)) as Map;
      expect(restored['revision'],3);
      await w.close();
    }finally{await dir.delete(recursive:true);}
  });
}
