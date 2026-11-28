import 'package:flutter/material.dart';
import 'package:openvietgis_design_tokens/openvietgis_design_tokens.dart';
export 'package:openvietgis_design_tokens/openvietgis_design_tokens.dart';
class OvgButton extends StatelessWidget {
 final String label; final VoidCallback? onPressed; final bool busy;
 const OvgButton({super.key,required this.label,this.onPressed,this.busy=false});
 @override Widget build(BuildContext context)=>FilledButton(onPressed:busy?null:onPressed,child:busy?const SizedBox(width:18,height:18,child:CircularProgressIndicator(strokeWidth:2)):Text(label));
}
class OvgField extends StatelessWidget {
 final String label;final TextEditingController controller;final String? error;final bool readOnly;final TextInputType? keyboardType;
 const OvgField({super.key,required this.label,required this.controller,this.error,this.readOnly=false,this.keyboardType});
 @override Widget build(BuildContext context)=>TextField(controller:controller,readOnly:readOnly,keyboardType:keyboardType,decoration:InputDecoration(labelText:label,errorText:error));
}
class OvgLayerTile extends StatelessWidget {
 final String label;final bool visible;final bool selected;final ValueChanged<bool>? onVisibility;final VoidCallback? onSelect;
 const OvgLayerTile({super.key,required this.label,required this.visible,this.selected=false,this.onVisibility,this.onSelect});
 @override Widget build(BuildContext context)=>ListTile(title:Text(label),selected:selected,onTap:onSelect,leading:Checkbox(value:visible,onChanged:onVisibility==null?null:(v){if(v!=null)onVisibility!(v);}));
}
class OvgFieldStatus extends StatelessWidget {
 final bool offline;final String gpsLabel;final String syncLabel;
 const OvgFieldStatus({super.key,required this.offline,required this.gpsLabel,required this.syncLabel});
 @override Widget build(BuildContext context)=>Wrap(spacing:8,runSpacing:8,children:[Chip(label:Text(offline?'Offline':'Online')),Chip(label:Text(gpsLabel)),Chip(label:Text(syncLabel))]);
}
class OvgAdaptiveWorkspace extends StatelessWidget {
 final Widget map;final Widget tools;final String toolsLabel;
 const OvgAdaptiveWorkspace({super.key,required this.map,required this.tools,this.toolsLabel='Tools'});
 @override Widget build(BuildContext context)=>LayoutBuilder(builder:(context,c)=>c.maxWidth>=840?Row(children:[Expanded(child:map),SizedBox(width:320,child:tools)]):Stack(children:[Positioned.fill(child:map),Positioned(bottom:16,right:16,child:OvgButton(label:toolsLabel,onPressed:()=>showModalBottomSheet<void>(context:context,isScrollControlled:true,builder:(context)=>SafeArea(child:SizedBox(height:MediaQuery.sizeOf(context).height*.75,child:tools)))))]));
}
ThemeData ovgNativeTheme(Brightness brightness)=>OvgTokens.theme(brightness,density:OvgDensity.touch);