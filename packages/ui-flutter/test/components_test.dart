import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:openvietgis_ui/openvietgis_ui.dart';
void main(){
 testWidgets('busy and disabled buttons', (tester)async{
  var count=0;
  await tester.pumpWidget(MaterialApp(theme:ovgNativeTheme(Brightness.light),home:Scaffold(body:OvgButton(label:'Save',onPressed:()=>count++))));
  await tester.tap(find.text('Save'));expect(count,1);
  await tester.pumpWidget(MaterialApp(home:Scaffold(body:OvgButton(label:'Save',busy:true,onPressed:()=>count++))));
  expect(tester.widget<FilledButton>(find.byType(FilledButton)).onPressed,isNull);
 });
 testWidgets('layer visibility callback', (tester)async{
  bool? changed;
  await tester.pumpWidget(MaterialApp(home:Scaffold(body:OvgLayerTile(label:'Roads',visible:false,onVisibility:(v)=>changed=v))));
  await tester.tap(find.byType(Checkbox));expect(changed,true);
 });
}