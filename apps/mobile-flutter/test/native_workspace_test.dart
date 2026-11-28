import 'dart:io';
import 'package:flutter_test/flutter_test.dart';
import 'package:openvietgis_mobile/src/rust/api/simple.dart';
import 'package:openvietgis_mobile/src/rust/frb_generated.dart';

void main(){
  setUpAll(() async { await RustLib.init(); });
  tearDownAll(() { RustLib.dispose(); });
  test('native workspace create, reopen and reject stale/foreign handles',()async{
    final dir=await Directory.systemTemp.createTemp('ovg-w1-');
    try{
      final id=await createLocalWorkspace(rootPath:dir.path);
      expect(await listLocalWorkspaces(rootPath:dir.path),[id]);
      final first=await openLocalWorkspace(rootPath:dir.path,workspaceId:id);
      await first.checkpoint();
      await first.close();
      await expectLater(first.checkpoint(),throwsA(anything));
      final reopened=await openLocalWorkspace(rootPath:dir.path,workspaceId:id);
      await reopened.checkpoint();
      await reopened.close();
      await expectLater(openLocalWorkspace(rootPath:dir.path,workspaceId:'../outside'),throwsA(anything));
    }finally{await dir.delete(recursive:true);}
  });
}
