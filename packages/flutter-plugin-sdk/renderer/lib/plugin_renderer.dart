/// Trusted host widgets. Only reviewed runtime data is accepted. No dynamic
/// widget factory, URL launch, embedded HTML, filesystem or native bridge.
library;
import 'package:material_ui/material_ui.dart';
import 'package:openvietgis_plugin_sdk/openvietgis_plugin_sdk.dart';
import 'dart:convert';

class PluginPanel extends StatefulWidget{
 final Runtime runtime;
 final Broker broker;
 const PluginPanel({super.key,required this.runtime,required this.broker});
 @override State<PluginPanel> createState()=>_PluginPanelState();
}
class _PluginPanelState extends State<PluginPanel>{
 final _values=<String,TextEditingController>{};
 bool _busy=false;
 String? _result;
 bool get _supported{
   bool allowed(Node n)=>['text','input_text','input_number','input_date','form'].contains(n.kind)&&n.children.every(allowed);
   return widget.runtime.nodes.every(allowed);
 }
 @override void dispose(){widget.broker.close();for(final c in _values.values){c.dispose();}super.dispose();}
 @override void didUpdateWidget(covariant PluginPanel oldWidget){
   super.didUpdateWidget(oldWidget);
   if(oldWidget.broker!=widget.broker||oldWidget.runtime!=widget.runtime){
     oldWidget.broker.close();for(final c in _values.values){c.dispose();}_values.clear();_result=null;_busy=false;
   }
 }
 TextEditingController _control(String id)=>_values.putIfAbsent(id,TextEditingController.new);
 Widget _node(Node n)=>switch(n.kind){
   'text'=>Padding(padding:const EdgeInsets.symmetric(vertical:8),child:Text(n.label)),
   'form'=>Column(crossAxisAlignment:CrossAxisAlignment.stretch,children:[Text(n.label),for(final child in n.children)_node(child)]),
   'input_text'||'input_number'||'input_date'=>Padding(padding:const EdgeInsets.symmetric(vertical:8),child:TextField(key:ValueKey(n.id),controller:_control(n.id),maxLength:8192,keyboardType:n.kind=='input_number'?const TextInputType.numberWithOptions(decimal:true,signed:true):TextInputType.text,decoration:InputDecoration(labelText:n.label))),
   _=>const SizedBox.shrink(),
 };
 Future<void> _invoke(Action action)async{
  if(_busy)return;
  final broker=widget.broker;setState((){_busy=true;_result=null;});
  try{
    final args=<String,dynamic>{};
    for(final e in action.input.entries){
     final text=_values[e.key]?.text;
     if(text==null)throw StateError('INPUT_NOT_BOUND');
     args[e.key]=switch(e.value){'text'||'uuid'||'date'=>text,'integer'=>int.parse(text),'number'=>double.parse(text),'boolean'=>switch(text){'true'=>true,'false'=>false,_=>throw StateError('INVALID_BOOLEAN')},'uuid_list'=>text.split(',').map((s)=>s.trim()).toList(),_=>throw StateError('UNSUPPORTED_INPUT')};
    }
    final result=await broker.invoke(action.id,args);
    if(mounted&&identical(broker,widget.broker))setState(()=>_result=jsonEncode(result));
  }catch(_){if(mounted&&identical(broker,widget.broker))setState(()=>_result='Action denied or failed');}
  finally{if(mounted&&identical(broker,widget.broker))setState(()=>_busy=false);}
 }
 @override Widget build(BuildContext context){
  if(!_supported)return const Text('This contribution requires an unsupported renderer. Not activated.');
  return ListView(padding:const EdgeInsets.all(16),children:[
   for(final node in widget.runtime.nodes)_node(node),
   for(final action in widget.runtime.actions.values)FilledButton(onPressed:_busy?null:()=>_invoke(action),child:Text(action.id)),
   if(_busy)const LinearProgressIndicator(),
   if(_result!=null)SelectableText(_result!),
  ]);
 }
}
