/// Candidate renderer for SPEC075; uses existing SPEC073 renderer package.
/// Host must supply a qualified UniversalSession; no local/native execution.
import 'dart:convert';
import 'package:material_ui/material_ui.dart';
import 'package:openvietgis_plugin_sdk/src/universal_document.dart';
import 'package:openvietgis_plugin_sdk/src/universal_session.dart';

class UniversalPanel extends StatefulWidget {
  final UniversalSession session;
  const UniversalPanel({super.key,required this.session});
  @override State<UniversalPanel> createState()=>_UniversalPanelState();
}
class _UniversalPanelState extends State<UniversalPanel>{
  final _values=<String,TextEditingController>{};
  bool _busy=false;
  String? _output;
  void _clear(){for(final c in _values.values){c.dispose();}_values.clear();_busy=false;_output=null;}
  @override void dispose(){widget.session.close();_clear();super.dispose();}
  @override void didUpdateWidget(covariant UniversalPanel oldWidget){
    super.didUpdateWidget(oldWidget);
    if(oldWidget.session!=widget.session){oldWidget.session.close();_clear();}
  }
  Future<void> _invoke(String id)async{
    if(_busy)return;
    final session=widget.session;
    setState((){_busy=true;_output=null;});
    try{
      final result=await session.invoke(id,{for(final e in _values.entries)e.key:e.value.text});
      if(mounted&&identical(session,widget.session))setState(()=>_output=jsonEncode(result));
    }catch(_){if(mounted&&identical(session,widget.session))setState(()=>_output='Không thể thực hiện lệnh / Action denied');}
    finally{if(mounted&&identical(session,widget.session))setState(()=>_busy=false);}
  }
  Widget _node(UniversalNode n)=>switch(n.kind){
    'text'=>Text(n.label,key:ValueKey(n.id)),
    'input'=>TextField(key:ValueKey(n.id),controller:_values.putIfAbsent(n.binding!,TextEditingController.new),
      maxLength:4096,decoration:InputDecoration(labelText:n.label)),
    'button'=>FilledButton(key:ValueKey(n.id),onPressed:_busy?null:()=>_invoke(n.binding!),child:Text(n.label)),
    'group'=>Column(key:ValueKey(n.id),crossAxisAlignment:CrossAxisAlignment.stretch,children:n.children.map(_node).toList()),
    _=>const SizedBox.shrink(),
  };
  @override Widget build(BuildContext context)=>ListView(padding:const EdgeInsets.all(16),children:[
    const Text('Plugin đọc-only / Read-only plugin'),
    for(final node in widget.session.document.widgets)_node(node),
    if(_busy)const LinearProgressIndicator(),
    if(_output!=null)SelectableText(_output!),
  ]);
}
